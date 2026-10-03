# Elsewhen 作为 Agent Harness：现状对照与缺口

> 2026-10-02。语境：这里说的 harness 指 agent harness（pi / Claude Code / Codex
> 这类"模型 + 工具 + 循环"的执行壳）——模型负责决策，harness 负责工具暴露、
> 上下文管理、循环控制、持久化。在此语境下盘点 elsewhen 已经具备的 harness
> 构件，以及如果要支持更长程的 agent 任务还缺什么。

## 1. elsewhen 已具备的 harness 构件

elsewhen 的核心机制其实就是一套 harness，只是服务对象是"个人事件流"而不是
代码仓库：

| Harness 构件 | elsewhen 对应物 | 位置 |
|---|---|---|
| 工具调用（tool dispatch） | AI 分析事件时可调知识库查询等工具 | `src/ai/tool/` |
| 循环 + 重试 | analysis/digest job 队列，指数退避，5 次失败冷却 6h | `src/storage/digest.rs` |
| 上下文管理 | BPE 计数、上下文窗口裁剪、per-request 预算 | `src/ai/budget.rs` |
| 持久化/会话状态 | SQLite 单源真相 + durable claims / leases / receipts | `src/storage/approvals.rs` |
| 长任务可恢复性 | generation fencing、过期 lease 恢复、late completion fencing | `knowledge_refresh_jobs` |

其中 schema v10 的 chat run 体系（durable claims、execution leases、late
completion fencing）是** harness 级的执行语义**：应用被杀掉重启后，健康
lease 存活、过期 claim 按 uncertain 恢复。这与 pi 的 goal 工具（跨
compaction 的持久目标）属于同一类设计。

## 2. 已在运行的 harness 式工作流

- **wiki digest**：事件 → job → 模型提案 → 整批校验 → 事务提交。
  经典的 "propose → validate → commit" agent 循环：模型只在 loop 里出提案，
  harness（Rust 侧）控制验证和落库。
- **knowledge/authoring.rs**：chat drafts 只绑定到选中的已验证证据——
  harness 给模型划的工具/数据边界。
- **review contract**（FRB review requests 绑 kind/ID/conversation/proposal
  version/target token）：harness 级别的"确认后执行"，防止模型拿旧 token 对
  新内容动手——即 agent harness 里的 stale action fencing。

## 3. 对照成熟 harness 的缺口

对照 pi 这类成熟 harness，elsewhen 缺的主要是面向**长程任务**的构件：

1. **显式的 agent 目标/计划持久化**。pi 有 create_goal + token budget +
   完成审计；elsewhen 的 digest/run 是任务队列但没有"高级目标"这一层。
   做"整理本周事件并产出周报"这类多步任务时会需要类似的 goal tracking。
2. **工具集对模型可见的动态编排**。目前工具固定；成熟 harness 会按任务
   阶段动态启用工具（如 pi 的 agent_browser_tools 按需 enable）。
3. **多轮对话的上下文压缩/compaction**。对话变长后的摘要与事实保留策略，
   目前没有专门处理。

## 4. 结论

elsewhen 在"持久化执行 + 验证提交 + 失败恢复"这些 harness 硬核部分已经
做得比很多 agent 框架扎实（尤其 lease/fencing 那套）；差的是把"目标管理、
动态工具编排、上下文生命周期"这些面向长程任务的构件显式化。当前
`docs/roadmap/2026-09-30-ai-chat-experience-roadmap.md` 里
"generation/streaming lifecycle" 那一步，本质上就是在补 harness 的
循环控制层。

## 5. 附：chat 提议执行的 lease 模型（`src/storage/approvals.rs`）

chat 提议"确认后执行"的防重/防孤儿执行机制。

### 5.1 解决什么问题

用户确认 AI 提议后要真正执行，执行期间 app 崩了有两种坏结局：
盲重试（实际已成功 → 重复执行）或静默丢失（半路死了没人知道）。
lease 模型把这两种情况变成可判定的状态机。

### 5.2 核心结构（`chat_proposal_decisions` 表三列）

| 列 | 作用 |
|---|---|
| `executor_id` | 进程级 UUID（`OnceLock`，每次 app 启动生成新的）——谁在执行 |
| `lease_until` | 租约截止，初始 `now + 5min`——执行权有效期 |
| `status` | `executing` → `succeeded`/`failed`/`uncertain` |

### 5.3 生命周期

1. **确认时占坑**（INSERT，L338）：插入 `status='executing'` 行，带本进程
   `executor_id` 和 5 分钟租约。同一 (kind, id, version, token) 只能有一个
   executing 行——at-most-once claim。
2. **执行中续约**（`keep_chat_proposal_alive`，L414）：先校验"这行归我且
   租约未过期"，起后台线程 `chat-approval-lease` 每 5 秒把 `lease_until`
   推后 5 分钟；续约 SQL 自带 `lease_until > now` 条件，已过期则影响 0 行、
   线程退出。`ProposalLease` 是 RAII guard，业务完成 drop 即停续约线程。
3. **完成时写回执**（`write_chat_receipt`，L395）：UPDATE 带四条件
   （executing + 我的 executor_id + 租约未过期），影响行数必须为 1，否则
   报错"执行租约或状态已改变，不能覆盖回执"——late completion fencing，
   崩了又被接管的进程若"诈尸"写回执会被拒。
4. **启动时恢复**（`recover_chat_proposal_decisions`，L506）：把
   `executing` 且 `lease_until <= now` 的行改成 `uncertain`，result 写明
   "执行租约已过期，结果需要核对；不会自动重试"。

### 5.4 设计要点

- **5s 续约 / 5min 租约**：崩溃 → 续约线程死 → 5 分钟后租约自然过期 →
  下次启动标 uncertain。正常执行时 5 秒续约保证租约永远新鲜。崩溃检测
  不需要锁/心跳表，租约过期本身就是崩溃信号。
- **健康的别人的 claim 不碰**：recover 只动过期的；另一进程 lease 仍新鲜
  时不抢（"Healthy claims, including those owned by another process, are
  not changed by startup"）。
- **uncertain 而不是 failed**：过期 ≠ 失败（可能恰好成功只是回执没写到）。
  让人核对，绝不自动 replay——无法判断原操作是否已生效，重试可能重复执行
  非幂等操作。

本质：数据库版分布式锁 + 崩溃检测 + 防护栏。用 lease 过期代替锁释放
（崩了没法主动解锁，只能靠租约到期），用 executor_id fencing 防僵尸进程
覆盖新执行者的结果。与 etcd lease、k8s leader election、GCS generation
fencing 同一思想家族。

### 5.5 合理性评估（2026-10-02 讨论结论）

机制本身合理：租约过期代替主动解锁（崩溃没法走 finally）、fencing 双保险
（executor_id + 租约未过期）、uncertain 而非 failed/retry（非幂等操作盲重试
代价远高于人工核对）、续约/租约比例合理、health claim 不抢。在单机 SQLite
场景没有过度设计。

三个值得商榷的点：

1. **单机单进程，分布式语义利用率存疑**。多进程 fencing 防的"另一个进程
   正在执行"在个人桌面 app 几乎不出现；真正主力场景是崩溃恢复（executor_id
   每次启动更换，旧 executor 的过期租约自然被清理），多进程是白赚的。
2. **5 分钟租约对 hang 不住的操作是盲区**。若业务操作卡死（非崩溃，是 hang），
   续约线程仍活着会一直续，租约永不过期，claim 永远卡在 executing。缺绝对
   上限（如无论续约多少次，总时长超 30 分钟强制 uncertain）。当前"影响 0 行
   线程退出"兑底的是续约失败，不是操作超时。**这是当前模型唯一值得补的点。**
3. **uncertain 的可观测性不够**。"结果需要核对"只有固定文案，没有"原操作是
   什么、执行到哪一步"的上下文；多步操作时用户难以判断成没成。这不是 lease
   的锅，是 uncertain 状态的上下文呈现不足。

结论：机制设计正确，放在个人本地应用场景不过度也不欠缺。最值得补的是给
executing 加绝对时长上限，防"hang 住但续约正常"的半死状态。
