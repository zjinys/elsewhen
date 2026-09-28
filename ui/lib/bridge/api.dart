/// 桥接 API 汇总入口。
///
/// flutter_rust_bridge 按 Rust 子模块（`src/api/*.rs`）拆分生成 Dart 文件；
/// 应用与测试代码统一 `import '.../bridge/api.dart' as api;`，不直接引用
/// `generated.dart/` 下的单个文件。新增 `src/api` 子模块并 `./regen.sh` 后，
/// 在此补一行 export。
library;

export 'generated.dart/api.dart';
export 'generated.dart/api/conversations.dart';
export 'generated.dart/api/entities.dart';
export 'generated.dart/api/fonts.dart';
export 'generated.dart/api/import.dart';
export 'generated.dart/api/knowledge_digest.dart';
export 'generated.dart/api/provider_config.dart';
export 'generated.dart/api/relations.dart';
export 'generated.dart/api/rules.dart';
export 'generated.dart/api/theme.dart';
export 'generated.dart/api/todos.dart';
export 'generated.dart/api/tweet.dart';
export 'generated.dart/api/wiki_chat.dart';
export 'generated.dart/api/wiki.dart';
