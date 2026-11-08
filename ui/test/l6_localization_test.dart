import 'dart:convert';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/l10n/notice_messages.dart';
import 'package:elsewhen_ui/l10n/processing_result.dart';
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/sync/sync_backlog_notice.dart';
import 'package:elsewhen_ui/sync/sync_provider_conflicts_controller.dart';
import 'package:elsewhen_ui/widgets/app_error_notice.dart';
import 'package:elsewhen_ui/widgets/knowledge_maintenance_sections.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';
import 'package:elsewhen_ui/widgets/sync_provider_conflicts_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'knowledge_maintenance_sections_test.dart' show MaintenanceFake;
import 'sync_owner_pairing_test.dart' show Fixture;
import 'sync_provider_conflicts_test.dart' show TestConflictApi;

void main() {
  final en = lookupAppLocalizations(const Locale('en'));
  final zh = lookupAppLocalizations(const Locale('zh'));
  Future<void> host(
    WidgetTester tester,
    Locale locale,
    Widget child, {
    MaintenanceFake? repository,
  }) async {
    await tester.binding.setSurfaceSize(const Size(380, 1100));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          if (repository != null)
            knowledgeRepositoryProvider.overrideWithValue(repository),
        ],
        child: MaterialApp(
          locale: locale,
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(body: SingleChildScrollView(child: child)),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  test('nested controller messages resolve in the current locale; malformed receipts fail closed', () {
    final token = appNotice('l6TheSyncStepDidNotRespond28e88', [
      appNotice('l6Arg0Secondse612a', ['12']),
    ]);
    expect(resolveNotice(token, en), contains('12'));
    expect(resolveNotice(token, en), contains('seconds'));
    expect(resolveNotice(token, zh), contains('12'));
    expect(resolveNotice(token, zh), contains('秒'));
    for (final invalid in [
      'elsewhen-notice-v1:bad',
      appNotice('unknown'),
      appNotice('connectionSucceeded', ['extra']),
    ]) {
      expect(resolveNotice(invalid, en), isNull);
    }
    const backlog = SyncBacklogNotice(
      kind: SyncBacklogKind.lockedBacklog,
      envelopes: 2,
      bytes: 1024,
    );
    expect(backlog.localizedMessage(en), contains('2 queued'));
    expect(backlog.localizedMessage(zh), contains('2'));
    expect(backlog.actionable, isTrue);
  });

  test('processing receipts preserve human protection, counts and original diagnostics', () {
    String receipt(String code, {int? count}) =>
        'elsewhen-result-v1:${jsonEncode({'version': 1, 'code': code, 'count': count, 'detail': '完整处理原文\nsecond line'})}';
    final single = ProcessingResult.decode(
      receipt('page_reprocessed', count: 1),
    )!;
    expect(single.message(en), contains('1 source event'));
    expect(single.message(zh), contains('1'));
    expect(single.detail, '完整处理原文\nsecond line');
    expect(
      ProcessingResult.decode(receipt('page_reprocessed', count: 2))!
          .message(en),
      contains('2 source events'),
    );
    final protected = ProcessingResult.decode(receipt('page_human_protected'))!;
    expect(protected.message(en), en.pageHumanProtected);
    expect(protected.message(zh), zh.pageHumanProtected);
    expect(
      ProcessingResult.decode(receipt('page_reprocessed', count: -1))!
          .message(en),
      isNull,
    );
    expect(ProcessingResult.decode(receipt('unknown'))!.message(en), isNull);
    expect(
      ProcessingResult.decode(
        'elsewhen-result-v1:{"version":1,"code":"page_reprocessed","detail":{},"count":1}',
      ),
      isNull,
    );
    expect(ProcessingResult.decode('旧格式的处理说明'), isNull);
  });

  testWidgets(
    'pending errors change language and full raw details remain reachable',
    (tester) async {
      final error = appFailure(
        '原始诊断\nprivate/path',
        key: 'taskMaterialChanged',
      );
      final child = AppErrorMessage(action: 'load', error: error);
      await host(tester, const Locale('zh'), child);
      expect(find.text(zh.taskMaterialChanged), findsOneWidget);
      await host(tester, const Locale('en'), child);
      expect(find.text(en.taskMaterialChanged), findsOneWidget);
      expect(find.textContaining('原始诊断'), findsNothing);
      await tester.tap(find.text(en.errorDetails));
      await tester.pumpAndSettle();
      expect(find.text('原始诊断\nprivate/path'), findsOneWidget);
      expect(find.text(en.copy), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'queue switches language without losing tasks or source details at phone width',
    (tester) async {
      final repository = MaintenanceFake();
      final section = KnowledgeWorkQueueSection(onOpenPage: (_) async {});
      await host(tester, const Locale('zh'), section, repository: repository);
      await tester.tap(find.text(zh.l6ProcessingQueueb0388));
      await tester.pumpAndSettle();
      expect(find.text('排队原料 0'), findsOneWidget);
      final calls = repository.offsets.length;
      await host(tester, const Locale('en'), section, repository: repository);
      expect(find.text(en.l6ProcessingQueueb0388), findsOneWidget);
      expect(find.text('排队原料 0'), findsOneWidget);
      expect(find.text(en.queueDetailWaiting), findsOneWidget);
      expect(repository.offsets.length, calls);
      expect(find.text('等待全文阅读'), findsNothing);
      await tester.ensureVisible(find.text(en.processingNotes));
      await tester.tap(find.text(en.processingNotes));
      await tester.pumpAndSettle();
      expect(find.text('等待全文阅读'), findsOneWidget);
      expect(repository.repaired, isNull);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );

  testWidgets(
    'English owner pairing preserves explicit approval and fingerprint at phone width',
    (tester) async {
      final f = Fixture();
      await f.initialize();
      await f.invite();
      await host(
        tester,
        const Locale('en'),
        Padding(
          padding: const EdgeInsets.all(16),
          child: LocalSyncSettings(controller: f.controller),
        ),
      );
      expect(tester.takeException(), isNull, reason: 'initial manage layout');
      await tester.tap(find.text(en.l6Pairing6aed7));
      await tester.pump(const Duration(seconds: 2));
      expect(tester.takeException(), isNull, reason: 'pairing layout');
      await tester.ensureVisible(find.text(en.l6VerifyAndApprove5c161));
      await tester.tap(find.text(en.l6VerifyAndApprove5c161));
      await tester.pump(const Duration(seconds: 2));
      expect(tester.takeException(), isNull, reason: 'approval dialog layout');
      final approve = find.widgetWithText(
        FilledButton,
        en.l6ApproveThisDeviced11af,
      );
      expect(tester.widget<FilledButton>(approve).onPressed, isNull);
      expect(find.textContaining('加入的设备'), findsWidgets);
      expect(find.textContaining(f.pairing.preview.fingerprint), findsWidgets);
      expect(f.pairing.approved, 0);
      await tester.tap(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.byType(CheckboxListTile),
        ),
      );
      await tester.pump();
      await tester.tap(approve);
      await tester.pump(const Duration(seconds: 2));
      expect(f.pairing.approved, 1);
      expect(f.pairing.approvedToken, 'displayed-token');
      expect(f.pairing.sent, 1);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      await f.close();
    },
  );

  testWidgets(
    'English provider conflict keeps user names, masked keys and versioned review',
    (tester) async {
      api.ProviderConflictProfileDto profile(String id, String name) =>
          api.ProviderConflictProfileDto(
            id: id,
            name: name,
            providerType: 'openai-compatible',
            baseUrl: 'https://example.invalid/v1',
            model: 'model-original',
            apiKeyMasked: 'SYN****MASK',
            apiKeyPresent: true,
            enabled: true,
            isActive: true,
            temperature: 0.7,
            reachabilityHints: const [],
          );
      final bridge = TestConflictApi()
        ..current = [
          api.ProviderSyncConflictDto(
            revisionId: 'conflict-original',
            senderDeviceId: 'device',
            senderDeviceName: '原始设备名称',
            localRevision: 'local-original',
            remoteProfiles: [profile('remote', '远端用户名称')],
            localProfiles: [profile('local', '本地用户名称')],
            remoteActiveId: 'remote',
            localActiveId: 'local',
            remoteFailoverOrder: const ['remote'],
          ),
        ];
      final controller = SyncProviderConflictsController(bridge: bridge);
      await host(
        tester,
        const Locale('en'),
        Padding(
          padding: const EdgeInsets.all(16),
          child: SyncProviderConflictsSettings(controller: controller),
        ),
      );
      expect(find.textContaining('远端用户名称'), findsWidgets);
      expect(find.textContaining('本地用户名称'), findsWidgets);
      expect(find.textContaining('SYN****MASK'), findsWidgets);
      expect(bridge.resolutions, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      controller.dispose();
    },
  );
}
