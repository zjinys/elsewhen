import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/intl.dart';

/// 习惯问候卡只做展示与本地状态判断；这里直接覆盖 provider，不碰 FFI。
void main() {
  final today = DateFormat('yyyy-MM-dd').format(DateTime.now());
  final yesterday = DateFormat('yyyy-MM-dd')
      .format(DateTime.now().subtract(const Duration(days: 1)));

  testWidgets('今日问候显示按习惯定制的按钮', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(ref: 'nudge-1:$today'),
        plan: _walkPlan(),
        states: [
          api.HabitNudgeStateDto(
            nudgeId: 'nudge-1',
            localDate: today,
            response: null,
            snoozeUntil: null,
            askCount: 1,
            messageId: 'm1',
          ),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('今天午饭后散步了吗？'), findsOneWidget);
    expect(find.text('走了'), findsOneWidget);
    expect(find.text('没走'), findsOneWidget);
    expect(find.text('晚点再问'), findsOneWidget);
    expect(find.text('今天不问'), findsOneWidget);
  });

  testWidgets('没有本机 fire 行时不显示「晚点再问」', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(ref: 'nudge-1:$today'),
        plan: _walkPlan(),
        states: const [],
      ),
    );
    await tester.pumpAndSettle();

    // 非主设备只同步到消息，没有本机 fire；回答仍可直接同步。
    expect(find.text('走了'), findsOneWidget);
    expect(find.text('晚点再问'), findsNothing);
  });

  testWidgets('昨天的问候只作为历史，不提供补点按钮', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(ref: 'nudge-1:$yesterday'),
        plan: _walkPlan(),
        states: const [],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('这条问候已过期'), findsOneWidget);
    expect(find.text('走了'), findsNothing);
    expect(find.text('没走'), findsNothing);
  });

  testWidgets('已回答的问候显示记录句，不再显示按钮', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(id: 'm1', ref: 'nudge-1:$today'),
        plan: _walkPlan(),
        states: [
          api.HabitNudgeStateDto(
            nudgeId: 'nudge-1',
            localDate: today,
            response: 'done',
            snoozeUntil: null,
            askCount: 1,
            messageId: 'm1',
          ),
        ],
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('已记录：今天午饭后散步了'), findsOneWidget);
    expect(find.text('走了'), findsNothing);
  });

  testWidgets('同步到补问消息后，首条问候不再显示按钮', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(ref: 'nudge-1:$today'),
        plan: _walkPlan(),
        states: const [],
        supersededByNewer: true,
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('已再次提醒'), findsOneWidget);
    expect(find.text('走了'), findsNothing);
    expect(find.text('没走'), findsNothing);
  });

  testWidgets('另一台设备同步来的回答也会隐藏按钮', (tester) async {
    await tester.pumpWidget(
      _wrap(
        message: _nudge(id: 'm1', ref: 'nudge-1:$today'),
        plan: _walkPlan(),
        states: const [],
        checkin: Message(
          id: 'checkin-1',
          conversationId: 'conv-1',
          role: MessageRole.user,
          content: '今天午饭后散步了',
          createdAt: DateTime.now(),
          kind: 'habit_checkin',
          messageRef: 'nudge-1:$today',
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('已记录：今天午饭后散步了'), findsOneWidget);
    expect(find.text('走了'), findsNothing);
    expect(find.text('没走'), findsNothing);
  });
}

Widget _wrap({
  required Message message,
  required api.HabitNudgeDto plan,
  required List<api.HabitNudgeStateDto> states,
  Message? checkin,
  bool supersededByNewer = false,
}) {
  return ProviderScope(
    overrides: [
      habitNudgePlanProvider.overrideWith((ref, id) async => plan),
      habitNudgeStatesProvider.overrideWith((ref) async => states),
    ],
    child: MaterialApp(
      home: Scaffold(
        body: HabitNudgeBubble(
          message: message,
          checkin: checkin,
          supersededByNewer: supersededByNewer,
        ),
      ),
    ),
  );
}

Message _nudge({String id = 'm1', required String ref}) => Message(
  id: id,
  conversationId: 'conv-1',
  role: MessageRole.assistant,
  content: '今天午饭后散步了吗？',
  createdAt: DateTime.now(),
  kind: 'habit_nudge',
  messageRef: ref,
);

api.HabitNudgeDto _walkPlan() => api.HabitNudgeDto(
  id: 'nudge-1',
  pageSlug: 'habit/walk',
  question: '今天午饭后散步了吗？',
  doneLabel: '走了',
  doneText: '今天午饭后散步了',
  missedLabel: '没走',
  missedText: '今天还没走',
  weekdays: 127,
  localTime: '12:30',
  enabled: true,
  createdAt: '2026-10-22T00:00:00Z',
  updatedAt: '2026-10-22T00:00:00Z',
);
