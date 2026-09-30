# RikkaHub Desktop 聊天子系统底层架构分析（对比 Elsewhen）

> 分析对象：`ref/rikkahub-desktop/`（pc-server + web-ui）
> 分析视角：非功能清单，而是架构决策层——数据模型、流式管线、一致性、上下文工程、记忆系统。
> 结论先行：RikkaHub 整个聊天子系统围绕一个核心问题构建——**"流式生成中的状态如何在内存、磁盘、网络三处保持一致且各自不爆"**；Elsewhen 聊天是"请求-响应"模型，生成是一个同步事务。这不是功能差距，是状态机维度的差距。

---

## 〇、两家的根本分歧

```
RikkaHub:  Provider SSE → GenerationEvent → 单一应用器 → 内存活实例(working set)
                                                      ├→ 标脏 → 200ms 节流 upsert SQLite
                                                      └→ 33ms 合帧 → 指纹差分 → SSE → 前端不可变更新

Elsewhen:  Dart send_message → flutter_rust_bridge → Rust Store::send_message
             → SQLite 落库 → 一次性 provider.generate_reply() → 落库 → 返回 String
```

---

## 一、数据模型：消息树 vs 消息链——分支的真实成本

### RikkaHub：`Conversation → MessageNode[] → Message[]`

```
Conversation.messages = MessageNode[]      // 时间轴上的"轮次"
MessageNode.messages  = Message[]          // 同一轮次的多个分支版本
MessageNode.selectIndex                     // 当前选中哪个分支(持久化在节点行)
```

**分支不是树，是"节点数组 × 每节点分支数组"的二维结构**。同一 node 里 user 的多个编辑版本、assistant 的多个 regenerate 版本天然落位，`selectIndex` 是分支切换的全部状态。当前路径 = 每节点取 selectIndex 的序列，编码请求体时一次数组映射出链，O(1) 分支切换。

**付出的代价**（`fork-repair.ts` 记录的真实事故）：`pc_message_node.id` 是全局主键，fork 深拷贝沿用源节点 id，`ON CONFLICT(id) DO UPDATE` 把源会话的节点行**改挂到分支名下**——源会话重载后消息凭空消失，删分支还会级联销毁。修复器靠"受害特征识别 + 标题配对 + 新 id 前缀复制"抢救，明确写了"无法配对时仅记录日志，绝不猜"。

### Elsewhen：`Message.parentMessageId` 树 + `get_message_chain`

通用树模型，表达力更强（任意位置分叉），但三个隐性成本：

1. **当前路径没有物化**——UI 展示必须选叶子路径回溯（`message_tree_view.dart` 259 行专门处理）；
2. **分支切换状态没有存储位置**——selectIndex 持久化 vs UI 态重启丢失；
3. **编码请求体成本高**——每轮生成回溯整条链，RikkaHub 是一次数组遍历。

### 借鉴

- 物化"当前选中路径"：一张 `conversation_paths(conversation_id, leaf_message_id)` 表，UI 树导航简化，`generate_reply` 上下文直接从叶子回溯。
- fork 教训的反面教材：未来做"从某消息分叉新会话"，节点 id 必须新生成 + 前缀复制，绝不共享主键。

---

## 二、流式管线：差距最大、最值得整套搬的一层

### 2.1 引擎与副作用解耦：`GenerationEvent` sink（`inference-engine/events.ts`）

推理引擎**不碰 SSE、不碰 SQLite、不碰全局状态**，只发事件：

```typescript
type GenerationEvent =
  | { kind: "text_delta"; text }
  | { kind: "reasoning_delta"; text }
  | { kind: "tool_call_created"; ... }
  | { kind: "tool_approval_updated"; ... }   // 审批态只升不降:用户已决定的不回写
  | { kind: "tool_result"; final?: bool }    // partial 只刷新 output;final 才落 toolFinishedAt
  | { kind: "usage" | "engine_status" | "engine_fidelity" }
  | { kind: "finished" | "error" | "abort" }
```

所有事件汇进**唯一应用器** `generation-apply.ts`："两套写入逻辑必然漂移，漂移即渲染/落库分叉"。

细节坑：`GenerationTarget`（流式落点）必须**每事件现读**，不能闭包创建时解构缓存——steer 分裂会换绑落点到新节点，"缓存一次就等于永远写旧节点"（实测 bug：分裂后延续输出写进已定格的旧节点，空到收尾才被整段回填）。

另一个搬家事故：text_delta 分支曾丢 `touchStream` 调用 → 无工具会话全程无增量帧、无增量落库，流式中崩溃丢整段回答。

**Elsewhen 现状**：`generate_reply` Rust 侧同步返回 String，Dart 侧 `aiGeneratingProvider` 只是 `Set<String>`，中间无任何增量通道。flutter_rust_bridge 原生支持 `StreamSink`，架构无障碍，缺的是事件协议。

### 2.2 O(N²) 问题的四层解药（每层独立可搬）

流式本质矛盾：每 chunk 携带累积全量文本，流长 N 则传输/落库/渲染都是 O(N²)。

**① 网络层：指纹差分（`api/node-delta.ts`）**
flush 时与"上次已广播指纹"逐 part 比对：纯前缀增长（`startsWith`，300KB ~10µs）→ `text_delta` 帧只带增量；任何结构变化 → 全量关键帧兜底。**正确性不依赖"生成只会追加"的假设**。客户端 `baseLen` 自校验，失配即重订阅全量快照。**"增量帧丢失永远不会画错，只会退化为一次重同步"**——全系统反复出现的设计哲学。

**② 广播节奏层：33ms 合帧（`api/sse.ts`）**
首个 chunk 立即发，后续 ~30fps 合并，每帧携带最新引用。动机：朴素逐 chunk 广播让浏览器掉队（"卡住然后倾泻"），且**停止按钮感觉卡**——旧事件排在 stop 的 flush 前面。33ms 让 stop 响应延迟有上界。

**③ 持久化层：标脏 + 200ms 节流 upsert（`touchStream`）**
```typescript
markConversationRowDirty(id);       // 只标脏
markMessageNodeDirty(convId, nodeId);
scheduleThrottledConvFlush();        // 200ms 合并 upsert 进 WAL
scheduleNodeBroadcast(conv, node);   // 33ms 合帧广播
```
配套血泪教训（`upsert-cascade.test.ts`）：**`INSERT OR REPLACE` 的隐式 DELETE 在 foreign_keys=ON 时级联删光该会话全部节点**——流式期间每次 flush 清一次表，进程中途死亡 = 历史永久丢失。upsert 永远用 `ON CONFLICT DO UPDATE`，不用 REPLACE。

**④ 渲染层：frozen-prefix（`components/markdown/frozen-prefix.ts`）**
已完成的顶层 markdown 块"冻结"不再重算，每帧只重算活动尾部。末 2 块永不晋升（setext 标题/列表惰性延续只影响紧邻块）；巨型单块（流式表格 150 行 ~54ms、400 行 ~210ms）记录失败水位，尾部再涨 1KB 才重试，探测成本从 O(尾部)/帧摊薄到 O(尾部)/KB。四层思路同构：**增量识别 → 保守锚点 → 失配退化不画错**。

### 2.3 生成生命周期：带意图的中止（`generation-state.ts`）

```typescript
type GenerationAbortReason = "interrupted" | "replaced" | "deleted";
class GenerationAborted extends DOMException { constructor(readonly reason) ... }
// signal.reason 原对象传递,子类身份保留
```

**谁中止谁声明意图**，收尾按意图决定队列命运：
- `interrupted`（用户按停止）→ 队列冻结，绝不自动接棒——"用户按停止的意图是让这轮停"
- `replaced`（被 send/regenerate/edit 接管）→ 队列照常派发——新流代表继续对话的意图
- `deleted` → 队列随会话清除

为什么不能"事后看注册表猜"：旧流的 catch 抢在新流登记前跑到，会把接管误判成打断并冻结队列。**竞态发生在异步窗口里，事后推断必然判反，只有中止动作发生时的显式声明可靠。**

**Elsewhen 现状**：无 abort 语义，无接管语义，AI 生成中直接禁止提交新消息。这套机制在 Dart + Rust 两侧几乎可直接翻译。

---

## 三、一致性架构：Working Set 单一实例注册表（`conversations/working-set.ts`）

DB-first 迁移的最大风险："同一会话出现两个内存实例 → 并发修改互相丢失"。解法：**只要有代码持有某会话实例，checkout(id) 必返回同一实例**。sweep 清扫条件（缺一不可）：

```
refs === 0          // 无请求作用域持有(长 await 期间 refs>0,永不清)
!isGenerating(id)   // 不在流式生成中
!hasSseClients(id)  // 无打开的 SSE 流
!hasDirty(id)       // 无未落库脏标记
+ lastAccess 超 IDLE_GRACE(快速重开免重复加载)
+ hasQueuedMessages // 队列派发链跨"当前流结束→下条点火"空窗持有会话
```

**Elsewhen 的对应物是反面的**——`conversation_provider.dart` 自己的注释：

```dart
// 串行而非 Future.wait 并发:每次 listMessages 在 Rust 侧都会 Store::open
// 新连接,而 Store::open 会跑 ensure_schema(稳态仍抢写锁)+ backfill + wal_checkpoint。
// 并发 N 个连接同时抢写锁 → database is locked
```

同一个问题的另一端：RikkaHub 担心"多个内存实例"，elsewhen 正在承受"多个连接实例"。每次 API 调用 `Store::open()` 新建连接，稳态下 ensure_schema/backfill/checkpoint 全是写锁竞争者。

**借鉴**：Rust 侧 store 注册表——`HashMap<path, Arc<Store>>` + 引用计数，或进程级单 Store（1 写 N 读连接池）；ensure_schema/backfill 挪到启动期一次性执行。哲学相同：**实例获取必须经过注册表，临时 new 出来的实例是 bug 温床**。

配套：`persistence/instance-lock.ts`（dataDir 单实例 pid 锁，防双开共写同一数据目录，两进程 state.json last-writer-wins 互相覆盖）——30 行代码，elsewhen 桌面版同样需要。

---

## 四、上下文工程：缓存命中是设计出来的

### 4.1 易变段冻结快照（`inference-engine/context-snapshots.ts`）

**前缀缓存按首个分歧点截断**——system 消息里的"记忆块"和"最近会话块"（唯二高频易变段）一变，跟在后面的整个会话历史缓存全部失效。解法：按 `(会话, 助手, 相关开关)` 冻结快照，同一会话生命周期内 system 逐字节不变。

取舍明确：会话中途的记忆写入"在工具结果里模型本就看得见"，不失效；设置页手动改记忆是明确用户动作，立即 `invalidateContextSnapshots()` 重建。LRU 200 条。

### 4.2 usage 合并语义（`inference-engine/tool-loop.ts`）

`mergeTokenUsage`：新值 >0 才覆盖。流式场景后到的 usage 事件缺字段（Google 末尾 chunk 不带 cachedContentTokenCount）不能把已知缓存命中数清零——"这是'命中数有时不显示'的根因之一"。

### 4.3 Elsewhen 现状与借鉴

elsewhen 的 `SlidingWindowMemory.prepare_context`：全量消息 + system prompt + recent_background，`compress_context` 截断。上下文每次全量重建，没有缓存命中意识。**借鉴**：上下文构建先做"段稳定性分析"——会话内不变的段（角色设定）冻结放最前，每轮必变的段（recent_background）放最后，易变段绝不放在稳定段前面。token 成本立竿见影。

---

## 五、记忆系统（Memory）——本节的完整对比

### 5.1 概念分歧：两种"记忆"完全不同

| | RikkaHub memory | Elsewhen `ai/memory.rs` |
|---|---|---|
| 本质 | **跨会话的持久事实库**（LLM 长期记忆） | **会话内上下文窗口管理**（short-term） |
| 类比 | MemGPT/Letta 的 archival memory | LangChain 的 ConversationBufferMemory |
| 存储 | `memory/` 目录三个 JSON 文件 | 无持久化，从 messages 表实时构建 |
| 写入方 | 用户手动 + AI 工具提议 + 确认队列 | 无写入概念，纯读 |

两者名字相同但几乎正交。**Elsewhen 缺的是 RikkaHub 这种跨会话持久记忆**——它的生态位目前部分由知识库（wiki pages）和个人规则库承担，但"记住用户的偏好/事实"这个轻量场景没有对应物（规则库偏行为准则，知识库偏内容沉淀）。

### 5.2 RikkaHub 记忆系统的架构（`memory/index.ts`，557 行）

**分层模型**：
- **全局层**（`GLOBAL_MEMORY_ID = "__global__"`，字面量与安卓版 `MemoryRepository.kt:11` 保持一致——跨平台导入 state.json 不丢全局记忆）
- **助手层**（per-assistant 分组）
- **待确认队列**（pending，AI 提议未确认的记忆）

**注入对模型透明**（`memoriesForAssistant`）：全局层 + 助手层按 updatedAt 排序叠加注入 system prompt，**模型完全不感知层级存在**——这是产品决策核心。注入条件各自独立：助手关记忆时仍可见全局层。

**写入策略四档**（`WriteStrategy`）：
- `ask`（默认）：进待确认队列，用户事后确认存哪层
- `always_assistant` / `always_global`：直存对应层（对应层未启用时降级 ask——矛盾组合不报错，降级）
- `readonly`：不暴露写入工具，只注入已有记忆

**save_memory 工具设计**（`tools/definitions.ts`）：
- 模型只负责"提议" content，不感知层级
- **v1 废弃模型 edit/delete**——"定位歧义从根本上绕开（N3）"：让模型编辑/删除记忆需要它引用 id，指错就是数据损坏，不如只许提议、人类 reconcile
- 返回值**不含 pending: true——生成不在此暂停**（区别于 ask_user 的审批挂起）：记忆提议是 fire-and-forget，不打断对话流
- 工具描述里内建约束："Do NOT store sensitive info (ethnicity, religion...)"；"Prefer checking the injected memories list first; if a similar one exists, propose the updated content anyway and the user will reconcile"
- 暴露条件 = （助手层或全局层开启）且 writeStrategy !== readonly

**待确认队列纪律**（M7）：
- 容量上限 100，超限拒绝入队，工具返回 `overflow`，前端徽章变体高亮提醒处理积压
- 与现有 pending content 完全相同（忽略首尾空白）不重复入队，返回 `deduped`
- pending 落盘 **await 完成**——"pending 是用户尚未确认的数据，丢不得"（区别于 addMemory 的 fire-and-forget）
- resolve 三态：global / assistant / discard；用户编辑过内容 → source 置 manual（不再挂 AI 来源标签），原样确认 → source 保持 ai
- 来源追溯：conversationId + conversationTitle **入队时快照**（与会话改名/删除解耦）+ messageNodeId

**持久化纪律**：
- 三个文件：global（含 nextMemoryId 计数器）、assistant、pending
- **写顺序 S1：先写 global（计数器）再写 assistant/pending**——崩在 global 写完、assistant 写之前，重启 recompute 从不完整文件重算 max(id)+1，已落盘 id 永不重用
- `recomputeNextId()`：**不信任持久化计数器**，启动时 max(所有现存 id)+1 兜底自愈
- 串行化写队列 `writeQueue`：所有写操作排队，吞掉 reject 防"一次失败永久污染队列"
- 原子 temp-rename 写，8 次重试，失败 reportError（不静默）
- **损坏隔离**（corrupt-quarantine）：JSON 解析失败先把原件改名 `.corrupt-{ts}` 保住原始字节，再降级默认值 + reportError——"此前仅 console.warn 后返回空默认，后续任何 persistAll 都会用空数据覆写损坏原件且用户零感知（'记忆全没了'）"
- 助手删除后反查不到名字 → "未知助手" 快照分组（M5），不静默丢记忆
- 导入 replace/merge 两模式都**重新分配 id**（备份/APP 的 id 空间可能冲突）；merge 按 `(assistantId, content)` 去重
- 用户手动编辑 → source 置 manual——"前端据此决定是否显示'AI 来源'标签"

**prompt 构建**（`buildMemoryPrompt` / `buildRecentChatsPrompt`）：
- 记忆块：`JSON.stringify([{id, content}])` 注进 system（带 id 是为了用户问起时能引用）
- 最近会话块：直查活库 update_at 倒序 10 条（标题兜底经 peekFirstMessageParts 只读单行不驻留），`{title, last_chat}` 格式
- **这两块就是 context-snapshots.ts 冻结的对象**（见 §4.1）

### 5.3 Elsewhen 现状

`ai/memory.rs` 847 行，是上下文窗口管理：
- `SimpleMemory`（last N messages）/ `SlidingWindowMemory`（token 预算 + `compress_context`）
- `recent_background`：跨对话要点注入（`recent_user_messages_for_context(6, 160)`），knowledge_mentor 会话不注入；`optional_background` 标记（不序列化给 provider 的语义标记）
- `estimate_tokens`：CJK 加权
- 个人规则库（rules.rs）+ 知识库承担了"持久事实"的生态位

### 5.4 借鉴判断

Elsewhen 要不要引入 RikkaHub 式记忆？**应该引入一个轻量版**，理由：
- 规则库是"行为准则"（怎么做事），知识库是"内容沉淀"（什么结论），缺"事实记忆"（用户叫 X、偏好 Y、正在做 Z）——system prompt 里那个巨大的静态角色设定有一部分本该是动态记忆
- 生态位冲突要避嫌：记忆应该是**轻量、免确认成本低、自动注入**的层；重的、需要引用的内容仍走知识库 digest 管线

可直接搬的设计决策（按价值排序）：
1. **模型只提议不修改**（v1 废弃 edit/delete 的理由：定位歧义根本绕开）——与 elsewhen 的"草拟确认制"文化完全一致
2. **pending 队列纪律**：容量上限 + 去重 + await 落盘 + 来源快照——elsewhen 的 knowledge draft pending actions 可以对照自查（pending 是不是 fire-and-forget？容量有没有界？）
3. **注入对模型透明 + 开关独立**：层级是用户侧组织概念，不污染模型视角
4. **写入策略四档 + 矛盾组合降级**：writeStrategy 与开关的矛盾组合（always_assistant 但助手层没开）降级而非报错
5. **损坏隔离 + id 自愈**：readFile 失败先隔离原件再降级；计数器不信任持久化值，max(id)+1 重算——这两条对 elsewhen 的 wiki 页 JSON 存储同样适用
6. **记忆块冻结进前缀缓存**（§4.1）：记忆注入必须配合快照冻结，否则每轮失效整个前缀缓存
7. **source: ai/manual 标签 + 用户编辑即转正**：AI 来源的轻量事实与人工确认的事实要可区分，编辑过就算人的

不适合搬的：JSON 文件存储（elsewhen 全 SQLite，记忆表进 elsewhen.db 更自然，仍可保留三表分层 + 写顺序纪律）；助手层（elsewhen 没有多助手概念，简化为全局单层 + 可选按 conversation tag 分组）。

---

## 六、审批与中断的汇合：两种范式

- **聊天引擎：pause-resume 两段式**。工具批遇审批 → 整批暂停挂 pending 卡 → 用户决定 → 重触发续跑。
- **pi 引擎：run-and-suspend**（`inference-engine/approval-gate.ts`）。引擎无关"等待者登记表"：工具 execute 内部挂起在 Promise 上，生成保持在跑，用户决定经 API 到达后 resolve 唤醒在途 execute。键用 `\u0000` 分隔（ID 里不可能出现；字面 NUL 会让 rg 把文件当二进制跳过——连这种细节都注释）。

纪律：等待者随决定/中止即刻注销 + runner 收尾兜底清扫；跨重启孤儿审批"查不到等待者时按仅记录状态处理"。

**对 elsewhen**：未来若在生成中途插入确认（"AI 提议修改受保护页面，等确认后继续"），run-and-suspend 的登记-唤醒模式比"生成挂起等 UI"更适合 frb 跨进程场景——async 通道天然支持挂起。save_memory 的"生成不暂停、fire-and-forget 进队列"则是第三种模式，适合低风险写。

---

## 七、消息队列 + Steering：及时性与事实源的双通道

- **FIFO 队列（`message-queue.ts`）是事实源**：纯内存态——消息本体发送即落库，队列只压"待触发的生成"（重启只丢未触发的生成，消息零丢失）。终局三态 Completed/Failed→收尾续跑（Failed 被 pause 门控）/Interrupted→冻结等显式 resume（对齐 Codex ThreadIdleCause 门控）。跨引擎统一：队列对引擎无感，只在 generateAnswer 完成时由编排层派发。
- **Steering 通道（`steering-channel.ts`）是及时性旁路**：生成中补发的消息同步推一份；引擎在"工具批执行完、下一次模型请求前"的轮边界取出，编码成 user turn **注入当前这轮**——用户期待"模型在当前任务中途就看到"，不是"等全部完成后作为新一轮"。
- 联动：注入成功 → 回调队列移除对应项（已注入 ≠ 待触发）；注入前被撤回 → 排水时 id 不在队列 → 丢弃；收尾未注入残留自动作废，FIFO 兜底，零丢失。
- 微小不变式：纯附件不进 steering 通道（注入面是 user turn 文本），留在 FIFO 走完整多模态路径——保证"排水命中 ⟺ 有可注入文本"，引擎侧判定只需看返回数组非空。

**Elsewhen 的独特机会**：主对话流混合"记录事件"与"对话"。引入队列后可分流——记录型消息立即落库不打断生成（事件捕获不能被 AI 阻塞，CLAUDE.md 硬约束），对话型进 steering/queue。这个分流 RikkaHub 没有（无事件捕获域），是 elsewhen 可以超越原作的点。

---

## 八、传输与规模

### 窗口化快照 + 内容戳（`api/snapshot-window.ts`）

快照只带最近 60 节点 + **全量节点 wyhash64 内容戳清单**（千节点 ~10KB）。客户端"可验证前缀合并"：已加载的更早节点逐比内容戳，一致才保留对象身份，不一致报 staleRange 由分片端点重拉替换（react-virtuoso 不支持 firstItemIndex 原地增大，就地截短列表尺寸树错乱出幽灵空白——所以前缀原样保留 + 区间重拉，短暂展示旧版，永不画错结构）。

### SharedWorker 单连接（`workers/app-events-worker.ts`）

N 个标签页共享 1 条 SSE 连接；快照类事件缓存最新一帧给晚接入页面重放。页面判死阈值 120s 的实测记录：Chromium intensive throttling 把 15s 心跳压到 1/min，45s 阈值会把活页误判死 → 连接永不重启，面板永久卡在进行中。SSE 看门狗：45s 无字节判连接已死（合盖睡眠的半开 TCP 会让 reader.read() 永久挂起不抛错）。

**Elsewhen 借鉴**：单窗口桌面应用无标签页问题，但**内容戳思路适用于 frb 桥**：`list_messages` 每次全量传 JSON 数组，序列化成本随会话长度线性涨。`(id, hash)` 或 `(id, updated_at)` 增量协议，wiki 页列表同理。

### 读模型：JS 侧过滤而非 SQL（`conversations/read-queries.ts`）

反直觉决策：过滤/排序/分页留 JS 逐字复刻旧内存实现——"SQLite lower()/LIKE 只处理 ASCII，与 JS toLowerCase() Unicode 语义有差异"。**对 elsewhen 的警示**：Rust `to_lowercase()` 是 Unicode 感知的，SQLite 不是。搜索/过滤下沉 SQL 时注意德文 ß、土耳其文 i 分叉。FTS5 trigram（≥3 字符 MATCH，1-2 字符中文走 LIKE 且 trigram 表 LIKE 自动走索引）可直接照搬给消息/事件全文搜索。

---

## 九、会话压缩（Compaction）

`conversations/auxiliary.ts` + `lib/compaction.ts`：`/compact` 指令或自动压缩；压缩边界注解画分割线（"线上的会话已被压缩"），两种压缩形态（pi 切点记录 / UI 历史替换）在展示面收敛为同一注解。压缩记录存会话行 `engine_compactions` 列（随行删除天然级联）；切点为消息 id，从切点（含）起保留原文，之前历史被 summary 取代；**只有"切点仍在选中路径"的最新一条生效**（编码器自校验，编辑/fork 后失效记录自动跳过）。压缩进行中状态存服务端（切页后压缩状态不丢）。压缩与 regenerate 写互斥（409 挡下，防压缩落库与 regenerate 竞态）。

**Elsewhen**：已有 `ai/budget.rs` 上下文预算基建，压缩是自然下一步；"切点+summary+选中路径自校验"的模型比"直接替换历史"安全——原消息不动（符合 elsewhen 不可变文化），压缩只是叠加的视图层注解。

---

## 十、总结：按架构层级的借鉴清单

| 层 | RikkaHub 机制 | Elsewhen 借鉴形态 | 价值/成本 |
|---|---|---|---|
| 事件协议 | GenerationEvent sink + 单一应用器 | Rust `StreamSink<GenerationEvent>` + Dart reducer | 一切流式能力的地基 |
| 中止语义 | 带 reason 的 AbortController（interrupted/replaced/deleted） | 直接翻译 Dart + Rust 两侧 | 小改动，消灭一整类竞态 |
| 节流管线 | 标脏→200ms 落库；33ms 合帧→指纹差分广播 | 事件落库批处理 + Dart 帧合并 | 流式配套，否则 O(N²) |
| 实例治理 | working-set 注册表（checkout/release + sweep 条件） | Rust 进程级 Store 单例/连接池，schema 检查挪启动 | **解决现有 database is locked** |
| 上下文工程 | 易变段冻结快照（前缀缓存稳定） | 段稳定性排序 + 冻结 | 纯 Rust，token 成本立竿见影 |
| **持久记忆** | 分层记忆 + save_memory 提议制 + pending 队列纪律 | 轻量版：SQLite 单层 + 提议制 + 容量/去重/快照纪律 | 补生态位（事实记忆），复用草拟确认文化 |
| 双通道注入 | FIFO 队列（事实源）+ steering（及时性） | 记录型/对话型消息分流 | elsewhen 场景特有，可超越原作 |
| 分支物化 | selectIndex 持久化 | `conversation_paths` 表存当前叶子 | 小改动，简化 UI 与上下文编码 |
| 内容戳 | wyhash 节点戳 + 可验证前缀合并 | (id, hash) 增量同步协议 | frb 序列化减负 |
| 压缩 | 切点+summary+选中路径自校验，视图层注解 | 配合 ai/budget.rs，不动原消息 | 符合不可变文化 |
| 运维细节 | instance-lock；INSERT OR REPLACE 教训；FTS trigram；损坏隔离；id 自愈 | 直接照搬 | 各 30-100 行，都是别人流过血的 |
| 出站请求 | 方言事实层（host 进、口径出）+ 极限目录三纪律 | 接第二家 provider 前先建方言层；output< context 自洽、宁小勿大、不要求不发 | 防漂移要在第一家时就位 |
| 消息富化 | 滞回截断 + tool 边界对齐 + 合成消息不落库 | **修 elsewhen 现存缺陷**：compress_context 无滞回（前缀缓存每轮灭）、无 tool 边界对齐（砍断即 400） | 高优先，现存 bug 级 |
| 时间感知 | time_reminder 合成消息（首条恒提醒，>10min 间隔提醒） | elsewhen 是时间敏感应用（"昨天记的事"），模型需要时钟 | 几十行，纯收益 |
| 错误分类 | 保守正则白名单分类链；错误类别决定恢复策略 | 限流（退避）vs 超上下文（压缩/换模型）分流；分析任务 backoff 更需要 | 50 行正则表起 |
| 辅助任务 | 三档哲学（静默跳过/报错档/回退档）+ 取消闸 + 期间变更作废 | digest/分析任务队列对照自查 | 设计原则，零代码 |
| 错误通道 | 环形缓冲 + 风暴合并 + 依赖注入防循环 | 统一错误中心视图 | 低成本高价值 |

### 补充盘点：按聊天系统组成逐一排查后的遗漏项

以下按"一个完整聊天系统由哪些子系统组成"系统盘点后补充，每项都是底层设计而非功能表面。

---

## 十一、出站请求层：方言与能力目录（被严重低估的一层）

聊天系统与 provider 之间最容易腐烂的接缝。RikkaHub 把它拆成三个纪律模块：

### 11.1 请求方言单源（`model-providers/request-dialect.ts`）

**背景**：同一个 provider，聊天引擎与工作区引擎各自决定请求形状必然漂移——kelivo 的终态（4100 行单文件、十几个厂商布尔散布）证明了不分层的代价。分层：

```
层1(本模块):引擎中性的「方言事实」——host 字符串进、口径结论出,纯函数零依赖
层2(各引擎翻译层):聊天引擎直接消费;pi 经 model-bridge 译成 compat
层3(平价回归测试):跨引擎 e2e 断言真实出站字节一致
```

方言事实的实证样本：
- 官方 OpenAI（api.openai.com / Azure）o 系/gpt-5 **硬性要求 `max_completion_tokens`，对 `max_tokens` 直接 400**；第三方兼容端点普遍只认 `max_tokens`，且对未知字段**静默忽略**（发错字段 = 上限失效、成本失控，不是报错）——所以 `openAiMaxTokensField(host)` 按主机身份二选一。
- `"developer"` 角色恒不发：官方对推理模型接受 `"system"` 并自动归一，第三方端点普遍拒收 developer（DashScope 400、火山方舟 missing input.role）。
- **收录门槛：跨引擎必须一致 + 有实证**。不做推测性厂商全覆盖——"没有实证的字段进来只会腐烂"。

### 11.2 模型极限目录（`model-providers/model-limits.ts`）

models.dev 目录原本只喂"统计分母"，查错只是分母略偏；P5 起它成了**出站请求字段**来源（Anthropic max_tokens 必填），同一份"尽力猜"从此决定请求成败：猜大一档就是硬 400（智谱 GLM-5.3 两模式同炸的报障）。三条纪律：

1. **按端点身份取值，不按名字猜**（三级阶梯解析，不按名字搜全目录——同名模型输出上限跨 provider 离散极大：中位 2 倍、p90 28 倍、最大 244 倍；而上下文窗口很稳，中位 1.02 倍——这正是缺陷长期潜伏的原因：原消费者只用那个宽容的字段）
2. **输出上限必须自洽**：output < context；目录 15.6% 的行 output == context（占位行）一律丢弃
3. **同 host 多命中取最小**：偏小只是答案截短（可感知退化），偏大是整个请求被拒（功能不可用），两个方向代价不对等

另一条同源纪律：**协议不要求上限时根本不发这个字段**。

### 11.3 对 elsewhen 的映射

elsewhen 是单 provider 配置（OpenAI 兼容），问题面小得多，但两条原则直接适用：
- `ai/budget.rs` 的 context window 属 provider config 的思路已对；输出上限如果未来要发，遵守"output < context 自洽 + 宁小勿大 + 不要求就不发"。
- 方言集中一处：elsewhen 的 provider.rs 若未来支持多家（Anthropic/Gemini 协议），先建"方言事实层"再接第二家，别等漂移后再收敛。

---

## 十二、消息富化层：DB 消息 ≠ 模型视野（`inference-engine/message-enrichment.ts`）

这是之前完全漏掉的一层：**把"DB 里用户写的消息"变成"模型该看到的消息"的全部文本加工集中一处**，两个引擎共用同一份裁决。四件套：

### 12.1 上下文滞回截断（hysteresis truncation）

朴素截断（每轮砍掉超出的最旧消息）会让上下文前缀**每轮都变**——前缀缓存全灭。滞回方案：起点按步长 `S=ceil(limit×0.2)` 量化前移，`floor((count-limit)/S)*S`——连续几轮追加消息，起点不动，前缀稳定。

配套 `alignContextStart` 安全边界回退：算术起点可能恰好压在一对工具调用中间，一旦切开，"无对应 call 的 tool result"发给上游 OpenAI/Claude 直接 400，**且因前缀稳定会连续多轮 400**。规则：R1 起点含已执行 tool → 前移到对应纯 call 的发起消息；R2 起点含纯 call → 归并到触发它的最近 USER（工具链入口）。只向前调整，不破坏保留下界。

**对比 elsewhen**：`compress_context` 按 token 砍，没有滞回（前缀每轮变）、没有 tool 边界对齐（elsewhen 有工具调用回传，ContextMessage.tool_call_id——砍断就是 400）。这两个都是 elsewhen 现有代码的**现存缺陷**，可直接按此修。

### 12.2 时间提醒（time reminder）

USER 消息前插 `<time_reminder>Current time: ... (N min since last message)</time_reminder>`：首条恒提醒，间隔 >10min 才提醒。刻意硬编码不开放给用户（"感知类节奏阈值，用户无需感知；10min 对齐桌面 IM 场景"）。对 elsewhen 的时间感知场景（"昨天记的事"）极有价值——模型没有时钟，事件时间戳都在 DB 里，这个机制让模型感知"对话中时间的流逝"。

### 12.3 合成消息纪律（幂等）

注入/提醒产生的是**合成消息，不写 DB**——每轮从 DB 原文重新富化，"DB 里永远不会沉淀出第二条 lorebook 正文"。`EnrichResult.syntheticIds` 显式列出合成 id：生成路径灌进编码器（挡在压缩切点映射之外），手动压缩经 `encodableMessages` 剥回纯真实行（**注入是配置不是对话，不进用户策展的摘要**）。消息对象零改动（不挂 metadata，不扩类型）。

### 12.4 模板变量 / 提示词注入（lorebook / mode injection）

`{{message}}` `{{model_name}}` `{{time}}` 等变量渲染；lorebook（世界书，按消息内容关键词触发的注入）与 mode injection 分层激活，会话级可覆盖助手级。elsewhen 没有多助手/角色扮演场景，这些优先级低；但**"富化层独立、合成不落库、syntheticIds 显式标注"的架构**在任何注入场景都适用——elsewhen 的 `recent_background`（`optional_background` 标记、不序列化给 provider）已经是这个思想的雏形，值得对齐到完整纪律。

---

## 十三、错误分类链：报错的"人话映射"（`inference-engine/provider-errors.ts`）

三家 provider 报错各说各话，原文透传给用户就是天书。分类链：`代理 ?? 超上下文 ?? 输出上限 ?? 限流 ?? 原文`。

关键设计原则：**模式刻意保守——误判（把正常报错说成限流，误导用户白折腾）比漏判（用户看到原文）更糟**。所以每个分类器都是正则白名单（覆盖 OpenAI/Claude/Gemini/DeepSeek/各家网关的实测报错格式），不命中返回 null 维持原文。限流文案附原始错误（常含可等待秒数）。

**elsewhen 现状**：`generate_reply` 失败直接把 anyhow error 文本显示给用户。这层 50 行正则表 + 分类链是纯收益，且 elsewhen 有重试场景（分析任务 backoff）更需要区分"限流（该退避）"和"超上下文（该压缩或换模型）"——**错误类别决定恢复策略**，不只是文案。

---

## 十四、辅助任务编排：模型角色分级（`conversations/auxiliary.ts`）

标题生成、追问建议、翻译、OCR、压缩全是独立辅助任务，共享一个模型解析纪律：

- **三档处理哲学**：标题/建议 = "静默跳过"档（fast model 未配置就不生成，不打扰）；OCR = "报错档"（"未配置不兜底、不静默"——OCR 只在聊天模型看不见图时被需要，跳过它等于把模型读不到的图悄悄丢进上下文，用户必须知情）。**同一系统里不同辅助任务按"用户是否需要知情"分档**，不是一刀切。
- 哨兵值防护：压缩模型未配置/已删除 → 回退会话模型，不能直接把 AUTO 哨兵交给 findModel——它查不到会兜底"第一个供应商 + 猜 auto→gpt-4o-mini"，对不提供该模型的服务商必 400。
- 压缩取消的两道闸：每分块前查 abort（取消不烧后续 LLM 轮次）；**落库前最后一道闸**（取消后 LLM 结果作废，绝不改写会话——压缩是破坏性替换，取消语义必须硬保证）。再加两道防线：会话在压缩期间被删除则结果作废（防无条件 upsert 把已删会话复活成"只剩摘要"的僵尸）；期间有写入（引用变更/长度变化）则整体作废。
- 进度展示不走 chatSuggestions 挪用（曾借建议条显示进度文本——挪用语义位且无法 i18n），并入 engine-status 帧。

**elsewhen 映射**：elsewhen 的分析/digest 任务队列已经有"辅助任务"概念，可借鉴分档哲学——哪些失败该静默重试（事件分析）、哪些必须让用户知情（导入失败）；以及**长任务落库前的取消闸 + 期间变更作废**这两条，digest 整批提交场景完全同构。

---

## 十五、文件/文档管线：子进程隔离（`files/extraction.ts`）

PDF/DOCX 全文提取为什么是"用完即弃的子进程"而不是主进程内解析：

1. **wasm 堆只涨不缩**——mupdf 解析过大 PDF 后，主进程永久占着峰值内存；子进程退出即归还，主进程内存曲线与文档大小彻底解耦
2. **崩溃隔离**——损坏/恶意 PDF 把 wasm 打崩时只死子进程，会话无感
3. **卡死可杀**——解析卡住可以直接 kill，主进程里的同步 wasm 杀不掉
4. **逐页进度**——子进程 stdout 逐行上报，前端轮询消费

单 exe 自孵化：`[process.execPath, ...argv.slice(1)]` 原样复刻启动命令，环境变量拐进 worker 分支（在绑端口/抢数据目录锁**之前**）。

**elsewhen 映射**：elsewhen 的目录导入（import_directory）目前应该都在 Rust 主进程。Rust 的内存管理比 wasm 好，但**崩溃隔离与卡死可杀**两条对解析用户提供的任意文件仍然成立——Rust 方案是 `tokio::process` 或简单地把解析放到带 timeout 的独立 task + catch_unwind 的边界意识。优先级中低。

---

## 十六、统一错误上报通道（`observability/app-errors.ts`）

"错误是横切关注点，本模块是它的唯一居所——catch 点要么 reportError，要么注明忽略原因"。

- 内存环形缓冲 200 条（**不落盘**——诊断信息重启清零可接受，避免 state.json 膨胀）+ console 镜像 + 注入式 SSE 广播
- **风暴合并**：同 domain+code+params 在 30s 窗口内只累加计数，不新增条目、不重复广播（从尾部线性扫最多 200 条找同源）
- 依赖注入防循环：SSE 广播函数由 api/sse.ts 启动时注入（observability 不依赖 api 层）
- code/params 分离：前端按 `settings:app_errors.codes.<code>` 用当前语言渲染，切语言即时生效；message 中文原文只做 console 镜像与键缺失兜底

**elsewhen 映射**：elsewhen 的错误目前散落各处（SnackBar 一次性展示、日志文件）。统一通道的价值在桌面应用同样成立：digest 失败、provider 失败、DB 失败都该进同一个"错误中心"视图，风暴合并防止 backoff 风暴刷屏。低成本高价值。

---

## 十七、其余快速结论

- **快照协商令牌**（`api/snapshot-negotiation.ts`）：重开 SSE 流时客户端带缓存令牌，一致则首帧只发轻量 meta（免全量重传）。令牌 = `updateAt:FNV-1a结构指纹`——updateAt 主判据，结构指纹（只遍历 id/长度，与文本长度无关）封堵同毫秒两次变更的碰撞窗口；**消息队列签名也并进令牌**（内存态变更必须让缓存失效，否则队列面板拿陈旧快照）。elsewhen 的 frb 场景可简化为 `(updateAt, len)` 元组。
- **请求日志与统计**（`api/logs.ts`）：按 provider/组（模型请求/搜索引擎/MCP）双维累计；stats 从 logs 拆出独立持久化，老用户一次性迁移不归零。elsewhen 已有 daily token usage，缺 by-provider/by-group 维度——多 provider 支持时的前置。
- **MCP OAuth**：授权服务器注入 Bearer 令牌，但**用户手配的 Authorization 头优先，不覆盖显式配置**——又一条"显式 > 自动"的不变式。
- **备份契约**：记忆导出格式标记"外部契约，格式不可变"（跨 PC/APP 平台）；导入 replace/merge 都重新分配 id。elsewhen 未来做导出/备份时，先冻结契约再实现。
- **skills 系统**：`use_skill` 工具按需加载技能正文（available_skills 只注入 name+description）——上下文经济的标准做法，elsewhen 的工具描述 prompt 已经很大，未来工具增多时可引入两级加载。



RikkaHub 每个模块头部写"病史"（方案为什么长这样、修过什么事故、哪次用户反馈改变了设计），关键决策对齐 Codex/安卓版出处。这让半年后的读者能区分"深思熟虑的取舍"和"没来得及收拾的临时方案"。最值得学的是**事故驱动的设计记录**（upsert-cascade、fork-repair、corrupt-quarantine 那样的测试+注释组合）——这类知识半衰期最长。建议 elsewhen 对同等级的决策采用同格式：现象 → 根因 → 不变式 → 回归测试。
