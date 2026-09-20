# Elsewhen Session 交接 — 2026-09-20

## 目标与用户约定

继续实现个人认知主循环。用户已认可方向，要求每完成一个独立步骤，立即更新 roadmap 的完成状态与验证记录。当前从 Phase 3 数据层继续，不需要重新讨论产品方向。

唯一实施路线图：[个人认知主循环 Roadmap](../roadmap/2026-09-17-personal-cognition-main-loop-roadmap.md)。
原始想法见根目录 thoughts.md；项目约束见根目录 CLAUDE.md。

### 必须长期遵守的交付规则

- 每个功能必须以用户能在当前应用界面完成的纵向闭环交付：输入/触发 → 用户确认（如需要）→ 数据落库 → 界面立即可见 → 来源可核验。
- 只有 schema、API、Bridge、只读展示或链路末端美化，不能单独宣称“功能完成”；如果上游尚不会稳定产生数据，就不应继续打磨空展示。
- 每完成一个步骤，必须明确告诉用户：界面哪里变化、从哪里进入、具体操作步骤、预期结果、仍未实现的边界。
- 用户实际验证后再扩展下一步；发现用户界面没有实质变化时，立即停止外围优化并补齐核心生成/确认链路。
- 内部数据概念不应直接变成用户导航或心智。例如 `topic` 是内部存储类型，不新增“议题库”、独立 Tab 或专门创建入口。
- AI 派生的人物、项目、关系和长期事实必须经过确认；确认后的事实必须绑定真实来源事件，事件 ID 由系统上下文注入，不能让模型提供或伪造。
- 测试必须使用隔离临时数据库，不得读取或修改用户真实数据库。

## 当前基线

交接前位于 main，工作树干净。当前提交：
- `1137ef0 feat: 统一输入链路 + 派生页视图 + 测试隔离桥接`

当前工作区包含 Phase 3 每日概览与每日回顾数据契约实现；UI 信息架构仍冻结，不上线空壳 Today 页面。

## 已完成

- Phase 0：Bridge 测试隔离临时数据库、分析队列五状态可观测、设置页展示队列、开发文档同步。
- Phase 1：input_records（migration v19）、普通输入及对话输入原子创建关联事件/消息/分析任务、Capture 与统一日流、URL 预览及来源关联、稳定提交幂等 key、业务写入保留确认门禁。
- Phase 2：src/api.rs 的 trigger_analysis 消费持久化队列。无 Provider 返回 no_provider；成功返回 processed:<成功数量>；合法 JSON 对象写入 event_analyses，并更新事件及任务状态；Provider 故障或非法 JSON 进入 retry / failed。
- 每次触发最多尝试 min(pending + retry, 50) 次，避免持续新增任务造成单次调用无限运行。
- 离线 StubProvider 测试覆盖多任务成功、不重复处理、非法 JSON/非对象 JSON及 Provider 失败。
- Flutter Bridge 已实现进程内串行 worker：启动及保存后非阻塞唤醒，合并重复信号，pending 超过 50 项自动续批，5 秒周期检查 retry 到期；测试 teardown 会取消 timer。
- init_bridge 会将上次进程遗留的 running 任务恢复为立即可执行的 retry；恢复只发生在启动边界，避免每次 Store::open 抢占活跃任务。
- `event-analysis-v1` 已定义并严格校验八字段 schema；结果 JSON 与 `prompt_version` 同步记录版本，数组字段规范化去重。
- 决策记录：
  - [统一输入与幂等边界](implemented/architecture/2026-09-17-unified-input-routing-and-idempotency.md)
  - [分析队列触发边界](implemented/architecture/2026-09-18-analysis-queue-trigger.md)

## 下一步（建议顺序）

1. 完成日期切换的数据边界：按本地日期读取事实、回顾和待办，保持无 AI 降级；随后再设计 Today 的完整使用场景。

## 关键文件

- src/api.rs：分析队列、统一输入、`get_daily_overview`、`save_daily_review`、`generate_daily_review`。
- src/storage.rs：analysis_jobs、daily_entries、daily_reviews、daily_review_sources、待办和输入事务。
- src/ai/provider.rs：同步 Provider 接口，网络调用有超时。
- ui/lib/bridge/rust_bridge_repository.dart：分析 worker、输入提交、`getDailyOverview` 和 `generateDailyReview`。
- ui/lib/providers/conversation_provider.dart、ui/lib/screens/capture_screen.dart：保存路径。
- ui/test/bridge_integration_test.dart：隔离数据库、统一输入、日流和 `getDailyOverview` 回归。
- ui/test/support/isolated_bridge.dart：真实 Bridge 测试隔离入口。
- regen.sh：生成绑定并构建 release 动态库。

## 本 session 验证

- `cargo test --all-targets`：通过。
- Flutter 相关 UI 测试 6 项通过。
- `fvm flutter analyze`：无 error，仅保留既有 info/style 提示。
- 修复 `left_sidebar.dart` 窄宽度标签 RenderFlex overflow；当前 UI 使用已声明的 SVG logo 资源，不再引用不存在的 `elsewhen-icon-v2-256.png`。
- Phase 3 数据层新增 `get_daily_overview`：事实、最新可追溯回顾和当天相关待办统一返回；Bridge 已重新生成并同步 release 动态库。
- 隔离 Flutter Bridge 已验证 `getDailyOverview`：无 AI 回顾时返回事实和空 review，未关联待办不会凭空出现。
- 新增 `save_daily_review`：写入前校验版本、日期、来源清单与逐条引用，生成器后续可安全追加回顾版本；尚未接入页面写入。
- 新增 `generate_daily_review`：显式触发、Provider 可用且当天有事实时才生成；Bridge repository 已接入，暂不自动调用。
- Phase 2.5 已开始：system prompt / `record_event` 工具描述加入可记录性边界；`event-analysis-v2` 兼容读取 v1，并增加 `recordable`、`kind`，详情 DTO 已同步。
- 日流 API、`get_daily_overview` 和每日回顾生成均已按最新分析结果过滤 `recordable=false`，原始事件仍保留。
- Phase 4A 已新增 `entity_facts`（migration v21）、幂等 upsert / 置信度提升和 `list_entity_facts` Bridge API；尚未自动从分析候选写入，保持确认边界。

## 验证事实与未完成验证

以下为前一实现阶段的实际执行记录，本次文档交接未重跑：
- cargo test：109 个 lib 测试及其余目标通过。
- 新增 analysis_tests：2 项通过。
- git diff --check：通过。
- 当前 Flutter 全量测试为 39 项通过；`flutter analyze` 无 error/warning，仅保留 32 条既有 info。
- `./regen.sh` 已成功，生成绑定与 release 动态库 hash 同步。
- `cargo test --all-targets`：lib 110 + bin 97 通过；schema 定向测试 3 项通过；Flutter 全量 39 项通过；`flutter analyze` 无 error/warning，保留 32 条既有 info。
- 最近真实数据库 SHA-256：23ae80c302b8c344cecbfaa79e1f664dd80637f5c12b4c986430b8540cf5e29c。此前 events/wiki_pages/conversations 计数为 18/19/45；新 session 不应假设用户数据此后未变化，应自行记录测试前后基线。
- 本次文档交接：构建/测试 skipped（仅文档变更）；执行 git diff --check。

## 风险与约束

- `event-analysis-v1` 已校验必填字段、字段类型、未知字段和 confidence 范围；纠正/忽略入口及候选实体确认仍待实现。
- 当前 processed:0 不区分“无可执行任务”和“本次全部失败”，需结合队列状态判断。
- 当前退避很短，较慢批次里早先失败的任务可能再次到期；处理上限限制的是尝试次数，不是唯一事件数。
- 保存原始事件不依赖 AI；不修改原始事件来纠正分析。
- 不允许测试访问个人默认数据库；不要清理个人库中的旧测试事件。
- 保留用户改动；只用 apply_patch 编辑文件。避免全仓 cargo fmt 引入无关格式变化。
- handoff 技能所引用的 docs/requirements/02_constraints.md、999_acceptance.md、ai_handoff_template.md 和 scripts/sync_handoff_to_obsidian_daily.sh 在本仓库不存在；未执行 Obsidian 同步。

## 新 Session 可直接使用的提示

请读取 docs/notes/ai_handoff.md 和当前 roadmap，继续 Phase 3 日期切换与每日回顾数据层。不要先上线空壳 Today 页面；每完成一个独立步骤立即更新 roadmap，测试必须隔离个人数据。
