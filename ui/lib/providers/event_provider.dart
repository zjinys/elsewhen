import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'state_holder.dart';
import '../models/event.dart';
import '../bridge/rust_bridge_repository.dart';

// Providers
final eventsProvider = FutureProvider<List<Event>>((ref) async {
  final repo = ref.watch(storageRepositoryProvider);
  return repo.listEvents();
});

final eventInputProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder(''),
);
