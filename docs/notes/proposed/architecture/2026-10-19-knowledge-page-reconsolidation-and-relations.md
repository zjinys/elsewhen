# 知识页持续更新与关系网

- 日期：2026-10-19
- 状态：proposed → **P0/P1/P2/P3 全部已实施**（2026-10-20 评审采纳，分期推进）。
  P1 落面：`page_consolidation_status` 派生读（最近整理=wiki_revisions
  最新时间、新增证据=来源事件晚于该点个数，不加列不动快照版本）+
  `reconsolidate_page` 手动重整理 worker（preservation 铁律提示词、
  人工编辑页直接拒绝、账本 task='page-reconsolidation'）+ 详情 API
  consolidation 字段 + 详情页「重新整理此页」按钮与观测行。P2 的
  证据增量/时间触发后台化直接复用 `reconsolidate_page`（签名不变，
  provider 注入）。
  已落地：A 相关性选页（`select_context_pages_for_events`，词项打分，
  evidence 只做平票裁定——缺陷①主因已修）；B 提示词纪律 + 代码侧同义
  新建改写（kind+归一化标题+tag 重合才转 update，宁漏判不误并）；
  migration 修正：实施时应为 **52**（51 已被 todo-reminder-fires 占用）。
  P1/P2/P3 待做。
  **P2 已实施**（2026-10-20）：migration 52 `knowledge_reconsolidation_jobs`
  （generation 防并发、租约心跳、退避字段）；触发阈值为派生读（证据增量
  ≥3 → evidence_delta；距上次整理 ≥30 天且有新证据 → scheduled），不存
  证据基线列；每 tick 至多一页串行清理（死进程回收 + 指数退避 2^n·60s
  上限 6h），人工编辑的后台重排不调模型直接标记跳过；任务表排除出快照。
  **P3 已实施**（2026-10-20，瘦身为「相关页面」）：migration 52
  `knowledge_relations`（UNIQUE(from,to,relation)、origin CHECK）；
  mentions 边确定性派生（tags 逐字命中标题、差量重建）；人工 add/remove
  （自环/未知页/派生边删除/12 边上限全拒绝）；详情页「相关页面」行
  （chip 点击跳对端页、人工边 × 可删、「+」从页列表排除已有）。不做
  AI 写边、级联更新与全图遍历（后续看数据密度再议）。
- 范围：让 elsewhen 自己总结的知识页（habit / profile / topic / …）随新事件持续更新；并回答「是否需要内容-文档关系网、是否按关系级联更新」
- 性质：消化管线的增量演进；新增「页级再汇总」与「语义关系边」，复用现有依赖 / 刷新 / 版本机制
- 关联：[FR-PES-004 LLM-Wiki 知识库闭环](../../../requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md)、[docs/llm-wiki.md](../../../llm-wiki.md)、[摘录孵化闭环](../product/2026-10-19-clip-incubation-and-negative-knowledge.md)

## 1. 现象与问题

知识库里已有 `habit/午饭后散步习惯`、`profile/家庭情况` 这类 **elsewhen 自己总结的页**。后来聊天里又提到散步行为，但这些页没有变化。

这不是「模型不聪明」，而是**更新时机只有一次**：事件被消化那一刻，页面正文不一定在上下文里；之后没有任何任务回头重看。所以「持续更新」在当前架构里没有承载者。

## 2. 现状：一次性、事件驱动的消化

### 2.1 链路

| 步骤 | 行为 | 位置 |
|---|---|---|
| 入队 | 事件入库时同事务登记消化任务，沉淀 10 分钟 | `src/storage/digest.rs:96` |
| 组批 | 最多 20 条事件 / 6000 字 | `src/wiki.rs:1325` |
| 拼提示词 | 本批事件 + 全量索引 + 少量页面全文 | `src/wiki.rs:1344` |
| 落库 | `PreserveHumanEdits`，一个事务写页面 + 任务确认 | `src/storage/digest.rs:286` |
| 结束 | 成功即终态，**不回访** | `src/wiki.rs:1415` |

### 2.2 模型实际看到什么

1. **本批事件原文**（每条最多 4000 字）；
2. **全量索引**：每页一行 `slug — 摘要（N 条事件支持）`，按 kind 分组；
3. **页面全文**：只给 `select_context_pages` 选中的页，选择依据是 **证据数降序 → 最近更新**，总预算 8000 字，单页最多 4000 字（`src/wiki.rs:149`、`1344`）；
4. 约束：「已有页面需要更新时 `op=update`，给出全文新内容」「一条事件通常 0~1 个页面变更，最多 2 个」。

### 2.3 落库策略

- 页面若被人工编辑过（`human_edited_at` 非空）→ **正文不覆盖**，只并入证据，并生成一条修订建议（`src/storage/wiki.rs:382`）；
- 否则正文直接替换为模型给的全文；
- 旧正文写入 `wiki_revisions`（`src/storage/wiki.rs:950`），可回滚；
- 任务成功后不再重试；**目前没有手动「重新消化」入口**（`src/wiki.rs:1413` 注释明说）。

### 2.4 为什么会漏更新

| # | 原因 | 信号 | 怎么验证 |
|---|---|---|---|
| 1 | **选页只看证据数/时间，不看相关性**：低证据的 habit 页只以一行索引出现，模型拿不到正文 | 该页 `evidence_count` 没涨，或出现同主题新页 | 看页面证据数 / 是否有重复页 |
| 2 | **人工编辑保护**：正文冻结，只加证据 | 该 slug 出现在消化日志 `protected_slugs` | 设置 → 知识 → 知识消化；首页「知识审阅」 |
| 3 | **事件没进消化**：被判不可记录，或仍在沉淀/分析 | 任务状态 `skipped` / `pending` | 消化队列明细 |
| 4 | **模型保守**：「0~1 页」约束下随口一提被判新事实不足 | 该批 `updated_slugs` 不含目标页 | 消化运行日志 |
| 5 | **slug 漂移**：新建了同主题新页而不是 update | 出现 `habit/afternoon-walk` 之类重复页 | 索引里同义页并存 |

第 1 条是设计缺陷，也是本提案的主攻方向。

## 3. 已有的关系底座

先分清三层，不要混成一张图：

| 层 | 现有物 | 语义 | 同步 | 谁在用 |
|---|---|---|---|---|
| **类型/分面** | `wiki_pages.kind` / `tags` / `area` | 这页是什么 | 同步 | 索引、筛选、召回 |
| **来源依赖** | `knowledge_dependencies` | 「这页由哪些上游版本推出来」+ basis hash / epoch | 本地 pipeline（`snapshot.rs:167`） | `knowledge_refresh_jobs` 级联刷新、过期检测 |
| **语义关联** | `relations` | 人物 / 项目关系 | 本地 AI 产物（`snapshot.rs:194`） | 人物页、实体合并；**消化管线不用** |

**结论：关系网不是从零开始，但缺一层。**

- 事件总结出来的页之间**没有语义边**；
- `knowledge_dependencies` 只在「派生稿 / 页内任务 / 手工提炼」时写入（`storage/wiki.rs:260`、`chat_page_tasks.rs:225`、`authoring.rs:126`），**digest 从不建边**；
- 所以 `habit/午饭后散步` 与 `profile/家庭情况`、`habit/作息` 之间是孤岛。

### 3.1 为什么不直接复用 `relations`

- 它是 **slug 级**、面向人物/项目，由 `propose_people_relations` 写入；
- 不参与知识管线，没有版本 / basis 语义；
- 本地 AI 产物，不同步；
- 无法承担「依赖过期」的传播。

### 3.2 为什么语义边不能塞进 `knowledge_dependencies`

- 依赖边带 `basis` hash + `basis_epoch`，一条边变化会触发 `knowledge_stale_dependencies` **级联**；
- 语义关联（「散步习惯 ↔ 作息」）不是「由…推出」，没有不可变 basis；
- 混用会让「相关」被误判为「过期」，触发大量无意义重写。

→ **新增第三层：`knowledge_relations`（语义关联边）**，与依赖边并行、各司其职。

## 4. 提案 A：消化选页改为「相关性优先」

目标：新事件提到「散步」时，`habit/午饭后散步习惯` 的正文**必然**进上下文。

算法：

1. 从本批事件抽词项（复用 `knowledge::query_terms` 的思路，抽成可复用函数）；
2. 对 `participating` 页面打分：
   - 标题 / tags / summary 命中权重高；
   - 正文命中次之；
   - 1-hop 关系邻居加分（图存在时）；
   - `evidence_count` / `last_seen_at` **只做 tie-break**；
3. 命中页给全文（单页 ≤4000 字），未命中页仍保留索引行；
4. 总预算维持 8000 字。

改动：`src/wiki.rs` 新增 `select_context_pages_for_events(events, pages, relations, budget)`；旧函数保留给无事件场景。

验收：构造事件「今天午饭后走了四十分钟」+ 已有 `habit/午饭后散步`，断言其 `content_md` 出现在 prompt。

## 5. 提案 B：提示词与建页纪律

- 索引中已有同主题页时**必须 `update`**，不得新建同义 slug；
- 标题 / tags 命中的候选页必须逐页判断「是否更新」，并在 `reason` 里说明；
- 「宁缺毋滥」保留，但改成「一条事件 0~2 页，**允许 0**；与已有页相关时优先更新而不是新建」；
- `create` 前做 slug / 标题近似检查（kind + 归一化标题 + tags），命中则转 `update`。

## 6. 提案 C：页级再汇总（持续更新的根本解）

### 6.1 触发

- **证据增量**：`evidence_count - last_consolidated_evidence_count >= N`（默认 3）；
- **时间**：距上次再汇总 ≥ 30 天，且期间有新证据；
- **手动**：「重新整理此页」；
- **关系**：邻居页发生实质变更（低优先级，进审阅候选，不自动改）。

### 6.2 任务

新表 `knowledge_reconsolidation_jobs`（本地 pipeline）：

```
page_id TEXT PRIMARY KEY,
trigger TEXT,               -- evidence_delta | scheduled | manual | related
status TEXT,                -- pending | running | retry | succeeded | skipped
generation INTEGER,
attempts INTEGER,
available_at TEXT,
last_completed_at TEXT,
last_evidence_count INTEGER,
detail TEXT
```

新增 worker `run_knowledge_reconsolidation`，单页处理：

1. 取当前页 + 全部 `source_event_ids` 对应事件原文（按时间排序，按预算截断）；
2. 取 1-hop 关系邻居的摘要（预算内可给全文）；
3. 取来源快照（若有）；
4. 提示词要求：**保留旧事实、只增量修补、冲突标注（待确认）**；
5. 落库：
   - `human_edited_at` 为空 → 发布新正文 + 写 `wiki_revisions`；
   - 已人工编辑 → 只生成 `knowledge_proposals`（审阅卡），正文不动。

### 6.3 成本与防抖

- 单页一次、串行；每轮最多 N 页；
- 失败指数退避，复用 `knowledge_background_runs` 运行日志语义；
- 同一页用 `generation` 防并发（与 refresh 一致）；
- **不做**「一次事件触发全库重算」。

## 7. 提案 D：关系网 `knowledge_relations`

### 7.1 边的类型

| relation | 语义 | 方向 |
|---|---|---|
| `related` | 主题相关 | 无向（规范化存储） |
| `part_of` | 从属（如 午饭后散步 part_of 日常作息） | 有向 |
| `mentions` | 页面提到某人 / 项目 / 资产 | 有向 |
| `supports` / `contradicts` | 证据关系 | 有向 |
| `same_topic` | 同主题不同侧面 | 无向 |

> `habit` 是 `kind`（类型），**不是一个节点**。真正有用的是页与页的边；「这页是 habit」由 `kind` 承担，不要再造 `type:habit` 节点。

### 7.2 表结构（草案）

```sql
CREATE TABLE knowledge_relations (
  id TEXT PRIMARY KEY,
  from_page_id TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
  to_page_id   TEXT NOT NULL REFERENCES wiki_pages(id) ON DELETE CASCADE,
  relation TEXT NOT NULL CHECK(relation IN
    ('related','part_of','mentions','supports','contradicts','same_topic')),
  note TEXT,
  confidence INTEGER NOT NULL DEFAULT 3,
  origin TEXT NOT NULL CHECK(origin IN
    ('ai_digest','ai_reconsolidation','human','citation')),
  source_event_ids TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  last_confirmed_at TEXT NOT NULL,
  UNIQUE(from_page_id, to_page_id, relation)
);
CREATE INDEX idx_knowledge_relations_from ON knowledge_relations(from_page_id);
CREATE INDEX idx_knowledge_relations_to   ON knowledge_relations(to_page_id);
```

### 7.3 谁写边

- **digest**：提案 JSON 增加可选 `relations`，命中已有页时写 1~2 条；
- **reconsolidation**：可补边；
- **人工**：页面「相关页面」可增删；
- **引用**：`mentions` 可从 tags / 引用**确定性派生**（不调模型）。

写入前校验：页面存在、无自环、每页 ≤12 条、置信度阈值。

### 7.4 边怎么用（关键边界）

- ✅ **扩上下文**：选页时 1-hop 邻居进候选；
- ✅ **发现候选**：邻居变更 → 给邻居排一个「待复核」低优先级任务；
- ✅ **导航**：页面底部「相关页面」；
- ❌ **不自动改写邻居**：语义相关 ≠ 内容依赖。只有 `knowledge_dependencies` 允许自动级联刷新；
- ❌ **不做全图遍历**：最多 1-hop，防止一次事件引爆全库。

这条边界是安全阀：**关系网用来找上下文，依赖网用来传播变更。**

### 7.5 同步取舍

- 建议 v1 归入 `local-knowledge-pipeline`（与 `knowledge_dependencies` / `relations` 一致），配 `graph_version` + 惰性重建；
- 理由：边可由内容重建；快照面已经很大；
- 若后续要让「人工确认的关系」跨设备，再把 `origin IN ('human')` 的子集提升为 carried。

## 8. 分期

| 期 | 内容 | 风险 | 产出 |
|---|---|---|---|
| **P0** | A 相关性选页 + B 提示词 / slug 纪律 | 低 | 立刻减少漏更新 |
| **P1** | 页面显示「最近整理 / 新增证据数」+ 手动「重新整理此页」 | 低 | 可观测、可手动纠偏 |
| **P2** | C 页级再汇总 job（事件证据） | 中 | 系统性持续更新 |
| **P3** | D 关系网 + 关系驱动候选 / 导航 | 中高 | 跨页上下文与发现 |

## 9. 迁移与改动清单

> 实施实录（2026-10-20）：migration 编号用 52；边数据与 mentions 派生差量重建
> 落在 `src/storage/relations.rs`；设置页队列面板未做——任务观察先走 `list_reconsolidation_jobs` API。

- migration `52-knowledge-reconsolidation-relations`（`50`/`51` 已被占用）：新建 `knowledge_relations`、`knowledge_reconsolidation_jobs`；
- `src/storage/sync/snapshot.rs`：两表登记为 excluded（理由 `local-knowledge-pipeline`），同步门禁测试；
- `src/wiki.rs`：相关性选页、提示词、`DigestProposal.relations`；
- `src/knowledge_background/reconsolidation.rs`（新）：worker；
- `src/knowledge/relations.rs`（新）：边校验 / 写入 / 1-hop 查询；
- API：`reconsolidate_page(slug)`、`list_relations(slug)`、`add_relation` / `remove_relation`；
- UI：页面「相关页面」、重新整理按钮、状态行；设置页显示再汇总队列。

## 10. 测试

- 单测：相关性选页命中；slug 近似转 `update`；再汇总保留旧事实；人工编辑页只出 proposal；关系边自环 / 上限 / 权限；1-hop 不越界；
- 集成：事件 → digest → 再汇总 → `wiki_revisions` 有旧稿；
- 迁移：新表登记、快照门禁通过。

## 11. 非目标 / 风险

- **不做全库重建**：不要改 `DIGEST_VERSION`——`knowledge_digest_jobs` 有 `UNIQUE(event_id, digest_version)`，旧事件在新版本下没有任务，需要额外回填；
- **不自动改写关系邻居**；
- **不让关系网替代依赖网**；
- 首版不做向量检索，用词项 + 图；
- 成本：再汇总会多一次模型调用 / 页，靠增量阈值和冷却控制。

## 12. 开放问题

1. 再汇总默认阈值：证据增量 3？30 天？
2. 人工编辑页是否接受「自动应用低风险补充」，还是永远走审阅？
3. 关系边 v1 是否确定不同步？（建议否）
4. `part_of` 是否允许跨 kind（如 `habit/* part_of profile/日常作息`）？
