import 'dart:io';

import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:flutter_test/flutter_test.dart';

/// Creates a real Rust bridge backed by a fresh per-suite temporary database.
///
/// Real bridge tests must use this helper instead of the platform's normal
/// Elsewhen data directory. The directory is removed after the suite finishes.
Future<RustBridgeRepository> createIsolatedBridge() async {
  final dataDir = await Directory.systemTemp.createTemp(
    'elsewhen-flutter-test-',
  );
  final repo = RustBridgeRepository(databasePath: dataDir.path);
  await repo.initialize();
  addTearDown(() async {
    repo.dispose();
    if (await dataDir.exists()) {
      await dataDir.delete(recursive: true);
    }
  });
  return repo;
}
