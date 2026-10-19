import 'dart:io';

import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';

// 仓储便捷方法声明于 bridge 库的 part extension 中；扩展方法不随类型传递，
// 测试文件只 import 本 helper 即可使用，故在此转发导出。
export 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:flutter_test/flutter_test.dart';

/// Creates a real Rust bridge backed by a fresh per-suite temporary database.
///
/// Real bridge tests must use this helper instead of the platform's normal
/// Elsewhen data directory. The directory is removed after the suite finishes.
///
/// 后台 worker 被关掉（`runBackgroundWorker: false`）：它每隔 5s 排空分析队列并做
/// 知识消化，这些是**写**操作，会插进测试自己的 FFI 调用之间抢 SQLite 锁，随事件循环
/// 交错时序随机产出 `SQLITE_BUSY`（表现为「每次失败的用例都不同」）。需要推进队列的
/// 用例显式调 `triggerAnalysis()`，语义更确定。
Future<RustBridgeRepository> createIsolatedBridge({
  bool runChatQueueWorker = false,
  SyncSetupController? syncSetupController,
  RustBridgeRepository Function(String dataDir)? repositoryFactory,
}) async {
  final dataDir = await Directory.systemTemp.createTemp(
    'elsewhen-flutter-test-',
  );
  final repo =
      repositoryFactory?.call(dataDir.path) ??
      RustBridgeRepository(
        databasePath: dataDir.path,
        runBackgroundWorker: false,
        runChatQueueWorker: runChatQueueWorker,
        syncSetupController: syncSetupController,
      );
  if (repo.databasePath != dataDir.path) {
    throw StateError('Bridge tests must use the generated isolated directory');
  }
  await repo.initialize();
  addTearDown(() async {
    await repo.localSync.close();
    repo.dispose();
    if (await dataDir.exists()) {
      await dataDir.delete(recursive: true);
    }
  });
  return repo;
}
