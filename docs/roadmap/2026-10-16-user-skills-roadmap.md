# 用户技能（User Skills）路线图

> 2026-10-16。定位：在工具懒加载（enable_tools，`c54367f`）与内建技能包的基础上，
> 让用户**自行安装** Claude Code 生态格式的技能（SKILL.md 目录），平台只管发现、
> 加载与安全执行，不做逐 skill 的 Rust 移植。

## 背景与决策记录

- 工具懒加载已落地：21 工具拆为 9 常驻 + 12 延迟，`enable_tools` 按技能包激活，
  省 ≈10k token/轮归还历史预算。
- 方向纠偏（明确否决）：为单个外部 skill 重写原生 Rust 工具**不可取**——那会把
  每个技能都变成内部开发工程，失去生态价值。平台职责是通用的发现/加载/执行。
- 格式直接兼容 Claude Code 的 `SKILL.md`（YAML frontmatter：name/description +
  Markdown 正文 + 目录内任意脚本/模板文件），现成技能拷贝即用。

## 信任边界（先于功能，四道墙）

1. **身份与内容分离**：skill 是否存在、启停状态存注册信息，正文只是内容；
   未注册的任何知识页/文件不会被执行或加载（阻断伪装注入的第一跳）。
2. **AI 写保护**：`save_wiki_revision` 等写工具不得触碰 skills 目录；
   skill 文件只由用户放置或修改。注入链断在"改动方必须是用户"。
3. **执行人工闸门**：`run_skill_script` 为 `ToolPolicy::WriteConfirm`，
   每次执行前完整展示命令行；路径限制在 skill 目录内（防穿越）；
   超时、输出截断、不继承本进程敏感 env、工作目录限定 skill 目录。
4. **审计回滚**：skill 目录纳入常规备份即可；启停操作记录到事件日志
   （`chat_run_events` 同款风格，可选）。

安全模型一句话：**装 = 信任来源（如装软件包），跑 = 逐次确认（人工闸门不豁免）**。

## 架构设计

### 双轨注册

- **内建技能**：静态 `BUILTIN_SKILLS` 表（现有 6 个），高频核心能力，零发现成本。
- **用户技能**：启动时扫描 `~/.local/share/elsewhen/skills/*/SKILL.md`，
  解析 frontmatter（name/description，description 截断一行进 L1 清单）。
- L1 清单顺序：内建在前、用户安装在后，格式一致；
  用户技能数量设上限（如 30 个），超出提示管理。

### L2 加载（复用现有机制）

- `enable_tools` 的 `skills` 参数天然支持动态清单：内建 + 用户技能同列。
- load 回传 = SKILL.md 正文 + 该 skill 目录文件清单（告知模型可执行哪些脚本）。
- 注入走尾部（与动态背景段同位置），不破坏 system+历史的前缀缓存。

### 脚本执行：一个通用工具

```
run_skill_script(skill: "proxy-pick", script: "templates/clash-pick",
                 args: "--list")
```

- 解释器按 shebang 解析（python3/bash/node…），从 PATH 查找；
  统一拒绝 `sudo`、交互式命令。
- 专属政策 `ToolPolicy::WriteConfirm`（每次确认），不随 skill 作者意图降级。

### 移动端（Android）

- 纯知识 skill：全功能可用。
- 脚本型 skill：正文的"方法论"部分照常加载（对指导用户手动操作仍有价值），
  **执行时**返回明确错误"此技能需要桌面环境"——不假装、不静默。
- 安装路径 P2 起：文件选择器导入 zip/目录；skills 未来可作同步 dataset
  （桌面安装、全家设备可用），正文同步、脚本降级。

## 分期

| 期 | 内容 | 验收 |
|---|---|---|
| P1 桌面可用 | 目录发现+解析、run_skill_script、skills 目录写保护、设置页管理区块（已装清单/开关/打开目录） | 丢一个真实 Claude Code skill（如 proxy-pick）进目录，重启后可加载、可执行、命令执行前有确认 |
| P2 移动端 | 文件导入安装、脚本型降级提示、设置页技能管理 | PEUM00 上装纯知识 skill 全功能；脚本型能看正文、执行有明确错误 |
| P3 同步 | skills 作为同步 dataset，跨设备分发 | 桌面装、手机同步可见；启停状态按设备独立 |
| P2 之外（存疑） | mihomo 等"宿主环境代理"类工具原生实现 | 仅当某 skill 的脚本依赖宿主机常驻服务且用户高频使用时再议 |

## 实施记录（2026-10-17 更新）

| 项 | 状态 | 提交 |
|---|---|---|
| 目录发现/frontmatter 解析/L1 清单/L2 enable 回传 | ✅ | `9df521b` |
| run_skill_script（WriteConfirm + canonical 防穿越 + env 白名单 + 120s/8KB） | ✅ | `9df521b` |
| 设置页管理（清单/开关/打开目录） | ✅ | `9df521b` |
| 安装：本地目录 / zip / git clone（重名拒绝、zip-slip 防御、失败回滚） | ✅ | `8be7032` |
| 用户技能上限 30（install 拒绝 + roster 截断提示） | ✅ | 本提交 |
| 审计：install/enable/disable → `skills/.audit.log`（JSONL，不进用户事件流） | ✅ | 本提交 |
| 移动端脚本降级：Android/iOS 执行返回明确"需要桌面环境"，正文照常加载 | ✅ | 本提交 |

**信任墙 #2（AI 写保护）审计结论**：不靠"禁令守卫"而是**结构上不存在**任意路径写——
盘点全部 AI 可触发的落盘点：wiki 正文/修订/图片台账全在 SQLite（`ingest_wiki_page_assets`
只读盘做台账，不写文件；`resolve_wiki_image_refs` 的 `data_dir.join` 仅为 exists 读检查）；
tweet_images 落盘的目录名是外部平台纯数字 id；run_skill_script 有 canonical 守卫。
未来新增写工具时，本条是 review 必检项：AI 参数不得直接拼文件系统路径。

**待验证**：`9df521b` 起的装机清单（同明天 arm64 装机一起）：真机装 proxy-pick
全流程（install → enable → run 前确认 → 节点切换）。P2 剩余仅真机回归；
P3 同步 dataset 在性能数据出来后排期。

## 关联

- 工具懒加载与内建技能包：`a86ea79`、`c54367f`（feature/wiki-retrieval）
- 前缀缓存观测：`4c83a85`（prompt_cached_tokens 落库，装 机后拉命中率报表）
- 与慢对话优化无耦合，不抢性能验证的优先级；排在本次装机验证之后
