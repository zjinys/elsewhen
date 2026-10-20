/// 桥接 API 汇总入口。
///
/// flutter_rust_bridge 按 Rust 子模块（`src/api/*.rs`）拆分生成 Dart 文件；
/// 应用与测试代码统一 `import '.../bridge/api.dart' as api;`，不直接引用
/// `generated.dart/` 下的单个文件。新增 `src/api` 子模块并 `scripts/regen.sh` 后，
/// 在此补一行 export。
///
/// ⚠️ frb 2.14.0-beta.2 布局备注：barrel `generated.dart/api.dart` 已不再
/// 生成（codegen 会跳过重写该文件，改动方法名后形成新旧不同步——曾因此
/// 冒出上百个 undefined_method）。现在入口一律经本文件显式 export 下列
/// 生成库；重跑 regen 后若 analyze 冒出 undefined，先对照
/// `generated.dart/` 下的文件树补漏。
library;

export 'generated.dart/api/analysis_schema.dart';
export 'generated.dart/api/chat_proposals.dart';
export 'generated.dart/api/chat_runs.dart';
export 'generated.dart/api/companion_flow.dart';
export 'generated.dart/api/conversations.dart';
export 'generated.dart/api/db_maintenance.dart';
export 'generated.dart/api/entities.dart';
export 'generated.dart/api/event_stats.dart';
export 'generated.dart/api/fonts.dart';
export 'generated.dart/api/goals.dart';
export 'generated.dart/api/import.dart';
export 'generated.dart/api/knowledge.dart';
export 'generated.dart/api/knowledge_digest.dart';
export 'generated.dart/api/knowledge_workflows.dart';
export 'generated.dart/api/mcp.dart';
export 'generated.dart/api/page_marks.dart';
export 'generated.dart/api/period_stats.dart';
export 'generated.dart/api/provider_config.dart';
export 'generated.dart/api/provider_reachability.dart';
export 'generated.dart/api/records.dart';
export 'generated.dart/api/relations.dart';
export 'generated.dart/api/review_flow.dart';
export 'generated.dart/api/review_report/detect.dart';
export 'generated.dart/api/review_report/store.dart';
export 'generated.dart/api/rules.dart';
export 'generated.dart/api/sync/conflicts.dart';
export 'generated.dart/api/sync/datasets.dart';
export 'generated.dart/api/sync/handoff.dart';
export 'generated.dart/api/sync/invite.dart';
export 'generated.dart/api/sync/join.dart';
export 'generated.dart/api/theme.dart';
export 'generated.dart/api/todos.dart';
export 'generated.dart/api/tweet.dart';
export 'generated.dart/api/user_skills.dart';
export 'generated.dart/api/web_search.dart';
export 'generated.dart/api/wiki.dart';
export 'generated.dart/api/wiki_chat.dart';
export 'generated.dart/knowledge/health.dart';
export 'generated.dart/knowledge/maintenance.dart';
export 'generated.dart/knowledge/organization.dart';
export 'generated.dart/knowledge/queue.dart';
export 'generated.dart/knowledge/review.dart';
export 'generated.dart/knowledge/select.dart';
export 'generated.dart/knowledge/workflows.dart';
export 'generated.dart/storage/knowledge/core.dart';

/// Saving, queueing and steering are distinct, explicit user choices.
enum ChatInputMode { generate, recordOnly, supplement }
