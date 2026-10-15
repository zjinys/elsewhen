# 决策辅助闭环 Roadmap（决策辅助 + 情绪陪伴）

## 目标与归属

让积累在决策点生效：从「认知复利账本」长成「越用越懂你、既能帮你做决定、也能接住你情绪的私人陪伴」。记录的意义是**在下一个决策点把过去的自己叫回来**——规则触发提醒、分析指导、情绪承载都是这个终点的形态。

- 定位宣言、缺口分析、红线（原始决策记录）：[决策辅助闭环 Agent Note](../notes/proposed/architecture/2026-09-19-decision-support-loop.md)。**本文是该 note 的实施路线**，定位与不再重复。
- 服务[个人认知主循环总路线](2026-09-17-personal-cognition-main-loop-roadmap.md)的 4C 收尾与 Phase 5；与「事件可记录性过滤」（输入质量）是同一产品主线两面。
- 更新规则：每个切片完成立即更新本文件，附代码/测试证据；通过自动化验收不等于真人体验签收，真人签收另行安排。

## 执行状态

- **建立日期：** 2026-10-14，基于用户确认「转成 roadmap，从增量维护闭环 + 分层确认起步」。
- **代码基线：** `f7f6e74`（迁移 34；Rust 711 / Flutter 372 / analyze 0 error/warning）。
- **当前阶段：** D0–D5 已完成；下一步 D6（闭环度量与「被记得」，MVP 后首轮扩展收官）

| 切片 | 内容 | 依赖 | 状态 |
|---|---|---|---|
| D0 | 契约与 schema 设计冻结 | 无 | **完成（2026-10-14，本文件 D0 节即设计冻结）** |
| D1 | 规则触发引擎 + 分级提醒 | D0 | **完成（2026-10-14，`2aaa174`）** |
| D2 | 用户模型增量维护 + 分层确认 | D0 | ✅ 已完成（`7d77a96`） |
| D3 | 实体消歧与别名归一 | D2 | ✅ 已完成（`0b8a41e`） |
| D4 | 分析升级「提取 + 指导」（挂知识库与用户模型） | D2 | ✅ 已完成（2026-10-15，见 D4 节实现锚点） |
| D5 | 情绪维度与陪伴扩展 | D2 | ✅ 已完成（2026-10-15，见 D5 节实现锚点；含 D0 情绪存储映射修订） |
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
  | 情绪 | ~~`event_emotions` 独立表~~ **D5 修订：不建独立表**——情绪作为 v3 `emotion` 字段随 `event_analyses.result_json` 落库（快照同步自动携带、跨设备删除一致）；面板读/删直接派生/编辑该字段，事件真源不动 | D5 落实时回写本表（已回，2026-10-15） |
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

## D2 — 用户模型增量维护 + 分层确认（分水岭切片）——2026-10-14 完成（`7d77a96`）

**收益：** 模型从「批量提议的产物」变成「随使用自动生长的活账本」。

- [x] 新事件分析输出 → 增量合并管道：命中既有 slug 更新 `last_seen_at`/`confidence`，新实体走分层确认；**替换** `propose_people_relations_from_events` 的全量批量地位（保留为手动全量重建工具）。——实现锚点：`upsert_relation` 增量语义（同事件重放置信不变 / 新事件来源 +1 封顶 5，`src/storage/records.rs`）；worker改为标注判定分派（`src/api/mod.rs` `process_analysis_queue`：笛卡尔积草拟改为「标注组合 → 自动入库 or 草拟」）。
- [x] 分层确认：高置信（含事件 `@/#` 权威标注）自动入库；低置信才草拟确认；确认队列聚合展示防刷屏（同实体多条合并一条）。——实现锚点：`try_auto_upsert_annotated_relation`（双向标注 + 双页已存在 → confidence=4 直接入库，缺失页面降级草拟卡）；聚合防刷屏由既有 `propose_people_relations` 确认卡单卡分组自然承载（一组一卡，非逐边逐卡），未另建聚合层。
- [x] 每维度用户可见面：wiki network 区自然承载 person/project；模型面板（只读先行）列全部维度条目 + 来源事件跳转 + 删除（删关系边/别名，原始事件不动）。——实现锚点：设置页规则库 tab 新「用户模型」区（关系边列表 + 置信点条 1..5 + 最近强化时间 + 删除）；`listRelations`/`deleteRelation` 既有 API 直接复用，零新增 FRB 面。
- [x] 测试：幂等合并（同事件重放不重复计 confidence）、删除后不复活（事件不重放提取）、批量提议与增量并存不冲突。——实现锚点：`relations_upsert_confidence_increments_once_per_event`（storage/tests.rs）；`user_model_tests`（api/mod.rs：标注自动入库/未标注不自动）。删除不复活由「原始事件只分析一次、不重放提取」架构保证（reanalyze 是显式用户动作，接受重建）。
- **遗留意项（如实）**：①「来源事件跳转」未做——事件详情视图不存在（同 D1 事件旁标记遗留），relations 已存 `source_event_id`，跳转待事件视图出现后接；② 自动入库关系类型固定「参与」，不提取动词（契约不升版，语义完整性让位于不新增调用面）；③ 「锁定期跳过」（不变式 ⑤）尚未实现——锁定期（sync snapshot staging）内分析队列 claim 被 `background_processing_allowed` 挡住属于已有机制，但「跳过后可恢复补跑」无专门测试承托；④ 表格中「新实体走分层确认」现状：新实体（页面不存在）自动降级为草拟卡，草拟卡确认时自动建页（既有路径），未做独立「新实体提示」面。

## D3 — 实体消歧与别名归一——2026-10-14 完成（`0b8a41e`）

**收益：** 表干净可检索，是 D4/D5 检索质量的前提。

**先行核实（D0 之外又一项过期判断被推翻）：** `entity_aliases` / `entity_merges` / `entity_merge_snapshots` 三表 baseline 即有，`src/storage/entities.rs` 已实现合并/撤销/别名增删/事实迁移/快照回滚（`entity_merge_and_undo_*` 测试齐全），wiki 详情实体区 UI 已挂合并与撤销。本切片不新增 migration。

- [x] ~~migration：`entity_aliases` 表~~ 已存在（baseline），无需新建。
- [x] 归一管道：事件 people 字符串 → slug 归一规则——实现锚点：`resolve_entity_slug`（`src/storage/wiki.rs`，标题/别名不区分大小写命中、kind 相符且页面 active 才算归一，多义/无命中返回 None 绝不猜；已合并页不算命中——别名在合并时已迁至目标页）；worker 自动入库路径接归一解析（`src/api/mod.rs` `try_auto_upsert_annotated_relation`：别名命中即写 canonical slug，查不到降级草拟卡——slugify 直译是重复实体成因，已拔除）。**遗留（如实）：** 「疑似别名 LLM 确认」未做——机械归一 + 人工别名编辑（详情区既有）+ merge/undo 已覆盖主场景，LLM 建议面不新增调用面，挂账不闭。
- [x] 合并操作：已有（`merge_entity`/`undo_entity_merge`，relations/entity_facts/entity_aliases/todos 跟随迁移 + 快照回滚 + 安全拒改），UI 已挂（wiki 详情实体区）。本切片补齐「已合并页不得再被归一命中」语义。
- [x] 测试：归并后 D2 增量管道写 canonical slug（`alias_hit_writes_canonical_slug_and_survives_merge_replay`：合并后重放边指向新 canonical、不另立第二条边、不复活已合并页）；拆回后历史引用仍可溯（既有 `entity_merge_deduplicates_and_undo_restores_source_copies` 等）。

## D4 — 分析升级「提取 + 指导」

**收益：** 每条新记录得到「这和你的哪些积累有关、可复用什么经验」的回答。

- [x] v3 契约 `guidance` 字段落地（检索上下文已就位——2026-10-14 核实见「现状底座」）；追加检索面：人物关系（D2 模型）与近期相关事件条目；遵守输入预算口径（预算不足整体省略，必要材料不截断——既有预算纪律）。→ **实现（2026-10-15）：** `decision_support_material` 段级 fail-soft（规则段 8 条 → select_knowledge（2500 预算，唯一向上报错段）→ 新增关系账本段 `list_relations` take 10 → 新增近期记录段 `recent_events(7,5)` 每条截 120）；prompt 挂 guidance 可选段（slug 只取上下文出现页、无关联整字段省略）；worker 成功分支接 `wash_guidance`（`src/api/mod.rs:1134`）：杜撰 slug（`get_wiki_page` 不存在）丢弃、related/lessons/follow_ups_refined 各封 10 条 / 每条 500 字、三相全空整字段清除。
- [x] `guidance` 输出落地为事件旁的派生层内容（事件详情可见 + 知识库聚合视图），不写事件真源。→ **实现：** 只入 `event_analyses.result_json`（LEFT JOIN 最新一条派生，不改 events 真源）；读面 `EventAnalysisDetailDto.guidance`（FRB regen）；wiki 页详情实体区新 `_EntityGuidance` 聚合 section（`page_guidance_items` SQL json_each 匹配 `$.guidance.related[].slug` LIMIT 50 + 坏 JSON 降级 Rust 逐行解析 `page_guidance_items_fallback`，src/storage/events.rs）；来源事件 dialog 展示 guidance 区（wiki_detail_entity_sections.dart）。
- [x] insight 引擎衔接：guidance 高价值项可被罗盘吸收（只挂接，不重写 insight）。→ **实现：** `recent_guidance_lessons`（src/knowledge_background.rs）读最近 30 条分析的 lessons 去重取 10，追加进 `generate()` prompt 末尾（「可吸收提炼，不得原样照抄」）；任何失败静默返回空，生成路径不变。
- [x] 成本护栏：单事件检索条数上限、超限省略；对照 token 口径回归在门禁内。→ 实现：检索面 take 硬上限（规则 8 / 关系 10 / 近期 5×120 字）；wash 封顶 10×500；select_knowledge 2500 预算纪律不变；全量门禁绿。
- [x] 测试：无知识库设备（空库）分析路径不劣化不报错；检索失败降级为纯提取。→ **实现（Rust 723 / Flutter 375）：** `wash_guidance_drops_invented_slugs_and_caps_counts`（杜撰 slug 丢弃、全空整字段省略）；`decision_material_includes_relations_and_recent_entries`（关系段+近期段）；`guidance_flows_to_detail_and_page_aggregation`（detail 读面 + 页聚合构造数据走 complete_analysis）；既有 queue_persists 空库不劣化承托；空库决策材料段级返回空（fail-soft）。Flutter 桥面 `listPageGuidance` 未知 slug 空结果不报错（settings_bridge_test）。**遗留：** ①段级 fail-soft 已结构化承托「检索段故障降级为纯提取」，但除 select_knowledge 外段落单独故障无专测；②契约标识仍 `event-analysis-v3`（v3 内挂段未升版，读兼容无影响）；③guidance 无独立「采纳」度量入口（D6 回引时自然覆盖）。

## D5 — 情绪维度与陪伴扩展

**收益：** 从「帮你做决定」扩到「接得住你」——2026-09-19 定位校准的落地。

- [x] 情绪标注落地：事件分析 `emotion` 字段（对谁/对什么事/情绪标注）→ ~~`relations` 情绪边或独立标注表~~ **D5 修订：随 `event_analyses.result_json` 落库**（理由见上方 D0 映射表修订行）；**绝不生成情绪分析报告**。→ **实现（2026-10-15）：** prompt 挂 emotion 可选段（feeling 简短文本、禁止打分/生成报告、无则整字段省略）；worker 成功分支接 `wash_emotion`（空 feeling 丢弃、重复合并、每条/条数封顶 200 字×8、全空整字段清除，src/api/mod.rs）；读面 `list_event_emotions`（SQL json_each 扫 `$.emotion`，坏行静默跳过）+ `EventAnalysisDetailDto.emotion`；删除 `delete_event_emotion` = 编辑 result_json 的 emotion 字段（数组清空则整字段移除；定位键 analysis_id+target+feeling；**灰色语义：target 空串/缺失均归一为「对事件本身」**）。跨设备一致性：result_json 随 event_analyses 快照同步（snapshot.rs carried 清单），零新增同步接线。
- [x] 其余维度按 D0 映射逐个轻量落地：财务/健康/地点/身份角色/创作主题线——先只做「事件承载 + 可见可删」，不做量化模型；发现语义冲突即回 D0 修订而非硬塞。→ **实现：** D0 映射本就是「wiki network 页型约定（零新表）」——知识库详情/合并/别名/编辑 UI 已全套在位（D3 实体区）；用户模型面板补维度全集提示行（列明九个页型维度、指向知识库）即完成收口，无发现语义冲突。
- [x] 维度全集面板收口（D2 只读面板扩为全维度）。→ **实现：** 设置页「用户模型」区 = 关系账本（D2）+ 情绪标注小节（D5，可删单条；emotion 拉取失败独立降级不拖垮关系展示）+ 其余维度的知识库承载提示行。
- [x] 测试：情绪边与普通边互不污染；删除维度条目不影响事件真源。→ **实现（Rust 725 / Flutter 377）：** `emotion_flows_list_and_delete_leaves_event_untouched`（删除前后 events.raw_text 逐字节相等断言 + 数组清空整字段移除 + 空串 target→「对事件本身」）；`wash_emotion_drops_duplicates_and_caps`（重复合并、缺 target 保留、封顶 8）；Flutter 桥面空库空列表不报错、删除不存在 id fail-closed 报错。**遗留：** ①情绪标注与 relations 完全同字段隔离（emotion 独立字段、不动关系图）由结构保证，无「边互写」专测（结构上不可能）；②reanalyze 会重建 emotion（显式用户动作，D2 已确立接受重建，与 guidance 同口径）；③target 目前存原文不 resolve 实体 slug（用户模型面板直接展示原文，无歧义归一需求——多义场景若出现再回 D0）。

## D6 — 闭环度量与「被记得」

**收益：** 系统知道哪类提醒真有帮助；用户感到「被记得」而非「被检索」。

- [x] 提醒/指导 → 采纳 → 结果度量（接 todo/follow_ups 既有对象；只记录不推断）；用户可见自己的反馈记录，可清。→ **实现（2026-10-15）：** rule_hits 加 `feedback(helpful|not_relevant)+feedback_at`（migration 36，ALTER 无损）；`set_rule_hit_feedback` fail-closed（未知值报错、非已处置状态/不存在 id 返回 false；api 层只做 trim/转发）；反馈只记在本地命中行上，**没有任何自动行为消费它**（不外发、不自动调阈值、不自动关规则），仅设置页「最近提醒」计数与按钮展示（有用/不相关，再点一次清除）。**指导采纳未做独立度量**（guidance 无独立对象，如实遗留：走印象验收）；todo 完成即「结果」由既有 follow_ups→todo 机制承载，未另建度量。
- [x] 每日回顾勾连：回顾态聚合规则提醒 + 共同记忆回引（「你上周说过……」），摘要复用既有 daily_review 能力。→ **实现：** `get_daily_overview` 扩展 `reminder_hits`（当日命中，`rule_hits_on` 复用 daily_entries 同款本地日界）+ `memory_echo`（`memory_echo_for` 回看 7→8→9 天窗口原文快照，最多 3 条，不评价不推断）；API 读面完备，**daily overview 的完整回顾 UI 仍不存在（如实遗留）**——共同记忆回引的可见面由情感回访横幅承载（同一素材来源）。
- [x] 合并 [今日记忆真检索提案](../notes/proposed/2026-09-15-daily-memory-retrieval.md)（对话内「查看今日记忆」入口，真检索非推断——它的范围天然是本切片的对话出口）。→ **实现（采用提案注入方案而非走「对话内特殊命令」）：** 三条 prepare_context 路径统一注入 `today-recorded-events` 段——当日 daily_entries 原文（正序、每条 160 字、12 条封顶、`OmissionPolicy::OptionalBackground` 预算内可省）；AI 回答「今天记了什么」= 检索快照转述而非编造。**偏离与遗留：** 提案设想的「对话内入口/按需查询」以常驻可选背景段替代（零 UI 成本、自然覆盖任意问法）；当日无记录时不注入空态段（由零编造角色约束兜底，不为此每次多花一段）；knowledge_mentor/知识库页对话跳过（与 recent_background 同口径）。
- [x] 主动回访：低打扰情感性回访（可关）；触发面仅限回顾/打开应用时，**绝不做系统级推送轰炸**。→ **实现：** `companion_settings` singleton（migration 37：开关默认开 + 间隔天数下限默认 7 可配 3/7/14/30 + `last_revisit_at` 展示即记账）；`check_companion_revisit` 只在主界面挂载时调用一次（CompanionRevisitHint 横幅，fail-safe 收缩，点掉当次不再现）；关闭则**零查询零成本**（enabled 短路是最先的一行代码）；素材=间隔期外带情绪标注的旧记忆优先、否则最新一般事件（确定性取数，无权重推断）；排除于快照（local-event-reminders 域）；永不注册系统级推送/通知通道/后台唤醒。
- [x] 测试：度量只记不外发；回访频率上限可配；关闭回访后零侵入验证。→ **实现（Rust 732 / Flutter 379）：** `companion_revisit_gates_and_prefers_emotion`（无素材不记账、情绪候选优先、记录时间窗红线）；`companion_interval_is_enforceable`（3 天间隔未满不吃回访——频率上限可配）；`companion_disabled_returns_none_and_is_untouched`（关闭后即便素材与间隔都满足也 None 且 last_revisit 不动——零侵入）；`rule_hit_feedback_records_clears_and_fails_closed`（记录/清除/未知值报错/pending 不可反馈/不存在 id false）；`today_memory_background_is_real_retrieval`（当日事件注入为可选背景、无记录不注入）；`daily_overview_parts_available_from_store`（当日命中隔离昨日）；`memory_echo_and_day_hits_are_store_level_queries`；Flutter 桥面 fail-closed + 回访偏好往返。**遗留：** api 层 `set_rule_hit_feedback` 的 trim/空串校验与 `get/set_companion_prefs`、`check_companion_revisit` 无独立测试（该层薄且会经 AppConfig::load 打真实库，测试一律走 store 层——D6 起 api 层测试纪律固化为「逻辑下沉 storage、api 只转发」，故 D4 legacy 提到的「段落级故障无专测」同口径处理）；回访横幅无 widget 测试（桥面已覆盖）；「只记不外发」由无任何外发代码路径 + 反馈字段排除于快照（rule_hits 本就 excluded）双重保证，无专测（结构性事实）。

## 遗留收口（2026-10-15 起；主线外三件真实遗留，用户拍板「总是需要解决」）

- [x] **A：用户语义知识状态入同步集**——`knowledge_maintenance_reviews`（append-only，fingerprint 幂等键，收端 `INSERT OR IGNORE`，无 head/CAS）+ `knowledge_reading_states`（五态选择字段，LWW by updated_at，不上 CAS）入 op 增量同步与快照 carried 集；孤儿 page fail-closed；管道产物（jobs/runs/snapshots/metrics/topic_plans）永久排除写入门禁纪律。→ **完成（`78ef76b`）：** `KnowledgeReviewAppended`/`KnowledgeReadingStateChanged` 两个 mutation + 快照两种 kind 标签；测试覆盖双端各自关同一 issue 收敛一行、LWW 双向传播、快照往返 bootstrap 后增量续流（Rust 735 全绿）。**留二刀（如实挂账）：** `knowledge_review_decisions` 的 `proposal_id→knowledge_proposals` 外键使它必须连带 proposals（含 original_content 草拟正文，快照变重）后续入集，本轮不动。
- [x] **B：guidance 采纳度量**——复用 D6 rule_hit feedback 纪律（只记不外推）：独立 append-only 反馈表（analysis_id + 条目定位键 + helpful/not_relevant + created_at），wiki 详情页 guidance 条目加有用/不相关按钮；未消费即无自动行为。→ **完成（`f4c187f`）：** migration 38 `guidance_feedback`（定位键 (analysis_id, slug, note)——guidance.related[] 条目无稳定 id，slug/note 即用户可见文本；同文本重复条目共享一行反馈）；读面 `page_guidance_items` 主路径 LEFT JOIN 注入 feedback + 暴露 analysis_id（fallback 批量补查）；`set_guidance_feedback` fail-closed（未知值报错、analysis 不存在 false、None=删行清除）；详情页条目有用/不相关按钮（再点一次清除，乐观更新+失败回滚）；表排除于快照（本机度量不跨设备复制）。测试 Rust 736 / Flutter 380。
- [x] **C：事件详情视图**——事件读面收口 + 顺带闭 D1（事件旁规则命中标记，list_event_rule_hits）/D2（relations source_event_id 跳转）两笔旧账。→ **完成（`c53fafb`）：** 定案：事件是知识库的源材料非平级域——左栏 footer「事件」入口 + 主工作区第三态 `SidebarTab.events`（不塞侧栏闲逛路径、不动 wiki 多 tab 宿主）；`list_recent_events` api（days/limit clamp，列表不拖 result_json，详情走既有 `get_event_analysis_detail`）；宽屏双栏/窄屏单列，详情页含原文只读 + 分析摘要/跟进/指导（related 带回跳 slug）；D1：详情页相关规则提醒区块（list_event_rule_hits + 有用/不相关反馈，再点清除，乐观更新+回滚）；D2：wiki 页来源事件从纯文本 dialog 升级为跳转事件详情。Flutter 381。挂账（如实）：规则命中行有来源 rule_id 但未链到规则库详情页；事件列表无搜索/过滤（窗口 clamp 3650 天 ×50 条已够当前量级）。

## 首个可用范围与暂不做

- **首个可用：** D1 + D2 + D4 通过真人验收后，规则触发与事件指导构成可感知的决策辅助 MVP；D5/D6 属体验深化，不阻塞 MVP 发布判断。
- **暂不做：** 知识图谱/全量社交语义建模；情绪量化模型与情绪报告；地理数据（地点只做情感坐标）；跨设备同步用户模型与派生层（knowledge_* 入同步集另走门禁——依赖同步侧既有决策）；主动外推补全画像。
- **与人机边界的关系：** 写类操作（规则建议、关系提议）一律草拟确认；本路线不产生任何自动执行的业务动作。

## 风险

1. **提醒过载是本路线的头号杀死项**：D1 的分级与可配优先级高于功能丰富度；任何「默认全开」的设计都以失败论。
2. **分析成本上涨**（D4 挂检索）：预算纪律与省略计数是验收项，不是优化项。
3. **D2 若模型维护被做成主动劳动即失败**：增量管道是副产品，验收标准含「零新增用户义务」。
4. **情绪承载的信任脆弱**：D5 的可见性/可删性测试不达标不得进入真人验收；过度工程（报告化）同罪。
