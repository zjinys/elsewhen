# 决策辅助闭环 Roadmap（决策辅助 + 情绪陪伴）

## 目标与归属

让积累在决策点生效：从「认知复利账本」长成「越用越懂你、既能帮你做决定、也能接住你情绪的私人陪伴」。记录的意义是**在下一个决策点把过去的自己叫回来**——规则触发提醒、分析指导、情绪承载都是这个终点的形态。

- 定位宣言、缺口分析、红线（原始决策记录）：[决策辅助闭环 Agent Note](../notes/proposed/architecture/2026-09-19-decision-support-loop.md)。**本文是该 note 的实施路线**，定位与不再重复。
- 服务[个人认知主循环总路线](2026-09-17-personal-cognition-main-loop-roadmap.md)的 4C 收尾与 Phase 5；与「事件可记录性过滤」（输入质量）是同一产品主线两面。
- 更新规则：每个切片完成立即更新本文件，附代码/测试证据；通过自动化验收不等于真人体验签收，真人签收另行安排。

## 执行状态

- **建立日期：** 2026-10-14，基于用户确认「转成 roadmap，从增量维护闭环 + 分层确认起步」。
- **代码基线：** `f7f6e74`（迁移 34；Rust 711 / Flutter 372 / analyze 0 error/warning）。
- **当前阶段：** D0（设计冻结）与 D1（规则触发引擎）已完成（同日）；下一步 D2（用户模型增量维护）

| 切片 | 内容 | 依赖 | 状态 |
|---|---|---|---|
| D0 | 契约与 schema 设计冻结 | 无 | **完成（2026-10-14，本文件 D0 节即设计冻结）** |
| D1 | 规则触发引擎 + 分级提醒 | D0 | **完成（2026-10-14，`2aaa174`）** |
| D2 | 用户模型增量维护 + 分层确认 | D0 | 未开始 |
| D3 | 实体消歧与别名归一 | D2 | 未开始 |
| D4 | 分析升级「提取 + 指导」（挂知识库与用户模型） | D2 | 未开始 |
| D5 | 情绪维度与陪伴扩展 | D2 | 未开始 |
| D6 | 闭环度量与「被记得」（主动回引 + 今日记忆检索合并） | D1、D4 | 未开始 |

## 不变式（继承 note 红线，实现必须逐一可达）

1. **只整理已托付的，绝不主动索取**：模型只来自用户记录，不做外推式画像补全。
2. **透明可看可改可删**：用户模型的每个维度都有用户可见的查看/修改/删除入口；无暗账。
3. **副产品原则**：用户模型零主动维护劳动，随事件入库增量更新；任何需要用户周期性整理的设计都是错的。
4. **事实与解读分层**：指导、情绪标注、关系推断全是派生层；原始事件不可变、可重算。
5. **分级防打扰**：即时提醒仅限高置信且用户可配；其余一律聚合进每日回顾；主动回访低打扰、可关。
6. **陪伴 ≠ 窥探**：情绪承载 ≠ 情绪分析报告；以用户感受优先，不以模型完备性为目标。
7. **平行独立**：各维度互相独立、与事件账本/规则库/知识积累并列；不做大一统数字分身、不做知识图谱。
8. 写类操作延续草拟确认制；Provider/本地预算边界不因检索上下文而突破。

## 现状底座（2026-10-14 D0 核实；修订 note 过期判断）

note（2026-09-19）的三个缺口判断有两条已随管线演进过期，以本节为准：

- `relations` 表（migrations/01：from/to_slug+kind、relation、confidence、source_conversation_id、last_seen_at、source_event_id、UNIQUE 三元组）——结构化模型的事实存储已就位。
- **分析管线已挂知识库与规则库**（修正 note「完全不碰」判断）：`decision_support_material`（src/api/mod.rs）在分析 prompt 注入规则前 8 条 + `knowledge::select_knowledge`（analysis 场景、2500 预算）+ 引用校验（`validate_answer_citations`/`bound_model_context` 24000）。**已检索 ≠ 已触发**：规则只是参考文本，无结构化条件、无命中判定、无提醒形态——D1 的缺口仍在。
- 可记录性过滤**已落地**（`recordable` + `kind`∈event/discussion/chitchat/meta，note 的姊妹提案不再是待办）。
- 输出契约 `EventAnalysisV1`（`deny_unknown_fields`）**无 guidance/emotion 字段**；`prompt_version` 入库但恒为 `event-analysis`（v1/v2 为 legacy 标识）。
- **同步链路对契约升级安全**：`result_json` 以不透明字符串过 wire（`sync/analysis.rs` apply 端 INSERT 不 parse），`analysis_task_id` 任务类型名与结果契约解耦；读取端（`event_analysis_detail`）为本机单点。
- 罗盘 insight（`src/ai/insight.rs`）具备「最近事件 + wiki」挖掘回写；D4 只挂接不重做。
- 人物关系批量提议 `propose_people_relations_from_events`（全量扫描 + 样本上限）——D2 改增量、D3 消歧后退役或降级为手动全量重建工具。

## D0 — 契约与 schema 设计冻结（2026-10-14 完成）

**收益：** 四个实施切片共享同一份契约，避免各做各的再返工。

- [x] 事件分析契约升级设计：`event-analysis-v3`（经核实现行 `deny_unknown_fields` + 同步不透明搬运 + `prompt_version` 入库，升级安全，详见上「现状底座」）。新契约 = 现 V1 全部字段 + 三组新字段，**均 `serde(default)`**（旧结果可读；读取端 accept v1/v2/v3，旧读端不存在跨端 parse）：
  - `guidance: Option<Guidance>`：`{related: [{slug, note}], lessons: [string], follow_ups_refined: [string]}`——related 与 lessons 同受引用校验约束；洞察高价值项由 insight 引擎吸收（D4 挂接）。
  - `emotion: Option<Vec<EmotionTag>>`：`{target: Option<string>, feeling: string}`——**只有文本标注，无数值评分**（红线：不是情绪分析报告）；target 空则视为对事件本身的感受。
  - `rule_confirmations: Vec<RuleConfirmation>`：`{rule_id, applicable, confidence}`——**规则确认并入同一次分析调用**（prompt 段「以下规则预筛命中，判定适用性」），一条事件一次 provider 调用完成提取+指导+情绪+规则确认，不新增调用面。
  - `prompt_version` 写 `event-analysis-v3`；`analysis_task_id` 任务名保持 `event-analysis` 不变。
- [x] 规则触发 schema（migration 35，D1 执行）：`rules` 增 `trigger_scene TEXT`（capture/conversation/daily_review，NULL=全场景）与 `trigger_keywords TEXT`（JSON 字符串数组，NULL=不参与预筛，存量规则零改动）；新表 `rule_hits(id, rule_id→rules ON DELETE CASCADE, event_id→events ON DELETE CASCADE, status∈{pending,notified,aggregated,declined}, created_at, updated_at, UNIQUE(rule_id,event_id))`。预筛在事件入库时刻同步跑（手写扫描大小写折叠，无 regex 依赖）；命中建 pending hit，由分析调用判定后转 notified/aggregated。
- [x] 用户模型维度全集 → 存储映射：
  | 维度 | 存储 | 备注 |
  |---|---|---|
  | 人物/项目/关系 | `relations`（已存在） | D2 增量管道专写 |
  | 别名 | `entity_aliases`（D3 新表） | alias↔canonical slug |
  | 目标 | `goals` 表引用 | 不重复建模 |
  | 情绪 | `event_emotions`（D5 新表：event_id, target_slug?, feeling, created_at） | 独立表（附着事件而非关系图），从 v3 `emotion` 字段落库 |
  | 承诺/复盘/节奏/偏好红线/健康/财务/地点/身份/创作线 | **wiki network 页型约定**（新 kind 页） | 零新表；透明可改可删天然满足，检索走 `select_knowledge`；发现语义冲突回本表修订而非硬塞 |
  每维度「查看/改/删」UI 面在 D2（只读面板起步）与 D5（全维度）交付。
- [x] API 面（按切片）：D1 `list_rule_hits`/`set_rule_reminder_prefs`（事件旁标记走既有事件详情）；D2 `list_user_model_entries`/`delete_relation_edge`/增量确认入口；D3 `merge_entities`/`split_entity`/`list_alias_suggestions`；D4 guidance 进既有 `event_analysis_detail`；D6 `list_reminder_feedback`/回访偏好。
- [x] 完成记录：即本次修订（2026-10-14，基线 `f7f6e74`）。核实证据：`src/api/mod.rs`（`EventAnalysisV1`、`process_analysis_queue`、`decision_support_material`）、`src/storage/events.rs`（`complete_analysis`/`fail_analysis`）、`src/storage/sync/analysis.rs`（不透明搬运 + 任务 ID 解耦）、`src/storage/records.rs`（rules 表读取面）。

## D1 — 规则触发引擎（决策辅助的最直接形态）——2026-10-14 完成（`2aaa174`）

**收益：** 用户踩过的坑在新事件发生时真正出声，不再只在对话里被动提及。

- [x] migration 35：`rules.trigger_scene`/`trigger_keywords`（NULL = 不参与预筛，存量零改动）；`rule_hits`（UNIQUE(rule_id,event_id) 幂等）+ `rule_trigger_settings` 偏好单行（instant/review_only/off）。
- [x] 事件入库事务内挂预筛（`src/storage/rule_triggers.rs`：大小写折叠 + 包含匹配，失败静默放行）；命中随**同一次分析调用**判定（v3 契约 `rule_confirmations`，零新增调用面）。
- [x] 分级提醒：confidence ≥0.8 → notified（设置页「最近提醒」+ 事件读面 `list_event_rule_hits`）；其余 applicable → aggregated（回顾聚合）。off 时预筛零成本。
- [x] 测试：预筛确定性/幂等（4 个模块测试 + 桥接往返）；判定按阈值与模式分级、只处理 pending 行。
- [x] 真人验收项：设置页规则库新增「事件提醒」偏好三态、每条规则触发条件编辑、「最近提醒」列表。
- **遗留意项（如实）**：①「事件旁标记」暂由设置页最近提醒列表承载——app 尚无事件列表/EventCard 挂载视图，待事件视图出现时接 `list_event_rule_hits`；②系统级桌面通知未做（需 flutter_local_notifications 依赖与各平台配置，当前只有应用内可见面），不承诺全平台；③ 锁定期跳过逻辑属 D2 分析队列部分，D1 未涉及。

## D2 — 用户模型增量维护 + 分层确认（分水岭切片）

**收益：** 模型从「批量提议的产物」变成「随使用自动生长的活账本」。

- [ ] 新事件分析输出 → 增量合并管道：命中既有 slug 更新 `last_seen_at`/`confidence`，新实体走分层确认；**替换** `propose_people_relations_from_events` 的全量批量地位（保留为手动全量重建工具）。
- [ ] 分层确认：高置信（含事件 `@/#` 权威标注）自动入库；低置信才草拟确认；确认队列聚合展示防刷屏（同实体多条合并一条）。
- [ ] 每维度用户可见面：wiki network 区自然承载 person/project；模型面板（只读先行）列全部维度条目 + 来源事件跳转 + 删除（删关系边/别名，原始事件不动）。
- [ ] 测试：幂等合并（同事件重放不重复计 confidence）、删除后不复活（事件不重放提取）、批量提议与增量并存不冲突。

## D3 — 实体消歧与别名归一

**收益：** 表干净可检索，是 D4/D5 检索质量的前提。

- [ ] migration：`entity_aliases` 表（alias ↔ canonical slug，来源标注）。
- [ ] 归一管道：事件 people 字符串 → slug 归一规则（大小写/空白/常见后缀）→ 疑似别名 LLM 确认（草拟确认制）。
- [ ] 合并操作：人工把重复 slug 归并（wiki 挪页语义），引用与 relations 边跟随迁移；误合并可拆回。
- [ ] 测试：归并后 D2 增量管道写 canonical slug；拆回后历史引用仍可溯。

## D4 — 分析升级「提取 + 指导」

**收益：** 每条新记录得到「这和你的哪些积累有关、可复用什么经验」的回答。

- [ ] v3 契约 `guidance` 字段落地（检索上下文已就位——2026-10-14 核实见「现状底座」）；追加检索面：人物关系（D2 模型）与近期相关事件条目；遵守输入预算口径（预算不足整体省略，必要材料不截断——既有预算纪律）。
- [ ] `guidance` 输出落地为事件旁的派生层内容（事件详情可见 + 知识库聚合视图），不写事件真源。
- [ ] insight 引擎衔接：guidance 高价值项可被罗盘吸收（只挂接，不重写 insight）。
- [ ] 成本护栏：单事件检索条数上限、超限省略并计数；对照 token 口径回归在门禁内。
- [ ] 测试：无知识库设备（空库）分析路径不劣化不报错；检索失败降级为纯提取。

## D5 — 情绪维度与陪伴扩展

**收益：** 从「帮你做决定」扩到「接得住你」——2026-09-19 定位校准的落地。

- [ ] 情绪标注落地：事件分析 `emotion` 字段（对谁/对什么事/情绪标注）→ `relations` 情绪边或独立标注表（按 D0 设计）；**绝不生成情绪分析报告**。
- [ ] 其余维度按 D0 映射逐个轻量落地：财务/健康/地点/身份角色/创作主题线——先只做「事件承载 + 可见可删」，不做量化模型；发现语义冲突即回 D0 修订而非硬塞。
- [ ] 维度全集面板收口（D2 只读面板扩为全维度）。
- [ ] 测试：情绪边与普通边互不污染；删除维度条目不影响事件真源。

## D6 — 闭环度量与「被记得」

**收益：** 系统知道哪类提醒真有帮助；用户感到「被记得」而非「被检索」。

- [ ] 提醒/指导 → 采纳 → 结果度量（接 todo/follow_ups 既有对象；只记录不推断）；用户可见自己的反馈记录，可清。
- [ ] 每日回顾勾连：回顾态聚合规则提醒 + 共同记忆回引（「你上周说过……」），摘要复用既有 daily_review 能力。
- [ ] 合并 [今日记忆真检索提案](../notes/proposed/2026-09-15-daily-memory-retrieval.md)（对话内「查看今日记忆」入口，真检索非推断——它的范围天然是本切片的对话出口）。
- [ ] 主动回访：低打扰情感性回访（可关）；触发面仅限回顾/打开应用时，**绝不做系统级推送轰炸**。
- [ ] 测试：度量只记不外发；回访频率上限可配；关闭回访后零侵入验证。

## 首个可用范围与暂不做

- **首个可用：** D1 + D2 + D4 通过真人验收后，规则触发与事件指导构成可感知的决策辅助 MVP；D5/D6 属体验深化，不阻塞 MVP 发布判断。
- **暂不做：** 知识图谱/全量社交语义建模；情绪量化模型与情绪报告；地理数据（地点只做情感坐标）；跨设备同步用户模型与派生层（knowledge_* 入同步集另走门禁——依赖同步侧既有决策）；主动外推补全画像。
- **与人机边界的关系：** 写类操作（规则建议、关系提议）一律草拟确认；本路线不产生任何自动执行的业务动作。

## 风险

1. **提醒过载是本路线的头号杀死项**：D1 的分级与可配优先级高于功能丰富度；任何「默认全开」的设计都以失败论。
2. **分析成本上涨**（D4 挂检索）：预算纪律与省略计数是验收项，不是优化项。
3. **D2 若模型维护被做成主动劳动即失败**：增量管道是副产品，验收标准含「零新增用户义务」。
4. **情绪承载的信任脆弱**：D5 的可见性/可删性测试不达标不得进入真人验收；过度工程（报告化）同罪。
