// M2 §5.3 spike：自定义「AI 对话」block（Form A：页尾对话块，设计 §7）。
// 验证三件事：
//  1. 聊天块在编辑模式下可交互（输入→发送→回复），且输入不写进正文文档；
//  2. 交互后点击正文段落，编辑器选区/光标回到正文（不干扰选区）；
//  3. 只读模式下正文展示、聊天块仍可交互；
//  4. 聊天块不进 markdown 往返（编码器静默跳过）。
import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/wiki/wiki_chat_block.dart';
import 'package:elsewhen_ui/wiki/wiki_markdown_codec.dart';

const _bodyMd = '# 标题\n\n正文段落';
const _chatSlug = 'topic/投资';

void main() {
  group('wiki_chat block · 编辑模式', () {
    testWidgets('聊天块可交互：输入→发送→回复，且不污染正文文档', (tester) async {
      final repo = _FakeWikiChatRepo();
      final editorState = _editorWithChat();

      await tester.pumpWidget(_app(repo, editorState, editable: true));
      await tester.pump(); // ensureWikiPageChat 完成
      await tester.pump(); // setState(_ready=true) 生效

      // 聊天块与输入框就位且可用
      expect(find.text('AI对话'), findsOneWidget, reason: '聊天块应渲染页内 AI 面板');
      final input = find.byType(TextField);
      expect(input, findsOneWidget);
      expect(tester.widget<TextField>(input).enabled, isTrue);

      await tester.enterText(input, '写一段总结');
      await tester.tap(find.byIcon(Icons.arrow_upward));
      await tester.pump(); // _busy=true（生成中）
      await tester.pump(); // sendMessage + generateReply 完成
      await tester.pumpAndSettle(); // 滚到底动画 + ripple

      expect(repo.aiCalls, 1, reason: 'AI 应被调用一次');
      expect(
        find.text('模拟回复', findRichText: true),
        findsOneWidget,
        reason: 'AI 回复气泡应出现',
      );
      expect(
        editorState.document.toJson(),
        _editorWithChat().document.toJson(),
        reason: '聊天的输入只进会话，不写进正文文档',
      );
    });

    testWidgets('聊天交互后点击正文段落：选区/光标回到正文', (tester) async {
      final repo = _FakeWikiChatRepo();
      final editorState = _editorWithChat();

      await tester.pumpWidget(_app(repo, editorState, editable: true));
      await tester.pump();
      await tester.pump();

      // 先聚焦聊天输入框（模拟用户正在对话框里打字）
      await tester.tap(find.byType(TextField));
      await tester.pump();
      await tester.enterText(find.byType(TextField), '这个问题怎么答？');
      await tester.pump();

      // 点击正文段落 → 编辑器选区应落到正文段落（path [1]，聊天块在 [2]）
      await tester.tapAt(
        tester.getCenter(find.text('正文段落', findRichText: true)),
      );
      await tester.pump();

      final selection = editorState.selection;
      expect(selection, isNotNull, reason: '点击正文后编辑器应有选区');
      expect(selection!.start.path, [1], reason: '选区应落在正文段落');
      expect(selection.isCollapsed, isTrue, reason: '点击应为折叠光标');
    });
  });

  group('wiki_chat block · 只读模式', () {
    testWidgets('正文展示、聊天块仍可交互', (tester) async {
      final repo = _FakeWikiChatRepo();
      final editorState = _editorWithChat();

      await tester.pumpWidget(_app(repo, editorState, editable: false));
      await tester.pump();
      await tester.pump();

      expect(
        find.text('正文段落', findRichText: true),
        findsOneWidget,
        reason: '只读也要展示正文',
      );

      await tester.enterText(find.byType(TextField), '只读下也能聊');
      await tester.tap(find.byIcon(Icons.arrow_upward));
      await tester.pump();
      await tester.pump();
      await tester.pumpAndSettle();

      expect(repo.aiCalls, 1, reason: '只读模式聊天块仍可发消息');
      expect(
        find.text('模拟回复', findRichText: true),
        findsOneWidget,
        reason: '只读模式回复应出现',
      );
      expect(
        editorState.document.toJson(),
        _editorWithChat().document.toJson(),
        reason: '只读模式下聊天输入同样不写进正文',
      );
    });
  });

  group('wiki_chat block · 文档往返', () {
    test('聊天块不进 markdown 输出；节点携带 slug', () {
      final doc = wikiMarkdownToDocument(_bodyMd);
      doc.root.insert(wikiChatNode(slug: _chatSlug));

      expect(doc.root.children.last.type, WikiChatBlockKeys.type);
      expect(
        doc.root.children.last.attributes[WikiChatBlockKeys.slugAttribute],
        _chatSlug,
      );
      expect(
        wikiDocumentToMarkdown(doc),
        '$_bodyMd\n',
        reason: '聊天块无 NodeParser，编码器静默跳过，正文往返不受影响',
      );
    });

    test('builder 校验通过', () {
      final builder = WikiChatBlockComponentBuilder();
      expect(builder.validate(wikiChatNode(slug: _chatSlug)), isTrue);
    });
  });
}

/// 正文 + 尾部聊天块 的编辑器状态（每次新建全新文档，用于比对）
EditorState _editorWithChat() {
  final doc = wikiMarkdownToDocument(_bodyMd);
  doc.root.insert(wikiChatNode(slug: _chatSlug));
  return EditorState(document: doc);
}

Widget _app(
  RustBridgeRepository repo,
  EditorState editorState, {
  required bool editable,
}) {
  return ProviderScope(
    overrides: [storageRepositoryProvider.overrideWithValue(repo)],
    child: MaterialApp(
      localizationsDelegates: const [
        DefaultMaterialLocalizations.delegate,
        DefaultWidgetsLocalizations.delegate,
        AppFlowyEditorLocalizations.delegate,
      ],
      home: Scaffold(
        body: AppFlowyEditor(
          editorState: editorState,
          editable: editable,
          autoFocus: false,
          editorStyle: EditorStyle.desktop(),
          blockComponentBuilders: {
            ...standardBlockComponentBuilderMap,
            WikiChatBlockKeys.type: WikiChatBlockComponentBuilder(),
          },
        ),
      ),
    ),
  );
}

/// 页内对话的假仓库：不发 Rust，直接内存应答
class _FakeWikiChatRepo extends RustBridgeRepository {
  final List<Message> messages = [];
  int aiCalls = 0;

  @override
  Future<Conversation> ensureWikiPageChat(String pageSlug) async {
    return Conversation(
      id: 'conv-page',
      createdAt: DateTime(2026, 1, 1),
      updatedAt: DateTime(2026, 1, 1),
      messageCount: messages.length,
      wikiPageSlug: pageSlug,
    );
  }

  @override
  Future<List<Message>> listMessages(String conversationId) async =>
      List.of(messages);

  @override
  Future<Message> sendMessage(
    String conversationId,
    String role,
    String content, {
    String? parentMessageId,
  }) async {
    final msg = Message(
      id: 'm${messages.length + 1}',
      conversationId: conversationId,
      role: MessageRole.fromString(role),
      content: content,
      createdAt: DateTime.now(),
    );
    messages.add(msg);
    return msg;
  }

  @override
  Future<String> generateReply(
    String conversationId, {
    String? providerType,
    String? memoryType,
    int? memoryWindowSize,
  }) async {
    aiCalls++;
    return '模拟回复';
  }
}
