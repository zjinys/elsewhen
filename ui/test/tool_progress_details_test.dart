import 'dart:convert';

import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/tool_progress_details.dart';
import 'package:elsewhen_ui/widgets/chat_run_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/chat_run_fixture.dart';

void main() {
  test('evidence projection preserves returned identity and ignores private fields', () {
    final details = toolProgressDetails(
      'search_knowledge_base',
      'read',
      jsonEncode({
        'knowledge_candidates': [
          {
            'title': '测试材料',
            'excerpt': '原始证据',
            'content_hash': 'old-hash',
            'sources': [
              {'title': '原始来源', 'snapshot_id': 'snapshot-7', 'version': 7},
            ],
            'reasoning_content': 'private reasoning',
          },
        ],
        'internal_protocol': 'secret',
      }),
    );
    expect(details, contains('原始证据'));
    expect(details, contains('snapshot-7'));
    expect(details, contains('old-hash'));
    expect(details, isNot(contains('private reasoning')));
    expect(details, isNot(contains('secret')));
    expect(details, contains('最终回答引用另行核验'));
  });

  test('failure and unsupported output never dump arbitrary protocols', () {
    expect(
      toolProgressDetails('unknown', 'read', '{"private":"secret"}'),
      isEmpty,
    );
    expect(
      toolProgressDetails('create_todo', 'failed', 'secret'),
      isNot(contains('secret')),
    );
    expect(
      toolProgressDetails('list_rules', 'read', '<think>private</think>'),
      isNot(contains('private')),
    );
    expect(
      toolProgressDetails('list_rules', 'read', 'x' * 10000).length,
      lessThan(1250),
    );
    expect(
      toolProgressDetails(
        'save_knowledge_draft',
        'proposal_ready',
        'private JSON',
      ),
      contains('审阅卡'),
    );
  });

  testWidgets(
    'expanded real tool step shows bounded business evidence, not raw JSON',
    (tester) async {
      final repo = RunTestRepository();
      // 完成态已静默收场；工具步骤回顾仅在运行/停止等未完成态展示。
      repo.runs['conv-1'] = fixtureRun(status: 'cancelled', sequence: 3);
      repo.events.addAll([
        fixtureEvent(
          1,
          'tool_started',
          payload: {'call_id': 'lookup', 'name': 'get_wiki_page'},
        ),
        fixtureEvent(
          2,
          'tool_result',
          payload: {
            'call_id': 'lookup',
            'name': 'get_wiki_page',
            'outcome': 'read',
            'content': jsonEncode({
              'knowledge_candidates': [
                {
                  'title': '测试材料',
                  'excerpt': '原始证据',
                  'content_hash': 'old-hash',
                  'sources': [],
                },
              ],
              'reasoning_content': 'private',
            }),
          },
        ),
        fixtureEvent(3, 'completed'),
      ]);
      await tester.pumpWidget(
        ProviderScope(
          overrides: [conversationRepositoryProvider.overrideWithValue(repo)],
          child: const MaterialApp(
            home: Scaffold(body: ChatRunStatus(conversationId: 'conv-1')),
          ),
        ),
      );
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pump();
      await tester.tap(find.text('实际工具步骤（最近 1 项）'));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.textContaining('原始证据'), findsOneWidget);
      expect(find.textContaining('old-hash'), findsOneWidget);
      expect(find.textContaining('reasoning_content'), findsNothing);
      expect(find.text('查询已完成'), findsOneWidget);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
