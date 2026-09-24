import 'package:elsewhen_ui/bridge/generated.dart/api.dart'
    show PendingActionDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'pending knowledge draft previews full content and saves only that draft',
    (tester) async {
      final repo = _DraftRepo();
      await tester.binding.setSurfaceSize(const Size(900, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            conversationRepositoryProvider.overrideWithValue(repo),
            selectedConversationIdProvider.overrideWith((ref) => 'conv-1'),
            messagesProvider.overrideWith((ref) async => []),
          ],
          child: const MaterialApp(home: Scaffold(body: MessageArea())),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('1 份待入库'), findsOneWidget);
      await tester.tap(find.text('1 份待入库'));
      await tester.pumpAndSettle();
      expect(find.text('待入库草稿（1）'), findsOneWidget);
      await tester.tap(find.text('闲鱼卖 CM4'));
      await tester.pumpAndSettle();
      expect(find.text('完整草稿正文', findRichText: true), findsOneWidget);
      expect(find.text('类型：topic · 待确认，未入库'), findsOneWidget);
      await tester.tap(find.text('保存到知识库'));
      await tester.pumpAndSettle();
      expect(repo.saved, ['conv-1:draft-1']);
      expect(find.text('0 份待入库'), findsOneWidget);
      expect(find.text('当前对话没有待入库草稿'), findsOneWidget);
      expect(find.textContaining('slug=kb-cm4'), findsOneWidget);
    },
  );

  testWidgets('failed draft save keeps preview open for retry', (tester) async {
    final repo = _DraftRepo()..fail = true;
    await tester.binding.setSurfaceSize(const Size(900, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith((ref) => 'conv-1'),
          messagesProvider.overrideWith((ref) async => []),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('1 份待入库'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('闲鱼卖 CM4'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('保存到知识库'));
    await tester.pumpAndSettle();
    expect(find.textContaining('保存失败'), findsOneWidget);
    expect(find.text('完整草稿正文', findRichText: true), findsOneWidget);
    repo.fail = false;
    await tester.tap(find.text('保存到知识库'));
    await tester.pumpAndSettle();
    expect(repo.saved, ['conv-1:draft-1', 'conv-1:draft-1']);
  });

  testWidgets(
    'today shows all drafts and deletion only declines selected draft',
    (tester) async {
      final repo = _DraftRepo()..includeSecond = true;
      await tester.binding.setSurfaceSize(const Size(900, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            conversationRepositoryProvider.overrideWithValue(repo),
            selectedConversationIdProvider.overrideWith((ref) => 'conv-1'),
            messagesProvider.overrideWith((ref) async => []),
          ],
          child: const MaterialApp(home: Scaffold(body: MessageArea())),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('2 份待入库'), findsOneWidget);
      await tester.tap(find.text('2 份待入库'));
      await tester.pumpAndSettle();
      expect(find.text('闲鱼卖 CM4'), findsOneWidget);
      expect(find.text('另一篇草稿'), findsOneWidget);
      await tester.tap(find.text('另一篇草稿'));
      await tester.pumpAndSettle();
      expect(find.text('另一篇完整正文', findRichText: true), findsOneWidget);
      await tester.tap(find.text('删除草稿').last);
      await tester.pumpAndSettle();
      expect(find.text('删除草稿？'), findsOneWidget);
      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();
      expect(repo.declined, isEmpty);
      await tester.tap(find.text('删除草稿').last);
      await tester.pumpAndSettle();
      await tester.tap(find.text('删除草稿').last);
      await tester.pumpAndSettle();
      expect(repo.declined, ['conv-1:draft-2']);
      expect(find.text('1 份待入库'), findsOneWidget);
      expect(find.text('另一篇草稿'), findsNothing);
      expect(find.text('闲鱼卖 CM4'), findsOneWidget);
      expect(repo.saved, isEmpty);
    },
  );

  testWidgets('today draft count and preview fit a narrow window', (
    tester,
  ) async {
    final repo = _DraftRepo();
    await tester.binding.setSurfaceSize(const Size(360, 760));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith((ref) => 'conv-1'),
          messagesProvider.overrideWith((ref) async => []),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('1 份待入库'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('1 份待入库'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('闲鱼卖 CM4'));
    await tester.pumpAndSettle();
    expect(find.text('完整草稿正文', findRichText: true), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}

class _DraftRepo extends ConversationRepository {
  _DraftRepo() : super(RustBridgeRepository());

  bool fail = false;
  bool savedDraft = false;
  bool includeSecond = false;
  final List<String> saved = [];
  final List<String> declined = [];

  @override
  Future<List<dynamic>> listPendingActions(String conversationId) async => [
    if (!savedDraft && !declined.contains('$conversationId:draft-1'))
      const PendingActionDto(
        id: 'draft-1',
        action: 'save_knowledge_draft',
        argsJson: '{"title":"闲鱼卖 CM4","kind":"topic","content_md":"完整草稿正文"}',
        createdAt: '2026-09-24T14:30:00Z',
      ),
    if (includeSecond && !declined.contains('$conversationId:draft-2'))
      const PendingActionDto(
        id: 'draft-2',
        action: 'save_knowledge_draft',
        argsJson: '{"title":"另一篇草稿","kind":"topic","content_md":"另一篇完整正文"}',
        createdAt: '2026-09-24T14:31:00Z',
      ),
    if (includeSecond)
      const PendingActionDto(
        id: 'todo-1',
        action: 'create_todo',
        argsJson: '{"title":"普通待办"}',
        createdAt: '2026-09-24T14:32:00Z',
      ),
  ];

  @override
  Future<void> declineKnowledgeDraft(
    String conversationId,
    String actionId,
  ) async {
    declined.add('$conversationId:$actionId');
  }

  @override
  Future<String> confirmKnowledgeDraft(
    String conversationId,
    String actionId,
  ) async {
    saved.add('$conversationId:$actionId');
    if (fail) throw StateError('写入失败');
    savedDraft = true;
    return '已保存知识页「闲鱼卖 CM4」（新创建，slug=kb-cm4）';
  }
}
