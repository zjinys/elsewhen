import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/generated.dart/api.dart' as api;
import 'package:elsewhen_ui/data/storage_repository.dart';
import 'package:elsewhen_ui/models/analysis.dart';

import 'support/mock_storage_repository.dart';

void main() {
  group('MockStorageRepository', () {
    late MockStorageRepository repo;

    setUp(() {
      repo = MockStorageRepository();
    });

    tearDown(() {
      repo.clear();
    });

    test('initializes successfully', () async {
      await repo.initialize();
      final provider = await repo.getAiProvider();
      expect(provider, 'mock-provider');
    });

    test('records and lists events', () async {
      await repo.initialize();

      final event1 = await repo.recordEvent('First test event');
      expect(event1.rawText, 'First test event');
      expect(event1.source, 'mock');
      expect(event1.status, 'pending');

      final event2 = await repo.recordEvent('Second test event');
      expect(event2.id, isNot(event1.id));

      final events = await repo.listEvents();
      expect(events.length, 2);
      expect(events[0].rawText, 'First test event');
      expect(events[1].rawText, 'Second test event');
    });

    test('trigger analysis returns structured no-provider result', () async {
      await repo.initialize();
      final result = await repo.triggerAnalysis();
      expect(result, const api.AnalysisTriggerResult.noProvider());
    });

    test('lists analyses', () async {
      await repo.initialize();

      repo.addMockAnalysis(
        Analysis(
          eventType: 'meeting',
          confidence: 0.95,
          summary: 'Team meeting scheduled',
          clarifications: [],
        ),
      );

      repo.addMockAnalysis(
        Analysis(
          eventType: 'task',
          confidence: 0.87,
          summary: 'Code review needed',
          clarifications: ['Which PR?'],
        ),
      );

      final analyses = await repo.listAnalyses();
      expect(analyses.length, 2);
      expect(analyses[0].eventType, 'meeting');
      expect(analyses[0].confidence, 0.95);
      expect(analyses[1].clarifications.length, 1);
    });

    test('clear removes all data', () async {
      await repo.initialize();
      await repo.recordEvent('Test event');
      repo.addMockAnalysis(
        Analysis(
          eventType: 'note',
          confidence: 0.9,
          summary: 'Quick note',
          clarifications: [],
        ),
      );

      repo.clear();

      final events = await repo.listEvents();
      final analyses = await repo.listAnalyses();
      expect(events.length, 0);
      expect(analyses.length, 0);
    });

    test('implements StorageRepository interface', () {
      expect(repo, isA<StorageRepository>());
    });

    test('auto-initializes on first operation', () async {
      // Don't call initialize explicitly
      final event = await repo.recordEvent('Auto-init test');
      expect(event.rawText, 'Auto-init test');

      final provider = await repo.getAiProvider();
      expect(provider, isNotNull);
    });
  });
}
