import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/todo.dart';
import '../bridge/rust_bridge_repository.dart';

/// 待办清单（未归档，含进行中 + 已完成）
final todosProvider = FutureProvider<List<Todo>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listTodos();
});
