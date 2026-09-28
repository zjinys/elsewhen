import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';

/// 复制能力验证：每条消息可复制、可一键复制全部对话（含角色/时间）。
/// 用假 repo + 假消息列表，拦截系统剪贴板通道，全程无需 FFI。
void main() {
  testWidgets('message copy + copy all', (tester) async {
    final repo = _FakeConversationRepo();
    final messages = [
      Message(
        id: 'm1',
        conversationId: 'conv-1',
        parentMessageId: null,
        role: MessageRole.user,
        content: '帮我总结一下今天的要点',
        createdAt: DateTime(2026, 9, 15, 10, 0),
      ),
      Message(
        id: 'm2',
        conversationId: 'conv-1',
        parentMessageId: 'm1',
        role: MessageRole.assistant,
        content: '要点如下：\n1. 配置 AI\n2. 保存生效',
        createdAt: DateTime(2026, 9, 15, 10, 1),
      ),
    ];

    // 拦截系统剪贴板
    final clipboardCalls = <MethodCall>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (MethodCall call) async {
        if (call.method == 'Clipboard.setData') {
          clipboardCalls.add(call);
        }
        return null;
      },
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith(
            () => StateHolder('conv-1'),
          ),
          messagesProvider.overrideWith((ref) async => messages),
        ],
        child: const MaterialApp(home: Scaffold(body: MessageArea())),
      ),
    );
    await tester.pump();

    // 界面元素：用户气泡 SelectableText + AI 气泡 MarkdownView + 2 个单条复制按钮 + 1 个复制全部
    expect(
      find.byType(SelectableText),
      findsOneWidget,
      reason: '用户气泡为 SelectableText',
    );
    expect(
      find.byType(MarkdownView),
      findsOneWidget,
      reason: 'AI 气泡经 MarkdownView 渲染',
    );
    expect(find.byIcon(Icons.copy_rounded), findsNWidgets(2));
    expect(find.byIcon(Icons.copy_all_rounded), findsOneWidget);

    // ① 复制单条（AI 那条）
    await tester.tap(find.byIcon(Icons.copy_rounded).at(1));
    await tester.pump();
    expect(clipboardCalls, isNotEmpty);
    final singleText = (clipboardCalls.last.arguments as Map)['text'] as String;
    expect(singleText, messages[1].content, reason: '复制的是该条消息原文');
    expect(find.textContaining('已复制消息'), findsOneWidget);

    // ② 复制全部对话
    await tester._clearSnackBars();
    await tester.tap(find.byIcon(Icons.copy_all_rounded));
    await tester.pump();
    final allText = (clipboardCalls.last.arguments as Map)['text'] as String;
    expect(allText, contains('我 (09-15 10:00)'));
    expect(allText, contains(messages[0].content));
    expect(allText, contains('AI (09-15 10:01)'));
    expect(allText, contains(messages[1].content));
    expect(find.textContaining('已复制对话'), findsOneWidget);
  });
}

class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());
}

extension on WidgetTester {
  /// 分段推进时钟直到 SnackBar 完全退场
  Future<void> _clearSnackBars() async {
    for (var i = 0; i < 12 && any(find.byType(SnackBar)); i++) {
      await pump(const Duration(seconds: 1));
    }
  }
}
