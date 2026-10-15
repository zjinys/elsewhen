import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/widgets/chat_page_task_scope.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';

import 'chat_run_widget_test.dart' show app, settleFrames;
import 'support/chat_run_fixture.dart';

class TaskBridge extends RustBridgeRepository {
  final RunTestRepository repo;
  final Map<String, WikiChatMaterialDto> materials;
  String? saved;
  int restores = 0;
  TaskBridge(this.repo, this.materials, this.saved);

  @override
  bool get supportsPageTasks => true;
  @override
  Future<String?> getWikiChatTask(String conversationId) async {
    restores++;
    return saved;
  }

  @override
  Future<WikiChatMaterialDto> getWikiChatMaterial(String slug) async =>
      materials[slug]!;
  @override
  Future<Conversation> ensureWikiPageChat(String slug) async => Conversation(
    id: 'conv-1',
    wikiPageSlug: slug,
    messageCount: repo.messages.length,
    createdAt: DateTime(2026, 10, 2),
    updatedAt: DateTime(2026, 10, 2),
  );
  @override
  Future<List<Message>> listMessages(String conversationId) =>
      repo.getMessages(conversationId);
}

WikiChatMaterialDto material(String slug, String version) =>
    WikiChatMaterialDto(
      slug: slug,
      title: slug,
      contentMd: '原文😀尾部',
      basisJson: jsonEncode({
        'slug': slug,
        'revision': {'hash': version},
      }),
    );

void main() {
  testWidgets(
    'wiki panel reopens the saved compare task with both displayed bases',
    (tester) async {
      final repo = RunTestRepository();
      final a = material('topic/test', 'v1');
      final b = material('topic/other', 'v1');
      final saved = ChatPageTask('compare', [a, b]);
      final bridge = TaskBridge(repo, {a.slug: a, b.slug: b}, saved.json);
      for (var mount = 0; mount < 2; mount++) {
        await tester.pumpWidget(
          app(
            repo,
            const WikiAiChatPanel(slug: 'topic/test'),
            wikiBridge: bridge,
          ),
        );
        await settleFrames(tester);
        final scope = tester.widget<ChatPageTaskScope>(
          find.byType(ChatPageTaskScope),
        );
        expect(scope.task.kind, 'compare');
        expect(jsonDecode(scope.task.json), jsonDecode(saved.json));
        expect(scope.task.materials, hasLength(2));
        await tester.pumpWidget(const SizedBox());
        await tester.pump();
      }
      expect(bridge.restores, 2);
    },
  );

  testWidgets('changed material blocks input until explicit task reset', (
    tester,
  ) async {
    final repo = RunTestRepository();
    final old = material('topic/test', 'v1');
    final current = material('topic/test', 'v2');
    final bridge = TaskBridge(repo, {
      current.slug: current,
    }, ChatPageTask('local_edit', [old], startChar: 1, endChar: 3).json);
    await tester.pumpWidget(
      app(repo, const WikiAiChatPanel(slug: 'topic/test'), wikiBridge: bridge),
    );
    await settleFrames(tester);
    expect(find.byType(ChatPageTaskScope), findsNothing);
    expect(tester.widget<TextField>(find.byType(TextField)).enabled, isFalse);
    expect(find.textContaining('不会自动沿用新版本'), findsOneWidget);
    await tester.ensureVisible(find.text('明确重新选择任务与材料'));
    await tester.tap(find.text('明确重新选择任务与材料'));
    await settleFrames(tester);
    final task = tester
        .widget<ChatPageTaskScope>(find.byType(ChatPageTaskScope))
        .task;
    expect(task.kind, 'local_question');
    expect(task.startChar, isNull);
    expect(jsonDecode(task.json)['pages'][0]['revision']['hash'], 'v2');
    expect(tester.widget<TextField>(find.byType(TextField)).enabled, isTrue);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
  });

  testWidgets(
    'translation shortcut declares a local fragment rather than a full page',
    (tester) async {
      final repo = RunTestRepository();
      final page = material('topic/test', 'v1');
      final bridge = TaskBridge(repo, {
        page.slug: page,
      }, ChatPageTask('full_summary', [page]).json);
      await tester.pumpWidget(
        app(
          repo,
          const WikiAiChatPanel(slug: 'topic/test'),
          wikiBridge: bridge,
        ),
      );
      await settleFrames(tester);
      await tester.tap(find.byTooltip('AI 任务'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('翻译片段'));
      await tester.pumpAndSettle();
      final task = tester
          .widget<ChatPageTaskScope>(find.byType(ChatPageTaskScope))
          .task;
      expect(task.kind, 'local_question');
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        contains('不声称翻译整页'),
      );
      await tester.pumpWidget(const SizedBox());
      await tester.pump();
    },
  );
}
