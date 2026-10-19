# 规则生命周期与主对话解耦——讨论与实施状态记录

- 日期：2026-10-19
- 状态：核心实施完成（`e7e9e4b`），后续观测消费的设计**未收敛**
- 相关：roadmap `2026-10-19-context-budget-and-tool-injection-roadmap.md`（R2B 未落地项 #1 部分化解）、迁移 35（事件提醒链）、迁移 53（MCP readOnlyHint）

## 背景与动机

个人经验规则库是产品初衷（记录日常时提醒可复用经验），不可砍。但基座
（system prompt）把全部 active 规则**全文注入**，规则数增长会让
stable_setup 线性膨胀，与 R2B 的 <3K 目标结构性冲突。用户挑战
「一问一答假设」后，收敛为主对话与规则校验解耦的架构方案，并立项实施。

## 一、已完成（提交 `e7e9e4b` + `911d761`，迁移 54）

### ① 基座瘦身
- SYSTEM_PROMPT_BASE 规则段改为触发纪律：分享踩坑/心得、或需引用既有
  规范 → 先调 `list_rules` 查库；`[规则提议]` 格式要求原文保留。
- PersonalRules 注入段不再搬规则原文，只播报规模（新增
  `count_active_rules`，计数不拉全量行）+ 指名查询入口。
- **stable_setup 从此与规则库大小脱钩**（R2B <3K 的规则侧障碍解除）。

### ② 规则查询
- 侦察发现 `list_rules` 工具已存在且常驻（schema ~30 tokens），未新造
  延迟工具——挂 R2A deferred 会触发「每个 deferred 工具需内建 skill 覆盖」
  不变量，得不偿失。只补强了 description 的触发场景描述。
- 这是计划与实际的偏差之一，已接受。

### ③ 对话影子校验流（迁移 54 `chat_rule_hits`）
与事件链同构的两阶段，独立成表（`rule_hits` FK 锚定 events、聊天消息
不进 events，不能混用）：

- **阶段一（预筛，零 LLM 成本，同步）**：execute completed 分支对当轮
  用户输入跑 `match_rules_for_text`（确定性关键词 contains、大小写折叠）。
  只有配了 `trigger_keywords` 的规则参与；`mode=off` 整体跳过。命中落
  `pending`；失败静默绝不阻塞对话；UNIQUE(rule_id,input_message_id)+
  ON CONFLICT DO NOTHING 幂等。
- **阶段二（判定，异步后台）**：`trigger_analysis` tick 新增
  `process_chat_rule_check_queue`，复用同一 provider，封顶 20 条/轮：
  极简 prompt（一条规则 + 消息原文，4K 截断，不确定宁判 false），
  回填 `applicable`（带 confidence）/ `excluded`；坏回包保持 pending
  下轮重试；resolve 只动 pending 行（重放不改写已定状态）。
- **pending 语义**：关键词命中但 LLM 未判定的代处理队列位（项目中有两
  个 pending：事件链 rule_hits.pending 由分析 LLM 消费；对话链
  chat_rule_hits.pending 由判定队列消费，结构同源互不通气）。
- **红线（已定并落实）**：影子校验绝不改写主回复、不向对话注入内容、
  不进事件卷宗、不跨设备同步（快照归类 local-event-reminders）。
- **观测面**：api `list_recent_chat_rule_hits` + 设置页规则库区
  「对话中的规则场景」只读审查面（pending=判定中 / applicable=已在对话
  遵守；excluded 收起不打扰）。无任何用户操作。

### 验收
Rust 887 全绿（+5：预筛/回填幂等、mode=off 闸门、判断队列坏回包重试、
基座不含规则原文断言、execute 全链路 SSE stub 集成测试）；release 零
警告；Dart 416 全绿；frb io.dart 毒化 0；release cdylib 重编。

## 二、已定的设计决策（不许翻案除非新证据）

1. **rules 表保持薄**：只存需要人确认的资产属性（content/status/闸门
   配置/提议来源）。机器产生的观测（命中、判定）不进 rules 列。
2. **观测流独立成表**：事件命中在 rule_hits、对话命中在
   chat_rule_hits，同构不混用；聊天消息不落事件卷宗是前提。
3. **影子校验无侵入红线**（见上）——「同一件事不做两遍」的边界。
4. **自动改写用户资产 = 红线**：任何基于判定的反哺（收紧关键词、退役
   规则）必须人确认，不做全自动。
5. **阈值哲学沿用**：不确定宁判 false（宁缺毋滥）；闸门开关与人共享
   （mode=off 两条链全关）。
6. **pending 双表双链**：明确两个 pending 的语义与本轮 goal 内不失焦
   （rule_hits 的 D1 分级 notified/aggregated/declined 属事件链，不在
   本轮）。

## 三、未定的讨论（设计缺口，待拍板）

以下为影子校验「生产数据但不消费数据」引出的缺口。2026-10-19 对话中
提出，用户尚未逐项拍板：

1. **判定结果无反馈通道**：LLM 判错（误报 applicable / 漏报 excluded）
   用户无法纠正，错误记录会一直挂着。是否给逐条「不适用 / 其实适用」
   按钮？纠正结果是否要沉淀（负知识？收紧关键词建议？）。
2. **applicable 触发了什么——答案是零副作用**：事件链有 instant 桌面
   提醒 / review_only 每日回顾两级，对话链只回填状态。是否在每日回顾
   聚合「本周对话中遵守了哪些规则」？还是保持纯审查面？**产品语义未
   定**。
3. **判定成本无上限保护**：封顶 20 条/轮，但 pending 会随聊天量积累，
   每条烧一次 LLM 调用。是否加每日判定配额 / 小模型快判链？
4. **与 [规则提议] 链的分工**：影子命中只说明「这轮聊到了规则相关的
   事」，不产出新规则；提议仍走生成链末尾标记。两条链共用关键词闸门
   但产物不同——此分工用户暂未明确表态。

### 关于「规则表要不要扩字段」的讨论（2026-10-19，未收敛）

- 用户提出：规则表现在只记一条内容，影子判定是否意味着要扩字段。
- 助手回应的分层建议（用户评价「不怎么对」，方向存疑、需重新讨论）：
  1. 派生视图优先：审查面升级为「每条规则的战绩面板」（近 N 次命中、
     适用率、消息摘录）——纯聚合查询，不加字段；
  2. 纠正权加在 hit 表：迁移给 `chat_rule_hits` 加 `user_override`
     列（用户覆盖 LLM 结论）而非扩 rules；
  3. 闸门反哺走建议流：excluded 占比高 → 审查面给「建议收紧关键词」
     按钮，点确认才改；零适用规则在每日回顾提示「考虑退役」。且建议
     等数据积累后再做。
- 用户的「不怎么对」具体指什么尚未说明——可能是对「rules 保持薄」的
  前提出有异议（例如希望规则本身携带更多类别/场景属性），后续讨论应
  先澄清用户的字段落点再出方案。

## 四、未完成 / 待办清单

1. 【讨论卡在对齐】规则表字段扩展方向：先澄清用户对上述分层哪里不满
   意，再谈字段 vs 派生视图 vs 迁移。
2. 【已识别未拍板】影子命中反馈通道（缺口 1）。
3. 【已识别未拍板】applicable 的下游消费（缺口 2，产品决策）。
4. 【已识别未拍板】判定成本上限（缺口 3，工程决策）。
5. 【有意不做、记录在案】清单/复核类：影子命中逐条操作的 widget 级
   测试（无命中注入通道，靠 Rust 侧测试守住）；R2B 「情绪回应」引导
   挪出 base（产品决策）；R2B 增量 diff 下发；R8E 有意拒绝。

## 关键锚点

- 提交：`e7e9e4b`（三件套）、`911d761`（roadmap 收口）
- 代码：`src/ai/memory.rs`（基座两段）、`src/ai/generation.rs`
  （预筛挂点）、`src/storage/rule_triggers.rs`（截图/判定/审查读面）、
  `src/api/mod.rs`（`process_chat_rule_check_queue`）、
  `src/api/rules.rs`（`list_recent_chat_rule_hits`）、
  `src/ai/tool/mod.rs`（list_rules 描述）、migrations/54
- 测试基线：Rust 887 passed / 19 ignored；Dart 416 passed；release 零警告
