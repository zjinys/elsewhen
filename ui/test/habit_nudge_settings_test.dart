import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/habit_nudge_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('习惯提醒集中列表显示运行中、已暂停与未设置', (tester) async {
    final entries = [
      HabitReminderEntry(
        page: _page('habit/walk', '每天散步'),
        plan: _plan(
          id: 'nudge-walk',
          slug: 'habit/walk',
          question: '今天散步了吗？',
          weekdays: 127,
          localTime: '20:00',
          doneLabel: '走了',
          missedLabel: '没走',
          enabled: true,
        ),
      ),
      HabitReminderEntry(
        page: _page('habit/read', '每天阅读'),
        plan: _plan(
          id: 'nudge-read',
          slug: 'habit/read',
          question: '今天阅读了吗？',
          weekdays: 31,
          localTime: '21:30',
          doneLabel: '读了',
          missedLabel: '没读',
          enabled: false,
        ),
      ),
      HabitReminderEntry(page: _page('habit/water', '每天喝水'), plan: null),
    ];

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          habitReminderEntriesProvider.overrideWith((ref) async => entries),
        ],
        child: MaterialApp(
          theme: AppTheme.darkTheme,
          home: const Scaffold(
            body: SingleChildScrollView(
              padding: EdgeInsets.all(24),
              child: HabitNudgeSettingsContent(),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('运行中 1 · 已暂停 1 · 未设置 1'), findsOneWidget);
    expect(find.text('每天散步'), findsOneWidget);
    expect(find.text('每天 20:00 · 走了/没走'), findsOneWidget);
    expect(find.text('运行中'), findsOneWidget);

    expect(find.text('每天阅读'), findsOneWidget);
    expect(find.text('每周 一、二、三、四、五 21:30 · 读了/没读'), findsOneWidget);
    expect(find.text('已暂停'), findsOneWidget);

    expect(find.text('每天喝水'), findsOneWidget);
    expect(find.text('尚未设置定期问候'), findsOneWidget);
    expect(find.text('未设置'), findsOneWidget);
    expect(find.text('设置'), findsOneWidget);
    expect(find.text('调整'), findsNWidgets(2));
  });
}

WikiPageMeta _page(String slug, String title) {
  final now = DateTime(2026, 10, 22, 8);
  return WikiPageMeta(
    id: slug,
    slug: slug,
    kind: 'habit',
    title: title,
    summary: '',
    tags: const [],
    evidenceCount: 0,
    firstSeenAt: now,
    lastSeenAt: now,
    status: 'active',
    createdAt: now,
    updatedAt: now,
    area: 'insight',
  );
}

api.HabitNudgeDto _plan({
  required String id,
  required String slug,
  required String question,
  required int weekdays,
  required String localTime,
  required String doneLabel,
  required String missedLabel,
  required bool enabled,
}) {
  return api.HabitNudgeDto(
    id: id,
    pageSlug: slug,
    question: question,
    doneLabel: doneLabel,
    doneText: '今天$doneLabel',
    missedLabel: missedLabel,
    missedText: '今天$missedLabel',
    weekdays: weekdays,
    localTime: localTime,
    enabled: enabled,
    createdAt: '2026-10-22T00:00:00Z',
    updatedAt: '2026-10-22T00:00:00Z',
  );
}
