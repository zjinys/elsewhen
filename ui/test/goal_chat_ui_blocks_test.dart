import 'dart:convert';

import 'package:ai_ui_render/ai_ui_render.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/goal_chat_ui_blocks.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  const sampleTree = {
    'root': 'card',
    'elements': {
      'card': {
        'key': 'card',
        'type': 'GoalCard',
        'props': {'title': '阶段盘点'},
        'children': ['h', 'm1', 'acts'],
      },
      'h': {
        'key': 'h',
        'type': 'GoalHeading',
        'props': {'text': '当前状态'},
        'children': [],
      },
      'm1': {
        'key': 'm1',
        'type': 'GoalMetric',
        'props': {'label': '行动停留', 'value': '5天'},
        'children': [],
      },
      'acts': {
        'key': 'acts',
        'type': 'GoalChoices',
        'props': {
          'options': [
            {'label': '调整当前行动', 'message': '帮我把行动滚动到下一个阶段'},
          ],
        },
        'children': [],
      },
    },
  };

  String fenced(Map<String, dynamic> tree) =>
      '先给结论：行动有点滞。\n```elsewhen-ui\n${jsonEncode(tree)}\n```';

  test('extractGoalUiBlock：剥块正文并解析出 UI 树', () {
    final data = extractGoalUiBlock(fenced(sampleTree));
    expect(data, isNotNull);
    expect(data!.strippedContent, '先给结论：行动有点滞。');
    expect(data.tree.root, 'card');
    expect(data.tree.elements.containsKey('card'), isTrue);
  });

  test('extractGoalUiBlock：无块 / 坏 JSON 返回 null（回退纯文本）', () {
    expect(extractGoalUiBlock('普通回复，没有块'), isNull);
    expect(
      extractGoalUiBlock('```elsewhen-ui\n{"root": "x", "elements": {}}\n```'),
      isNull,
    );
    expect(extractGoalUiBlock('```elsewhen-ui\n不是 JSON\n```'), isNull);
  });

  testWidgets('GoalUiBlock：目录组件渲染为原生卡片，未知类型静默跳过', (tester) async {
    AppTheme.apply(Brightness.dark);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [
              GoalUiBlock(
                tree: UITree.fromJson(sampleTree),
                onAction: (name, p) {
                  p;
                },
              ),
            ],
          ),
        ),
      ),
    );
    await tester.pump();

    // guardrail 组件渲染成真实文案
    expect(find.text('阶段盘点'), findsOneWidget);
    expect(find.text('当前状态'), findsOneWidget);
    expect(find.text('5天'), findsOneWidget);
    expect(find.text('行动停留'), findsOneWidget);
    // GoalChoices 渲染成点选按钮
    expect(find.text('调整当前行动'), findsOneWidget);
  });

  testWidgets('GoalUiBlock：点选项触发 send_message 动作并携带 message', (tester) async {
    AppTheme.apply(Brightness.dark);
    String? actionName;
    Map<String, dynamic>? params;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: ListView(
            children: [
              GoalUiBlock(
                tree: UITree.fromJson(sampleTree),
                onAction: (name, p) {
                  actionName = name;
                  params = p;
                },
              ),
            ],
          ),
        ),
      ),
    );
    await tester.pump();
    await tester.tap(find.text('调整当前行动'));
    expect(actionName, 'send_message');
    expect(params?['message'], '帮我把行动滚动到下一个阶段');
  });
}
