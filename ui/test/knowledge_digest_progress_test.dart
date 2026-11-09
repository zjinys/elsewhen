import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/widgets/knowledge_digest_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class Queue extends RustBridgeRepository {
  Queue() : super(runBackgroundWorker: false);
  String phase = 'reading';
  String? error;
  int calls = 0;
  @override
  Future<List<api.KnowledgeDigestJobDto>> listKnowledgeDigestJobs({
    String? status,
    int limit = 200,
  }) async {
    calls++;
    return [
      api.KnowledgeDigestJobDto(
        jobId: 'job',
        eventId: 'event',
        eventExcerpt: 'Original 🙂',
        recordedAt: '2026-11-09T00:00:00Z',
        status: 'pending',
        attempts: 0,
        failedRounds: 0,
        availableAt: '2026-11-09T00:00:00Z',
        updatedAt: '2026-11-09T00:00:00Z',
        reading: api.KnowledgeDigestReadingDto(
          phase: phase,
          readChars: phase == 'ready' ? 100 : 40,
          totalChars: 100,
          completedLeaves: 2,
          completedMerges: 1,
          error: error,
          retryAt: '2026-11-09T01:00:00Z',
        ),
      ),
    ];
  }

  @override
  Future<List<api.KnowledgeDigestRunDto>> listKnowledgeDigestRuns({
    int limit = 100,
  }) async => [];
}

void main() {
  testWidgets(
    'progress localizes live, refreshes, and waits for final processing',
    (tester) async {
      final repo = Queue();
      Widget host(Locale locale) => ProviderScope(
        overrides: [storageRepositoryProvider.overrideWithValue(repo)],
        child: MaterialApp(
          locale: locale,
          supportedLocales: AppLocalizations.supportedLocales,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const Scaffold(body: KnowledgeDigestDialog()),
        ),
      );
      await tester.pumpWidget(host(const Locale('en')));
      await tester.pumpAndSettle();
      final en = lookupAppLocalizations(const Locale('en'));
      expect(find.text(en.digestReadingPhase), findsOneWidget);
      expect(
        find.text('40 / 100 characters read · segments: 2 · merges: 1'),
        findsOneWidget,
      );
      final calls = repo.calls;
      await tester.pumpWidget(host(const Locale('zh')));
      await tester.pumpAndSettle();
      final zh = lookupAppLocalizations(const Locale('zh'));
      expect(find.text(zh.digestReadingPhase), findsOneWidget);
      expect(repo.calls, calls);
      repo.phase = 'ready';
      repo.error = 'event_reading_merge_input_exceeds_budget';
      await tester.pump(const Duration(seconds: 5));
      await tester.pumpAndSettle();
      expect(find.text(zh.digestReadyPhase), findsOneWidget);
      expect(find.text(zh.digestReadingBudgetHelp), findsOneWidget);
      expect(find.text(zh.completed), findsNothing);
      expect(find.byType(ElevatedButton), findsNothing);
      expect(find.text('Original 🙂'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
