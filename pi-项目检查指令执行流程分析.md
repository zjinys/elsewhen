# Pi 处理「检查当前项目」指令的执行流程与算法分析

> 分析对象：`ref/pi`（Pi Agent Harness 源码）
> 结论基于对 `agent-loop.ts`、`agent.ts`、`system-prompt.ts`、`resource-loader.ts`、`skills.ts`、`tools/` 等源码的通读。

## 核心结论

「检查一下当前项目，看看主要功能、特点和完成度、商业价值」**不是 pi 内置的命令、技能或算法**。pi 里没有任何叫 "project review / analyze / summary" 的硬编码流程。它只是一段普通自然语言，被当作一条 `user` 消息丢进 agent 循环。

真正执行"检查"和"写总结"的是 **LLM 模型本身**。pi 只负责提供两样东西：

1. 一个**确定性的工具调用循环**（`agent-loop.ts`）——决定"模型说调工具 → 执行 → 结果回填 → 再问模型"这件事怎么反复进行；
2. 一套**工具**（`read` / `bash` / `grep` / `find` / `ls` / `edit` / `write`）和一份**系统提示词**，把模型"武装"成一个会读代码的 agent。

所以答案是分两层的：**pi 侧有确定流程，但"怎么分析项目、总结写得怎么样"是模型的启发式行为，不是代码算法。**

---

## 一、pi 侧的确定性执行流程

### 1. 系统提示词组装（启动时，`system-prompt.ts`）

模型不是"裸"收到你这句话的。每次请求前，`buildSystemPromptSections()` 会把系统提示词拼成结构化分节（`<tools>`、`<rules>`、`<docs>`、`<project_context>`、`<skills>`、`<cwd>`…）：

| 分节 | 内容 | 来源 |
|------|------|------|
| `preamble` | "你是 pi 里的专家编码助手…" | 硬编码 |
| `tools` | read/bash/edit/write 等工具的用途一句话 | 工具注册表 |
| `rules` | "用 read 而非 cat/sed""简洁""显示文件路径"等 | 硬编码 + 工具贡献 |
| `project_context` | 项目根目录向上递归加载的 `AGENTS.md` / `CLAUDE.md` | `resource-loader.ts`（候选名：`AGENTS.override.md` > `AGENTS.md` > `CLAUDE.md`） |
| `skills` | 所有可用 skill 的 name+description 清单 | `skills.ts` 的 `formatSkillsForPrompt` |
| `cwd` | 当前工作目录 | — |

> 关键点：在 `elsewhen` 里发这条指令时，`/home/pp/playground/ai/elsewhen/CLAUDE.md` 的内容已经作为 `project_context` 灌进去了，所以模型一上来就知道这是"Rust core + Flutter UI 的本地优先事件系统"。

### 2. Agent 循环（`packages/agent/src/agent-loop.ts`）

这是唯一真正"跑起来"的部分，两层循环：

```
外层 while(true)：处理 follow-up / steering 消息
  内层 while(hasMoreToolCalls || pendingMessages)：
    1. prepareRequest()   —— 模型/思考等级等准备
    2. streamAssistantResponse() —— 调 LLM，流式接收
    3. 若返回 toolCall：
         executeToolCalls()  —— 并行(默认)或顺序执行工具
         结果作为 toolResult 消息追加回上下文
         hasMoreToolCalls = true → 回到第 2 步
    4. 若返回纯文本(无 toolCall)：
         hasMoreToolCalls = false
   结束条件：模型不再发 toolCall，且没有 follow-up → agent_end
```

**工具默认并行执行**（`toolExecution` 默认 `"parallel"`，见 `agent.ts` 构造器）。这就是为什么同一轮会**一次并行发多个 `read` / `bash`**——因为模型在一条消息里同时声明了多个 toolCall，pi 用 `Promise.all` 并发跑，而不是排队。

### 3. 决定流程形态的确定性约束（工具本身的截断规则）

这些是硬编码的，直接塑造了"检查项目"的具体动作：

- `read`：文本最多 **2000 行 / 50KB**，超了会输出 `[Showing lines x-y ... Use offset=N to continue]`，逼着模型用 `offset/limit` 分页读大文件（`read.ts`）。
- `bash`：输出同样截断到 2000 行 / 50KB，超了写临时文件。
- `read` 有专门 guideline：**"用 read 而非 cat/sed"**；`bash` 的 guideline 是"用 bash 做 ls/rg/find 这类文件操作"。

---

## 二、"算法"部分：其实是 LLM 的启发式策略

代码里**没有**"项目检查五步法"。是模型拿到上述工具后，自己决定这么干：

**阶段 1 — 探索目录结构**
```
ls -la（或 bash find）看根目录布局、.gitignore、README
```

**阶段 2 — 读"元文档"建立框架**
```
read README.md / AGENTS.md / CLAUDE.md
```
（这些其实已在 `project_context` 里，但模型常会再读 `docs/roadmap/*.md`、`package.json`、`Cargo.toml`）

**阶段 3 — 针对性深挖**
```
grep 找 TODO/FIXME/未实现/roadmap
read 关键源码文件（storage.rs / ai.rs / ui/...）
find 统计目录、测试文件数量
```

**阶段 4 — 综合输出**
把读到的内容归纳成"功能 / 特点 / 完成度 / 商业价值"四段。

⚠️ 其中 **"完成度"和"商业价值"不是计算出来的**，没有代码去统计"已完成 TODO 比例"或"估值模型"。它们是模型根据 `roadmap` 里的 checkbox、`#[cfg]`/`todo!()`、README 的措辞、git 历史等做的**语义推断**。

---

## 三、"总结文档"是怎么产生的

默认情况下，**不会生成文件**。总结就是循环停止时模型输出的最后一段 `assistant` 文本（`streamAssistantResponse` 里 `finalMessage` 的 text 内容），直接显示在终端。

只有当指令明确要求"写成一个文档 / 存成文件"时，模型才会额外调一次 `write` 工具，把总结落盘。所以：

- "得出总结文档" = **LLM 的最后一段文本**，不是 pi 有专门的文档生成器；
- "写文件" = 模型自主决定调用 `write` 工具。

---

## 四、关键代码位置速查

| 环节 | 文件 |
|------|------|
| Agent 主循环（两层循环 + 工具执行） | `packages/agent/src/agent-loop.ts`（`runLoop`） |
| Agent 状态机 / 工具并行默认值 | `packages/agent/src/agent.ts` |
| 系统提示词组装 | `packages/coding-agent/src/core/system-prompt.ts` |
| AGENTS.md/CLAUDE.md 向上递归加载 | `packages/coding-agent/src/core/resource-loader.ts`（`loadProjectContextFiles`） |
| Skills 发现与注入提示词 | `packages/coding-agent/src/core/skills.ts` |
| 斜杠命令（`/settings` 等内置命令清单） | `packages/coding-agent/src/core/slash-commands.ts` |
| Prompt 模板（`/cl` `/deslop` 等，`/` 开头才展开） | `packages/coding-agent/src/core/prompt-templates.ts` |
| read 工具及截断规则 | `packages/coding-agent/src/core/tools/read.ts` |
| 工具清单（read/bash/grep/find/ls/edit/write） | `packages/coding-agent/src/core/tools/index.ts` |

**一句话总结**：这条指令走的是"普通自然语言 → 组装好的系统提示词 + 工具 → agent 循环反复『模型出招 → 并行执行工具 → 结果回填』→ 模型停止发工具时输出总结文本"。pi 提供的是循环和工具，具体"检查什么、怎么下结论"完全是模型在系统提示词约束下的自主行为，没有任何针对"项目检查"的专用算法。
