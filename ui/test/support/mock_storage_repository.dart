// 测试用的假实现，放在 test/support/ 而非 lib/：生产代码没有任何地方用得上
// 它，留在 lib/ 只是让「死代码」扫描和 review 都得多过滤一个目录。
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/data/storage_repository.dart';
import 'package:elsewhen_ui/models/analysis.dart';
import 'package:elsewhen_ui/models/event.dart';

/// Mock implementation of StorageRepository for testing
class MockStorageRepository implements StorageRepository {
  bool _initialized = false;
  int _idCounter = 0;
  final List<Event> _events = [];
  final List<Analysis> _analyses = [];

  @override
  Future<void> initialize() async {
    await Future.delayed(const Duration(milliseconds: 10));
    _initialized = true;
  }

  @override
  Future<Event> recordEvent(String rawText) async {
    if (!_initialized) await initialize();

    final event = Event(
      id: 'mock-${++_idCounter}',
      rawText: rawText,
      recordedAt: DateTime.now(),
      source: 'mock',
      status: 'pending',
    );

    _events.add(event);
    return event;
  }

  @override
  Future<void> recordUnifiedInput(
    String rawText, {
    String source = 'capture',
  }) async {
    await recordEvent(rawText);
  }

  @override
  Future<List<Event>> listEvents() async {
    if (!_initialized) await initialize();
    return List.from(_events);
  }

  @override
  Future<List<Analysis>> listAnalyses() async {
    if (!_initialized) await initialize();
    return List.from(_analyses);
  }

  @override
  Future<String?> getAiProvider() async {
    if (!_initialized) await initialize();
    return 'mock-provider';
  }

  @override
  Future<api.AnalysisTriggerResult> triggerAnalysis() async {
    if (!_initialized) await initialize();
    return const api.AnalysisTriggerResult.noProvider();
  }

  // Test helpers
  void addMockAnalysis(Analysis analysis) {
    _analyses.add(analysis);
  }

  void clear() {
    _events.clear();
    _analyses.clear();
  }
}
