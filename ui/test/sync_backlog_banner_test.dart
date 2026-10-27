import 'package:elsewhen_ui/sync/sync_backlog_notice.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/widgets/sync_backlog_banner.dart';
import 'package:elsewhen_ui/widgets/sync_tick_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/sync_setup.dart';

SyncSetupController _controllerFor(TestSyncSetupApi setup) => SyncSetupController(
  keyStore: SyncKeyStore(storage: setup.storage),
  bridge: setup,
  previewEnabled: true,
);

void main() {
  group('判定（纯函数）', () {
    test('锁定 + 有积压 => 待解锁积压，且是可点的一类', () {
      final notice = syncBacklogNotice(
        testSyncState('locked', pendingWireEnvelopes: 12, pendingBytes: 58590),
      );
      expect(notice, isNotNull);
      expect(notice!.kind, SyncBacklogKind.lockedBacklog);
      expect(notice.actionable, isTrue);
      expect(notice.message, '待解锁积压 12 条 · 57 KB，解开本机密钥后自动同步');
    });

    test('就绪 + 有未回执 => 对端未确认，且不提供假按钮', () {
      final notice = syncBacklogNotice(
        testSyncState(
          'local_ready',
          pendingWireEnvelopes: 3,
          pendingBytes: 2048,
        ),
      );
      expect(notice, isNotNull);
      expect(notice!.kind, SyncBacklogKind.awaitingPeer);
      expect(
        notice.actionable,
        isFalse,
        reason: '等对端确认没有本机可做的动作，给按钮只会让人以为点了会变快',
      );
      // 精确断言整句：真机上曾渲染成「对端未确认 1 条 · 2.7 KB（1 条）」，
      // 条数被说了两遍 —— `contains` 抓不到这种重复。
      expect(notice.message, '同步延迟：对端未确认 3 条 · 2.0 KB，正在重试');
    });

    test('没有积压 / 未配置 / 需要修复 都返回 null', () {
      expect(syncBacklogNotice(null), isNull);
      expect(syncBacklogNotice(testSyncState('locked')), isNull);
      expect(
        syncBacklogNotice(
          testSyncState('not_configured', pendingWireEnvelopes: 5),
        ),
        isNull,
        reason: '没开同步就没有积压可言',
      );
      expect(
        syncBacklogNotice(
          testSyncState('requires_repair', pendingWireEnvelopes: 5),
        ),
        isNull,
        reason: '需要修复有自己的出口，不要用状态位抢它的位置',
      );
    });

    test('字节量级按人话显示', () {
      expect(formatBacklogBytes(512), '512 B');
      expect(formatBacklogBytes(2048), '2.0 KB');
      expect(formatBacklogBytes(153600), '150 KB');
      expect(formatBacklogBytes(5 * 1024 * 1024), '5.0 MB');
    });
  });

  group('主界面状态位', () {
    testWidgets('锁定时出现，点一下直达同步页', (tester) async {
      final setup = TestSyncSetupApi(TestSyncSecureStorage())
        ..current = testSyncState(
          'locked',
          pendingWireEnvelopes: 8,
          pendingBytes: 1024 * 1024,
        );
      final controller = _controllerFor(setup);
      await controller.refresh();
      var opened = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SyncBacklogBanner(
              controller: controller,
              onOpenSync: () => opened++,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.textContaining('待解锁积压'), findsOneWidget);
      expect(find.textContaining('去解锁'), findsOneWidget);
      await tester.tap(find.textContaining('待解锁积压'));
      await tester.pumpAndSettle();
      expect(opened, 1);
    });

    testWidgets('等对端确认时出现但没有假按钮', (tester) async {
      final setup = TestSyncSetupApi(TestSyncSecureStorage())
        ..current = testSyncState(
          'local_ready',
          pendingWireEnvelopes: 2,
          pendingBytes: 4096,
        );
      final controller = _controllerFor(setup);
      await controller.refresh();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SyncBacklogBanner(controller: controller, onOpenSync: () {}),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.textContaining('对端未确认 2 条'), findsOneWidget);
      expect(find.textContaining('去解锁'), findsNothing);
    });

    testWidgets('没有积压时整条不渲染（不能变成常驻噪音）', (tester) async {
      final setup = TestSyncSetupApi(TestSyncSecureStorage())
        ..current = testSyncState('local_ready');
      final controller = _controllerFor(setup);
      await controller.refresh();
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: SyncBacklogBanner(controller: controller, onOpenSync: () {}),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byType(Material), findsWidgets);
      expect(find.textContaining('积压'), findsNothing);
      expect(find.textContaining('对端未确认'), findsNothing);
    });
  });

  group('桌面同步页的积压行', () {
    testWidgets('有积压时显示条数与字节', (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: SyncBacklogLine(envelopes: 12, bytes: 58590),
          ),
        ),
      );
      expect(find.text('对端未确认 12 条 · 57 KB'), findsOneWidget);
    });

    testWidgets('零积压时不占位置', (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(body: SyncBacklogLine(envelopes: 0, bytes: 0)),
        ),
      );
      expect(find.textContaining('对端未确认'), findsNothing);
    });
  });
}
