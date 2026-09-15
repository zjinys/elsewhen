import 'storage_repository.dart';
import '../models/event.dart';
import '../models/analysis.dart';

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
  Future<String> triggerAnalysis() async {
    if (!_initialized) await initialize();
    return 'success';
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
