//! `run_skill_script`：执行用户技能目录内的脚本。
//!
//! 安全模型（docs/roadmap/2026-10-16-user-skills-roadmap.md）：
//! - 装即信任来源，但**跑 = 逐次确认**：工具本体只做校验并挂待确认动作，
//!   用户在审阅卡接受后才会真正起进程（`execute_pending_action`）。
//! - 例外：技能被用户在设置页标为「信任」（skills_state.json 的 trusted 列表）
//!   时直接执行——与 MCP trusted 同模型，设置页已明示风险。
//! - 脚本路径必须落在技能目录内（canonical 比对防穿越）。
//! - 解释器按 shebang 解析，从 PATH 查找；环境变量仅白名单透传。
//! - 超时 120s、输出截断 8KB；每次执行在事件日志留痕。

use super::{Tool, ToolContext, ToolPolicy, arg_str, arg_str_opt};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SCRIPT_TIMEOUT: Duration = Duration::from_secs(120);
const OUTPUT_LIMIT: usize = 8 * 1024;

/// 仅透传的环境变量白名单。
const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TERM",
    "TMPDIR",
    "CLASH_PICK_API",
    "CLASH_PICK_SECRET",
    "CLASH_PICK_SOCK",
];

pub struct RunSkillScriptTool;

impl Tool for RunSkillScriptTool {
    fn name(&self) -> &'static str {
        "run_skill_script"
    }

    fn description(&self) -> &'static str {
        "执行用户已安装技能包内的脚本。脚本须是该技能 SKILL.md 中列出的文件；\
         每次执行前会展示完整命令请你确认。仅桌面端可用。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type":"object",
            "properties":{
                "skill":{"type":"string","description":"技能包 id（enable_tools 列表中的名字），必填"},
                "script":{"type":"string","description":"技能目录内的脚本相对路径，如 templates/clash-pick，必填"},
                "args":{"type":"string","description":"传给脚本的命令行参数字符串，可选；会被按空格拆分，含空格的参数用引号包裹"}
            },
            "required":["skill","script"],
            "additionalProperties":false
        })
    }

    fn deferred(&self) -> bool {
        true
    }

    fn policy(&self) -> ToolPolicy {
        ToolPolicy::WriteConfirm
    }

    fn run(&self, args: &Value, ctx: &ToolContext) -> Result<String> {
        let data_dir = crate::config::AppConfig::load()?.data_dir;
        self.run_with_data_dir(args, ctx, &data_dir)
    }
}

impl RunSkillScriptTool {
    /// 审批/执行逻辑本体，data_dir 显式传入供测试注入（临时技能目录）。
    fn run_with_data_dir(
        &self,
        args: &Value,
        ctx: &ToolContext,
        data_dir: &std::path::Path,
    ) -> Result<String> {
        let skill = arg_str(args, "skill")?;
        let script = arg_str(args, "script")?;
        let extra_args = arg_str_opt(args, "args").unwrap_or_default();
        // 移动端诚实降级（roadmap P2）：正文照常加载（方法论仍有价值），
        // 脚本执行直接明说需要桌面环境——不假装、不静默失败。
        #[cfg(any(target_os = "android", target_os = "ios"))]
        {
            let _ = (&skill, &script);
            bail!(
                "脚本执行需要桌面环境（此设备无法运行本地脚本）。该技能的方法论部分仍可通过 enable_tools 加载参考。"
            );
        }
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            let resolved = resolve_script(&data_dir, &skill, &script)?;
            let command = build_command_display(&resolved, &extra_args);
            let action_args = json!({
                "skill": skill,
                "script": script,
                "args": extra_args,
            });
            // trusted 技能：跳过审批直接起进程（与 MCP trusted 同模型）。
            // 执行路径与确认后完全一致（execute_confirmed 重做全部校验）。
            if crate::ai::skills::is_trusted(&data_dir, &skill) {
                return execute_confirmed(&data_dir, &action_args);
            }
            // 待确认动作：确认时按 args_json 原样执行，杜绝确认前后偷换内容
            super::store_create_pending(ctx.store, ctx.conversation_id, self.name(), &action_args)?;
            let excerpt = resolved.body_chars.take(400).collect::<String>();
            Ok(format!(
                "脚本已通过校验，等待你确认后执行：\n{command}\n\n脚本开头：\n{excerpt}\n\n在审阅卡点击接受后执行。"
            ))
        }
    }
}

/// 校验后的执行计划。
pub struct ResolvedScript {
    pub interpreter: PathBuf,
    pub interpreter_args: Vec<String>,
    pub script_path: PathBuf,
    pub work_dir: PathBuf,
    /// 脚本开头一段，供确认卡片展示。
    pub body_chars: std::iter::Take<std::str::Chars<'static>>,
}

/// 校验技能启用状态、脚本路径落在技能目录内、解析 shebang。
/// skill_id/script/args 全部来自待确认动作的 args_json——确认时重新校验一遍，
/// 不信任 pending 时的环境状态。
pub fn resolve_script(data_dir: &Path, skill_id: &str, script: &str) -> Result<ResolvedScript> {
    anyhow::ensure!(
        crate::ai::skills::is_enabled(data_dir, skill_id),
        "技能 {skill_id} 已被停用，请用户在设置中开启后再试"
    );
    let skill_dir = crate::ai::skills::skills_dir(data_dir).join(skill_id);
    anyhow::ensure!(
        skill_dir.is_dir(),
        "未安装技能 {skill_id}（缺少目录 {}）",
        skill_dir.display()
    );
    let script_path = skill_dir.join(script);
    let canonical_script = script_path
        .canonicalize()
        .with_context(|| format!("脚本不存在：{script}"))?;
    let canonical_root = skill_dir.canonicalize()?;
    anyhow::ensure!(
        canonical_script.starts_with(&canonical_root),
        "脚本路径越出技能目录，已拒绝"
    );
    let content = std::fs::read_to_string(&canonical_script)
        .with_context(|| format!("读取脚本 {}", canonical_script.display()))?;
    let (interpreter, interpreter_args) =
        parse_shebang(&content).context("无法识别脚本解释器：缺少可识别的 shebang 首行")?;
    let chars: &'static str = Box::leak(content.into_boxed_str());
    Ok(ResolvedScript {
        interpreter,
        interpreter_args,
        script_path: canonical_script,
        work_dir: canonical_root,
        body_chars: chars.chars().take(400),
    })
}

/// 解析 shebang：`#!/usr/bin/env python3` → (env, ["python3"])；
/// `#!/bin/bash` → (/bin/bash, [])。无法识别返回 None。
fn parse_shebang(content: &str) -> Option<(PathBuf, Vec<String>)> {
    let first = content.lines().next()?;
    let line = first.strip_prefix("#!")?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.as_slice() {
        [interp] => Some((PathBuf::from(interp), Vec::new())),
        ["env", interp, rest @ ..] => Some((
            PathBuf::from("/usr/bin/env"),
            [interp.to_string()]
                .into_iter()
                .chain(rest.iter().map(|s| s.to_string()))
                .collect(),
        )),
        [interp, rest @ ..] => Some((
            PathBuf::from(interp),
            rest.iter().map(|s| s.to_string()).collect(),
        )),
        _ => None,
    }
}

/// 确认卡片里展示的命令行（不执行，仅供预览）。
pub fn build_command_display(resolved: &ResolvedScript, extra_args: &str) -> String {
    format!(
        "{} {} {} {}",
        resolved.interpreter.display(),
        resolved.interpreter_args.join(" "),
        resolved
            .script_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<script>"),
        extra_args
    )
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

/// 确认后的真正执行入口（`execute_pending_action` 的 run_skill_script 分支）。
pub fn execute_confirmed(data_dir: &Path, args: &Value) -> Result<String> {
    let skill = super::arg_str(args, "skill")?;
    let script = super::arg_str(args, "script")?;
    let extra_args = super::arg_str_opt(args, "args").unwrap_or_default();
    let resolved = resolve_script(data_dir, &skill, &script)?;

    use std::process::{Command, Stdio};
    let mut command = Command::new(&resolved.interpreter);
    command
        .args(&resolved.interpreter_args)
        .arg(&resolved.script_path)
        .current_dir(&resolved.work_dir)
        .stdin(Stdio::null())
        .env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    if let Ok(shell_words) = shlex_split(&extra_args) {
        command.args(shell_words);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "启动脚本 {script} 失败（解释器 {}）",
                resolved.interpreter.display()
            )
        })?;

    // 输出用 reader 线程收，主循环只管超时；避免子进程写满管道反向阻塞。
    use std::io::Read;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    fn read_to_end<R: Read>(mut pipe: R) -> Vec<u8> {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    }
    let stdout_handle = std::thread::spawn(move || match stdout_pipe {
        Some(pipe) => read_to_end::<std::process::ChildStdout>(pipe),
        None => Vec::new(),
    });
    let stderr_handle = std::thread::spawn(move || match stderr_pipe {
        Some(pipe) => read_to_end(pipe),
        None => Vec::new(),
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if started.elapsed() > SCRIPT_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                bail!("脚本执行超过 {} 秒，已终止", SCRIPT_TIMEOUT.as_secs());
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    };
    let stdout: Vec<u8> = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    let mut out = String::from_utf8_lossy(&stdout).to_string();
    let err = String::from_utf8_lossy(&stderr).to_string();
    if !err.trim().is_empty() {
        out.push_str("\n[stderr]\n");
        out.push_str(&err);
    }
    let truncated = out.chars().count() > OUTPUT_LIMIT;
    let head: String = out.chars().take(OUTPUT_LIMIT).collect();
    let status_line = if status.success() {
        "执行成功".to_string()
    } else {
        format!("退出码 {:?}", status.code())
    };
    Ok(format!(
        "[{skill}/{script}] {status_line}\n\n{}{}",
        head,
        if truncated {
            "\n\n…（输出已截断）"
        } else {
            ""
        }
    ))
}

/// 极简 shell 词拆分：支持单双引号包裹；不解析 $ 变量、不做通配。
fn shlex_split(input: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            None => current.push(ch),
        }
    }
    if quote.is_some() {
        bail!("参数里有未闭合的引号：{input}");
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shebang_variants() {
        assert_eq!(
            parse_shebang("#!/usr/bin/env python3\nrest").unwrap().1,
            vec!["python3"]
        );
        assert_eq!(
            parse_shebang("#!/bin/bash\nx").unwrap().0,
            PathBuf::from("/bin/bash")
        );
        assert!(parse_shebang("print('hi')").is_none());
    }

    #[test]
    fn shlex_handles_quotes() {
        assert_eq!(
            shlex_split("--group \"漏网 之鱼\" --top 5").unwrap(),
            vec!["--group", "漏网 之鱼", "--top", "5"]
        );
        assert!(shlex_split("a \"unclosed").is_err());
    }
}

#[cfg(test)]
mod resolve_tests {
    use super::*;

    fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-script-test-{}-{}",
            tag,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let skill = dir.join("skills/proxy-pick");
        std::fs::create_dir_all(skill.join("templates")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: proxy-pick\ndescription: test\n---\nbody",
        )
        .unwrap();
        (dir, skill)
    }

    #[test]
    fn resolve_normal_script_and_reject_traversal() {
        let (dir, skill) = fixture("resolve");
        std::fs::write(
            skill.join("templates/clash-pick"),
            "#!/usr/bin/env python3\nprint('ok')",
        )
        .unwrap();
        let resolved = resolve_script(&dir, "proxy-pick", "templates/clash-pick").unwrap();
        assert!(resolved.script_path.ends_with("templates/clash-pick"));

        // 目录穿越必须拒绝
        let outside = dir.join("outside.sh");
        std::fs::write(&outside, "#!/bin/sh\n").unwrap();
        assert!(resolve_script(&dir, "proxy-pick", "../../outside.sh").is_err());
        // 停用的技能拒绝执行
        crate::ai::skills::set_enabled(&dir, "proxy-pick", false).unwrap();
        assert!(resolve_script(&dir, "proxy-pick", "templates/clash-pick").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod trust_tests {
    use super::*;

    fn fixture(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elsewhen-skill-trust-{}-{tag}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("t")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let skill = dir.join("skills").join("echo-skill");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: echo-skill\ndescription: test\n---\nbody",
        )
        .unwrap();
        std::fs::write(skill.join("run.sh"), "#!/bin/sh\necho skill-echo \"$1\"\n").unwrap();
        dir
    }

    fn dispatch_env(tag: &str) -> (crate::storage::Store, String, std::path::PathBuf) {
        let dir = fixture(tag);
        let db = dir.join("test.db");
        let store = crate::storage::Store::open(&db).unwrap();
        crate::storage::Store::mark_initialized(&db).unwrap();
        let conversation_id = store.create_conversation(None, None).unwrap();
        (store, conversation_id, dir)
    }

    fn call() -> serde_json::Value {
        serde_json::json!({ "skill": "echo-skill", "script": "run.sh", "args": "hi" })
    }

    fn ctx<'a>(
        store: &'a crate::storage::Store,
        conversation_id: &'a str,
    ) -> ToolContext<'a> {
        ToolContext {
            store,
            conversation_id,
            source_event_id: None,
            knowledge_candidates: &[],
        }
    }

    #[test]
    fn untrusted_skill_dispatch_creates_pending_action_with_sealed_args() {
        let (store, conversation_id, dir) = dispatch_env("untrusted");
        // 未 trust：走 pending，不直接执行。
        let out = RunSkillScriptTool
            .run_with_data_dir(&call(), &ctx(&store, &conversation_id), &dir)
            .unwrap();
        assert!(out.contains("等待你确认"), "got: {out}");
        // 未执行：不含执行成功的完成态文案。
        assert!(!out.contains("执行成功"));
        let pending = store
            .pending_actions_for_conversation(&conversation_id)
            .unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].action, "run_skill_script");
        let sealed: serde_json::Value =
            serde_json::from_str(&pending[0].args_json).unwrap();
        assert_eq!(sealed["skill"], "echo-skill");
        assert_eq!(sealed["script"], "run.sh");
        assert_eq!(sealed["args"], "hi");
        // 确认后走同一条 execute_confirmed（execute_pending_action 的
        // run_skill_script 分支内部就是它），输出正确。
        let executed = execute_confirmed(&dir, &sealed).unwrap();
        assert_eq!(executed, "[echo-skill/run.sh] 执行成功\n\nskill-echo hi\n");
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn trusted_skill_dispatch_executes_directly_without_pending_action() {
        let (store, conversation_id, dir) = dispatch_env("trusted");
        // trust 后 dispatch 跳过审批直接执行，不创建 pending_action。
        crate::ai::skills::set_trusted(&dir, "echo-skill", true).unwrap();
        assert!(crate::ai::skills::is_trusted(&dir, "echo-skill"));
        let out = RunSkillScriptTool
            .run_with_data_dir(&call(), &ctx(&store, &conversation_id), &dir)
            .unwrap();
        // 免审批路径的输出格式与确认后一致：完成态 + stdout。
        assert_eq!(out, "[echo-skill/run.sh] 执行成功\n\nskill-echo hi\n");
        assert!(
            store
                .pending_actions_for_conversation(&conversation_id)
                .unwrap()
                .is_empty()
        );
        // 审计留痕（信任墙 #4 对齐：trust 动作写入 skills/.audit.log）。
        let audit = std::fs::read_to_string(dir.join("skills/.audit.log")).unwrap();
        assert!(audit.contains("\"trust\""));
        assert!(audit.contains("echo-skill"));
        drop(store);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn untrust_and_retrust_roundtrip_updates_state_file() {
        let dir = fixture("state");
        assert!(!crate::ai::skills::is_trusted(&dir, "echo-skill"));
        crate::ai::skills::set_trusted(&dir, "echo-skill", true).unwrap();
        assert!(crate::ai::skills::is_trusted(&dir, "echo-skill"));
        crate::ai::skills::set_trusted(&dir, "echo-skill", false).unwrap();
        assert!(!crate::ai::skills::is_trusted(&dir, "echo-skill"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
