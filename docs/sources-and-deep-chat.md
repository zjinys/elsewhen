# 来源层（Raw Sources）与深度聊天：设计讨论沉淀

> 状态：设计定案，**未实现** · 日期：2026-09-15
> 关联：`docs/llm-wiki.md`——已实现的 v1（事件 → wiki 页 → 认知推微）；本文是知识库 **v2 路线图**
> 性质：多轮用户交流的结论存档，后续实现以本文为准
>
> **2026-09-27：部分已被取代。** 实施以 [FR-PES-004](requirements/product/FR-PES-004-LLM-Wiki知识库闭环.md) 与 [ARCH-002](requirements/architecture/FR-PES-ARCH-002-LLM-Wiki知识库闭环技术设计.md) 为准：URL/文本导入已落地为 `source` 页、对话已有 `search_knowledge_base` 工具（§6「完全不注入」已过时）；CLI（含下文 `elsewhen wiki ingest`）已移除。本文保留为方法论/案例/规律与 `applicable_when`/`strength` 的设计讨论存档。

## 1. 背景：这几轮问的是什么

| 用户的问题 | 结论 |
|---|---|
| "知识库到底是什么？怎么建立？" | 概念澄清：知识库 = 关于用户的事实集合；是记录的副产物，不是要"建"的东西 |
| "我写的 / 别人写的有用内容放进来算什么？" | 它们是 **Raw sources（原始材料层）**，独立于事件的第三层原料 |
| "LLM wiki 适合放这个系统吗？会是双存储吗？" | 模式适合、文件形态不适合；**SQLite 唯一真源**，export 是只读快照 |
| "方法论 / 案例 / 规律放进来，系统怎么用？" | 四个消费通路 + `applicable_when`/`strength` 强度档（见 §4、§5） |
| "这些字段谁维护、何时维护？" | 三权分立：AI 起草 / 核心校验 / 人定夺（§4.3） |
| "能不能像咱俩这样对话题深度聊天？" | **管线已全通**，缺的是"知识底座"（§6） |

## 2. 概念澄清：三样东西各是什么

在这个系统语境下，原料分两类，提炼方向不同：

| 原料 | 提炼成 | 例 |
|---|---|---|
| **事件** `events`（你的日记，发生了什么） | **关于你的事实**（wiki 页） | "每周往返惠州 60 元" → `recurring-cost/通勤页` |
| **外部内容**（sources：你写的长文、别人的方法论、案例、规律） | **概念/主题/方法/案例页** | "BlaBlaCar 商业模式" → `case/blablacar`；"80/20" → `principle/pareto-8020` |

**为什么文章不能混进事件**：digest 的提炼规则是"关于用户的事实"，把外部内容塞进事件会污染画像（把文章内容硬套成"关于你"），且原始出处会丢。事件是生活流水，文章是想吸收的知识，不是一回事，必须分层。

关键认识：**知识库不是你要去"建立"的东西，而是持续记录事件的副产物**（将来还包括持续选源）。它没有"建好"的时刻，每天 digest 都在变准。

## 3. 存储形态定案

```
真源：SQLite（wiki_pages 等表）—— 唯一
  ├─ 应用内读取/浏览/编辑（Flutter GUI 是知识库的"IDE"，待做）
  └─ wiki export → 只读 markdown 快照（浏览/备份用，编辑不回写）
```

- **不是双存储**：export 是导出视图（类似表导成 JSON），方向单向。快照里改的东西会被下一次 export 覆盖，无任何回流——因此标注为"只读快照"。
- **不做 Obsidian 方向**（用户明确：不用，应用内看就行）。export 保留但降级为排查/备份工具。
- 已落地的一致性改动：README、`docs/llm-wiki.md` §4.4、CLI `wiki export` 输出（"真源在数据库，快照修改不会写回"）。

## 4. sources 层设计（v2 第一步）

### 4.1 数据与流程

```
sources 表（不可变原文 + 出处 + 时间）          ← 新 raw 层
    │  elsewhen wiki ingest <url|文件|文本>
    ↓  LLM 编译（提议）+ 核心确定性落地（复用现有合并机制）
method / case / principle 页（带 applicable_when + strength）
    ↓
消费：insight（透镜库）/ 规则（清单）/ 事件分析（参照）/ lint（审视）
```

### 4.2 编译形态

| 放进去 | 编译成 | 页面内容 |
|---|---|---|
| 某人的方法论（如对外合作五步法） | `method/<slug>` | 核心步骤 + 适用条件 |
| 案例（如 BlaBlaCar） | `case/<slug>` | 它做了什么 / 为什么成立 / 可迁移条件 |
| 规律（如 80/20） | `principle/<slug>` | 要点 + 常见适用场景 |

每页 frontmatter 必备两字段：

- `applicable_when`：**适用条件**（检索钩子）——该页何时该被拿出来用
- `strength`：**强度档**——`reference`（参考）/ `method`（方法）/ `rule`（规则）

### 4.3 素材形态细则：网页（含图片）如何存

**核心原则：存"抓取那一刻的快照"而非存活链接；正文是编译原料（纯文本），图片是资产（给人看）——分层处理。**

| 内容 | 存哪 | 怎么存 |
|---|---|---|
| 正文 | `sources.content_md` | 抓取 → 提取正文 → 转 markdown → **不可变快照**；`origin_url` 只记出处，不依赖其可用（网页会改会删） |
| 图片（默认） | 仅元数据 | 正文保留 `![alt](原url)` 占位 + URL 列表；不下载、不入库二进制；编译时跳过并提示"含 N 图已省略" |
| 图片（可选增强） | 文件系统目录（非 SQLite blob） | `--with-images` 下载到 `assets/<source_id>/`；asset 表存 `local_path / url / alt / hash / 尺寸`；图表/截图走多模态解读（v1 不做） |

**为什么图片默认不下载、且绝不进 SQLite blob**：绝大多数网页图是装饰性的，正文编译用不到；blob 入库膨胀、难备份、移动端沙盒难管理；文件系统 + 路径引用才贴合 Flutter 应用。图片是"回看原网页体验"的资产，不是"知识"本身。

### 4.4 素材形态细则：系列页面（3 个页面是一系列）如何组织

**核心原则：单篇是强事实（独立 source、独立溯源），系列是软组织（两个字段），编译时产出系列概念页。**

```
存储层：3 行独立 source（各自不可变、各自出处、各自溯源）
        + collection='系列slug' + seq=1,2,3       ← 软组织，不设硬约束
        ↓ ingest 同一 collection 的页面作为一组整体编译
编译层：series/<slug> wiki 页  ← 系列主旨 + 各篇要点 + 篇间递进/依赖
消费层：可逐篇引用（溯源到具体篇原话）或整体引用系列页
```

关键决定与理由：

1. **绝不合并成一个 source**——溯源粒度（哪条知识来自第几篇）与 revision 粒度会丢，上下文会爆炸
2. **collection 是软组织**——作者补篇、用户只喜欢其中 2 篇、篇序调整，都不动数据模型；series 页如实反映"只含你选的 1、3 篇，缺第 2 篇"
3. **系列页是编译产物**——"第 2 篇依赖第 1 篇的概念"这类篇间关系不是导入时让用户填的，是编译时提炼、随证据复利更新

CLI 形态：`wiki ingest url1 url2 url3 --series 对外合作方法论` → 3 个独立 source + `series/对外合作方法论` 页。

### 4.5 applicable_when / strength：谁维护、何时维护

**三权分立**（延续"AI 提议、核心校验、人定夺"铁律）：

| 角色 | 负责什么 | 权力边界 |
|---|---|---|
| **AI** | 起草 `applicable_when`，**建议** `strength`（默认 reference） | 只能提议 |
| **核心（Rust）** | 校验：strength 三档枚举、applicable_when 非空、长度上限；rule 级无确认不生效 | 不推断内容，只查边界 |
| **用户** | `wiki show / edit` 修改；**唯一的 rule 级确认权** | 最终权威 |

核心原则一条：**`applicable_when` 是检索钩子（质量资产，AI 起草、人可改）；`strength` 是权力开关（涉及打扰权，人说了算）**。AI 永远不能自己升自己为 rule——rule 生成硬提醒，升级必须走"待确认队列"由用户拍板（与既有"事件分类低置信 → 待确认"同一模式）。

**维护时机**：

```
T0 ingest     首次编译 → AI 写 applicable_when + 建议 strength
T1 digest     每轮证据强化该页时 → AI 顺带复核 applicable_when 是否仍准
              （例：顺风车事件让 BlaBlaCar 页更新，可迁移条件可能要修）
T2 用户主动   wiki show / edit；rule 级确认动作
T3 lint      （将来）启发式 lint 报"applicable_when 与页面内容脱节"
```

## 5. 消费通路：系统如何使用方法论 / 案例 / 规律

**① 认知推微：你的事实 × 你的透镜（soft）**
原来 4 个透镜写死在 prompt 里；有了 sources 后透镜库变成数据：

```
事实（wiki）             透镜（source 编译页）
recurring-cost/通勤页  ×  method/副业方法论 + case/blablacar + principle/pareto-8020
            ↓ 核心先确定性选页（applicable_when 匹配）
prompt：把这些透镜应用到用户的事实上
```

案例引用必须输出"适用性说明"（为什么这个案例适用），防乱套。

**② 规则提醒：你的方法论 = 硬性检查清单（hard）** ← 回答早前"规则"问题
规则的硬性清单来源 = 用户自己放进来的方法论。例：ingest《对外合作方法论》，标 rule 级 → 事件"开始和老王合作"触发时，检查清单**从该页面生成**（书面需求 / 付款条款 / 验收标准 / 保密协议）；方法论更新自动传播到未来清单。**知识驱动的规则：硬性、可追溯、由用户维护。** 规律（80/20）通常 reference/method 级，只给视角，不生成硬提醒。

**③ 事件分析时的背景参照（momentary）**
事件"老王项目，价格还没谈" → 轻量检索方法论页 → 分析结果带一句"Suggestion：该合作方法论第 2 步（付款条款）未确认"。

**④ 回顾 / 检查时的自我审视（lint）**
反向检查：你记录过的方法论，你自己遵守了吗？书面"付款前先验收"却已付款未验收 → 提示（不靠硬规则也成立）。

## 6. 深度聊天：能力盘点与缺口

### 6.1 现状：管线全通（真实代码，非构想）

```
Flutter UI（左右布局会话界面，git: "implement conversation UI with left-right layout"）
  → api.rs（create_conversation / send_message / generate_reply ~L317）
  → src/ai/conversation.rs：generate_conversation_reply
  → SlidingWindowMemory（默认 4096 token 滑动窗口）
  → OpenAiCompatibleProvider（.env：gpt-4o + hub.oaifree.com）
  → conversations + messages 表留痕
```

### 6.2 缺口：记忆孤岛

当前聊天上下文**只有本会话自己的滑动窗口**——不注入 wiki / 事件 / sources，不看知识库。"像咱俩对话那样"的深度聊天本质是 **会话 + 知识库的合成**，系统现在只有"会话"这一半。

### 6.3 补齐方案（小改动）

1. **话题钩子**：~~`conversation` 表已有 `tag` 字段~~ —— 修正：`tag` 是 CHECK 枚举（`diary / idea / discussion / general`），**不能**做自由话题检索。二选一：① 用会话 `title` + 最近消息提取关键词去检索 wiki（零迁移）；② 将来迁移把 `tag` 放开为自由文本
2. **注入**：`generate_conversation_reply` 拼上下文时，按话题钩子检索命中的 wiki 页 + 相关最近事件，作为 system prompt 注入，再接滑动窗口历史
3. 可选增强：流式输出（现为一次性返回）；对话中确认的决策按 digest 规则沉淀回 wiki

成本：每次回复多一次本地 wiki 检索，可忽略；质量上限 = gpt-4o + 注入质量。

## 7. 诚实的边界（设计必须防的坑）

1. **"该用哪个透镜"不能靠模型大海捞针**——几十个方法论页让它自己挑必出噪音；靠 `applicable_when` 让核心先确定性匹配，模型只负责应用
2. **方法论会反过来绑架用户**——`strength` 三档限权：reference/method 级默认只给 Suggestion，绝不打扰
3. **案例硬套**——引用案例必须输出适用性说明，核心校验
4. **真正崭新的主张依然少**——大部分输出是"你的事实 × 已知方法"的组合，期望管理

## 8. 落地路线（下一步的最小闭环）

1. `sources` 表 + `elsewhen wiki ingest <url|文件|文本>`（存源 + 编译 method/case/principle 页，带 applicable_when/strength，rule 级需确认）
2. 聊天注入：tag → wiki 检索 → system prompt（脱离记忆孤岛）
3. demo 验证：合作方法论 + BlaBlaCar 案例 + 80/20 三份来源，跑 digest/insight/chat 全链路，验证"透镜来自精选材料"与"方法论变检查清单"

> 用 demo 数据目录（`/tmp/opencode/elsewhen-demo`），不碰真实库。