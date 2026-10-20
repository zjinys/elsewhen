# Elsewhen 项目深度代码 review（全部模块）

**文件路径**：`/data/playground/ai/elsewhen/docs/review_logs/$(date +%Y-%m-%d)/review_$(date +%Y%m%d_%H%M%S).md`
**创建时间**：$(date)
**review 模块**：全部模块（storage / wiki / ai / flutter / sync）
**状态**：已保存

---

## 📋 Elsewhen 项目深度代码 review（全部模块）

**项目定位**：本地优先的个人认知系统（Rust 核心 + Flutter UI），核心原则：原始事件不可变 + AI 仅提案 + 知识复利（LLM Wiki）

### 1. 存储层（src/storage/）—— 核心原子性与不可变性
**review 结果**：**9.1/10（优秀）**

**核心文件**：
- `events.rs`：`Store::insert_event` 原子事务 + `enforce_event_immutability` trigger
- `digest.rs`：`enqueue_digest_job_on`、`claim_digest_batch`、`apply_digest_batch`、`fail_digest_batch`、`recover_interrupted_digest_jobs`
- `migrations/`：schema v6~v10

**优点**：
- 原始事件原子插入 + 触发器保护
- 知识消化全自动：沉淀窗 → 可记录性决策 → 分批处理 → 同一事务提交
- 人工编辑保护 + 证据并集 + 连续失败5次冷却6h
- 全部只读视图（`digest_job_stats`、`list_digest_jobs`、`list_digest_runs`）

**具体问题**：
- 触发器写死在 `events.rs`，未来新增写路径可能遗漏
- batch size 固定为 5，缺少动态配置
- 缺少 `evidence_snapshot_json` 列

**修复建议**：
- 把 trigger 抽成 `enforce_event_immutability(tx, event_id)` 函数统一调用
- 把 batch size 改成配置项（`max_events` + `max_chars` 动态计算）
- 新增 schema v11：`wiki_pages.evidence_snapshot_json` JSON 列

### 2. Wiki 知识库层（src/wiki.rs）—— 复利引擎
**review 结果**：**9.3/10（优秀）**

**核心功能**：
- `WIKI_KINDS` 枚举（profile/person/…/memo）
- `validate_slug`、`slugify`、`unique_slug`
- `build_index_md`（按 kind 分组 + 证据数排序）
- `select_context_pages` + `select_context_pages_for_events`（事件驱动相关性选页）
- `redirect_synonym_creates`（同标题同人自动 merge）

**优点**：
- 三层架构清晰（raw_event → wiki_page → schema）
- LLM 只提案 + 核心确定性合并（evidence 并集、人工保护）
- 认知推微导航（INSIGHT_KINDS）
- 近似标题检查 + 冲突避让

**具体问题**：
- `call_provider` 在 test 中返回固定字符串，生产环境依赖 `OpenAiCompatibleProvider`
- 长原文分块阅读（6000-char chunks）已在 roadmap 中，但当前未实现

**修复建议**：
- 把 `call_provider` 改成 `async` + 工具调用（`get_event_evidence`、`suggest_wiki_merge`）
- 实现 lazy-loading 缓存（新增 `knowledge_reading_cache` 表）

### 3. AI 层（src/ai/）—— 容错与预算控制
**review 结果**：**9.4/10（优秀）**

**核心文件**：
- `provider.rs`：`OpenAiCompatibleProvider` + 共享 HTTP client 池 + 错误截断
- `budget.rs`：`RequestBudgetError` + `fit_request` + 工具结果占位化 + 历史裁剪 + 补充材料截断

**优点**：
- 预算控制极严（input_tokens + output_reserve + 上下文溢出识别）
- 工具结果早期占位（保留最近3次）
- 补充材料截断声明（防止用户误信）
- provider 错误体截断（防止敏感信息泄漏）

**具体问题**：
- 长回复 JSON 解析可能卡死（markdown fence 回退逻辑未优化）
- 缺少 `get_event_evidence`、`suggest_wiki_merge` 等专用工具

**修复建议**：
- JSON 解析改成双阶段（`serde_json::from_str` + fallback_markdown）
- 增加工具调用（返回 JSON 结构便于 AI 使用）

### 4. Flutter UI 层（ui/）—— 现代跨平台
**review 结果**：**8.5/10（良好）**

**核心文件**：
- `ui/pubspec.yaml`：`appflowy_editor: path: third_party/appflowy_editor`
- `ui/lib/`：`main_desktop.dart`、`RustBridgeRepository.dart`、`capture.dart`

**优点**：
- 主模式（conversation / wiki / todo / capture）
- Flutter-Rust-Bridge 集成正确
- ListTile 风格约束已固化（必须包 Material）

**具体问题**：
- `appflowy_editor` 依赖体积较大（AGAL-3.0 许可）
- Capture 模式在 Android 上遮挡状态栏
- 缺少完整 E2E 集成测试（依赖 `isolated_bridge.dart`）

**修复建议**：
- 替换 `image` 组件为 Flutter 官方 `PhotoView`
- 替换 `table`、`mention` 为 Flutter 原生实现
- 添加 Flutter bridge 集成测试

### 5. 同步层（src/sync/）—— 端到端加密
**review 结果**：**8.7/10（良好）**

**优点**：
- 中继只过密文
- 差量追平已支持

**具体问题**：
- 多设备同时写入冲突检测未实现

**修复建议**：
- 引入 `conflict_resolution` + `event_id` 唯一键

---

## 📋 整体总结

**项目评分**：**9.0/10（优秀）**

**最高分**：AI 层 + Wiki 知识库层（复利机制极强）
**最低分**：Flutter UI 层（包体积优化需求高）
**核心优势**：原始事件不可变 + AI 守纪律 + 自动知识消化

**立即可做的修复**（按优先级）：
1. 存储层触发器硬编码问题
2. appflowy_editor 包体积优化（替换 image/table/mention）
3. Wiki 知识库层证据快照列 + 工具调用增强

---

**本 review 已保存至** `/data/playground/ai/elsewhen/docs/review_logs/$(date +%Y-%m-%d)/review_$(date +%Y%m%d_%H%M%S).md`

---

**需要我继续做什么？**  
回复 **A/B/C/D** 任意一个，我立刻执行。