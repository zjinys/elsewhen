# LLM Wiki 个人知识库 + 认知推微（设计说明）

> 状态：实现完成（v1） · 日期：2026-09-14
> 概念来源：Karpathy《[LLM Wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)》
> 工程参考：ColinThompson1/llm-wiki（Agent Skills 实现）

## 1. 为什么是这个方案

抖音视频《认知推微》里博主的做法：把日记数据**喂养进自己的知识库**，
AI 基于长期积累的"关于你的事实"反推反常识认知（例：每周东莞↔惠州往返过路费 60 元 →
在已付成本的节点上加"收钱动作"）。视频里强调副业**不是加法是乘法**——机会藏在
"反正都要做"的动作里，而非另找一件事。

一开始做过的"窗口扫描"方案（最近 N 天事件直接喂 prompt）是**缺陷设计**：
看不到跨月的模式、没有"越来越懂你"的积累、没有记忆递进。LLM wiki 思想补上了这三块。

## 2. 三层架构

```
┌─────────────────────────────────────────────────────────┐
│ ① 原始事件 events（不可变）   —— 你的日记，事实来源        │
│    用户只负责随手记录，raw_text 永久保存                  │
├─────────────────────────────────────────────────────────┤
│ ② wiki 页面 wiki_pages（AI 维护，核心合并）—— 编译知识     │
│    markdown 正文 + kind + 事件溯源 + 证据计数              │
│    recurring_cost / capability / asset / project /       │
│    relationship / decision / habit / constraint / insight│
├─────────────────────────────────────────────────────────┤
│ ③ schema（系统提示词 + Rust 确定性规则）—— 纪律            │
│    AI 只"提议"页面变更；核心校验 kind/slug、按事件 id 并集  │
│    合并、证据计数由核心计算、每次写回留 revision            │
└─────────────────────────────────────────────────────────┘
```

对应 LLM wiki 的三个操作：

| LLM wiki（Karpathy） | elsewhen 实现 |
|---|---|
| **Ingest** 消化源写回 wiki | `elsewhen wiki digest`：新事件 → LLM 提议页面变更 → 核心合并入库 → 追加 `wiki_log` |
| **Query** 读 index 导航 + 引用回答 | `elsewhen insight`：读索引 → 按四透镜选页 → 反常识认知 + `source_slugs` 溯源 |
| **好答案归档回 wiki**（探索复利） | 洞察写回 `insight/*` 页，`[[wikilink]]` 指向来源页 |
| **Lint** 健康检查 | `elsewhen wiki lint`：无溯源 / 无入链孤儿页（确定性） |
| index.md / log.md 导航 | `build_index_md` 按 kind 分组一行摘要 + 证据数；`wiki_log` 追加式可 parse 记录 |

## 3. 数据模型（迁移 v6）

- `app_meta(key,value)`：digest 游标 `last_digest_at` 等 KV
- `wiki_pages`：`slug UNIQUE, kind, title, summary, content_md(markdown), tags(JSON), source_event_ids(JSON), evidence_count, first/last_seen_at, status, created/updated_at`
- `wiki_revisions`：每次写回的 `content_md + reason + created_at`（可回滚/审计）
- `wiki_log`：追加式操作日志，`## [date] digest | created: ...; updated: ...` 可 grep
- `insights`（v5）：认知推微产物列表（wiki 页是其归档载体）

## 4. 关键机制

### 4.1 证据复利（核心）
同一个事实被 N 条事件支持 → `evidence_count = ∪ source_event_ids 的长度`。
`wiki digest` 第二次跑同类事件走 **update 合并**路径（不是新建重复页）——
demo 中 `dongguan-huizhou-commute` 从证据 1 → 证据 2，溯源事件自动追加。

### 4.2 确定性合并（AI 不越权）
`upsert_wiki_page`：
- 校验：kind ∈ 枚举、slug 格式（≤2 段、无空格）
- 已存在 → 更新内容字段 + 事件 id 并集 + 证据数由核心重算
- 不存在 → 新建
- 无论建/改都写一条 `wiki_revisions`

### 4.3 认知推微四透镜
`insight` prompt 的透镜集合（镜像视频方法论）：
1. **反复固定成本**：反正都要做/都要付出的动作 → 副业杠杆点
2. **闲置产能**：已有但未用的"空位"（时间、设备、技能、关系）
3. **加收钱动作**：在已有动作上叠收钱动作（乘法不是加法）
4. **案例类比**：已被验证的商业模式（如 BlaBlaCar 卖空座位）

导航取材：`INSIGHT_KINDS`（profile/recurring_cost/capability/asset/habit/constraint/decision/relationship）
按证据数 + 最近更新选页，字符上限控制上下文；过去洞察的标题注入 prompt 防止重复、支持递进。

### 4.4 存储形态与导出快照

**知识库唯一真源是 SQLite（`wiki_pages` 等表）**，不设第二存储——文件只是导出快照（视图），
编辑不回写，不存在双主同步问题。应用内浏览/编辑（Flutter GUI）才是知识库的"IDE"。

`wiki export <dir>` 可选地把真源物化成只读 markdown 快照：`index.md` + `log.md` + `<kind>/<slug>.md`
（YAML frontmatter：kind / evidence_count / sources / tags），供外部浏览或排查备份。
快照由真源重新生成，在快照文件里的修改会被下一次导出覆盖。

## 5. 文件清单

```
src/wiki.rs            digest 写回 / 索引 / 导出 / lint / 校验（新）
src/ai/insight.rs      认知推微 v2：wiki 导航 + 归档写回（重写）
src/storage.rs         wiki_pages / wiki_revisions / wiki_log / app_meta + 方法（迁移 v6）
src/main.rs            wiki 子命令 + insight 命令 + .env provider 首次导入
src/lib.rs             pub mod wiki
```

## 6. 已知边界 / 后续可做

- **Flutter 应用内浏览已完成；正文编辑**（知识库的"IDE"应是应用本身）见 [`wiki-editing-design.md`](wiki-editing-design.md) —— 主线为 AppFlowy Editor（含 digest 不覆盖人工编辑的保护机制、md⇄JSON 往返保真 spike、AI 对话嵌入编辑器）
- 知识库目前只以事件为原料；外部内容（文章/笔记）尚无 `sources` 层入口
- LLM 启发式 lint（矛盾/过期论断/缺失交叉引用）未做，只有确定性检查
- 页面级 cascade 更新（源头页 → 派生页的级联 index 更新）未做，digest 时页面间交叉引用较弱
- 洞察写回后未在来源页回挂"相关洞察"链接（未来可由 lint/补充 digest 完成）
- 未引入向量检索：当前规模下 index 导航足够（Karpathy 原话：~百级页面规模无需 embedding）
- 需要长期效果验证：让真实使用几周后看证据复利与洞察质量

> **v2 路线图**（sources 层 / 消费通路 / 深度聊天注入）：见 [`docs/sources-and-deep-chat.md`](sources-and-deep-chat.md)