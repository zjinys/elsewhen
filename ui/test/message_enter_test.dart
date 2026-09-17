import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// 验证：
/// ① 回车直接提交（多行输入下不依赖发送按钮）
/// ② 写库失败 & AI 回复失败都有可见的 SnackBar 反馈（不静默）
/// 用假 ConversationRepository 确定性抛错，避免真实 FFI 在测试 fake-async 区不完成。
void main() {
  testWidgets('Enter 提交 + 发送/AI 错误反馈', (tester) async {
    final repo = _FakeConversationRepo();
    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          conversationRepositoryProvider.overrideWithValue(repo),
          selectedConversationIdProvider.overrideWith((ref) => 'conv-1'),
          messagesProvider.overrideWith((ref) async => <Message>[]),
        ],
        child: const MaterialApp(
          home: Scaffold(body: MessageArea()),
        ),
      ),
    );
    await tester.pump();

    final input = find.byType(TextField);
    expect(input, findsOneWidget, reason: '有选中会话时应出现输入框');

    // ① 回车提交，写库失败 → 错误 SnackBar + 文本保留可重试
    await tester.enterText(input, 'fail-send');
    await tester._pressEnter();
    expect(
      find.textContaining('发送失败'),
      findsOneWidget,
      reason: '写库失败应弹出明确反馈',
    );
    expect(tester.widget<TextField>(input).controller!.text, 'fail-send',
        reason: '失败时保留文本便于重试');

    // 清掉 SnackBar（分段推进时钟，SnackBar 时长 Timer 才触发）
    await tester._clearSnackBars();

    // ② 写库成功、AI 回复失败 → 文本清空 + 'AI 回复失败' SnackBar
    expect(repo.sentContents, isEmpty);
    await tester.enterText(input, 'ok-ai-fail');
    await tester._pressEnter();
    expect(repo.sentContents, ['ok-ai-fail'], reason: '消息应已写库');
    expect(tester.widget<TextField>(input).controller!.text, isEmpty,
        reason: '写库成功后输入框清空');
    expect(
      find.textContaining('AI 回复失败'),
      findsOneWidget,
      reason: 'AI 调用失败应弹出可见反馈',
    );

    // 清掉 SnackBar
    await tester._clearSnackBars();

    // ③ Shift+Enter → 换行而非提交
    await tester.enterText(input, '第二行');
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester._pressEnter();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pump();

    expect(repo.sentContents, ['ok-ai-fail'], reason: 'Shift+Enter 不应触发提交');
    expect(tester.widget<TextField>(input).controller!.text, '第二行');
    expect(find.textContaining('发送失败'), findsNothing);
    // AI 失败提示是「会话内气泡」：选中的会话内持久展示，
    // 仅在下次发送成功 / 切换会话时清除 —— Shift+Enter 未提交自然不清除。
    expect(
      find.textContaining('AI 回复失败'),
      findsOneWidget,
      reason: 'AI 失败提示为会话内气泡，未发送成功前保留（便于用户看到失败原因）',
    );
  });
}

/// 模拟：sendMessage 内容为 'fail-send' 时失败；generateReply 一律失败
class _FakeConversationRepo extends ConversationRepository {
  _FakeConversationRepo() : super(RustBridgeRepository());
  final List<String> sentContents = [];
  int aiCalls = 0;

  @override
  Future<Message> sendMessage(String conversationId, String content) async {
    if (content == 'fail-send') {
      throw Exception('写库失败(模拟)');
    }
    sentContents.add(content);
    return Message(
      id: 'm${sentContents.length}',
      conversationId: conversationId,
      parentMessageId: null,
      role: MessageRole.user,
      content: content,
      createdAt: DateTime.now(),
    );
  }

  @override
  Future<String> generateReply(String conversationId) async {
    aiCalls++;
    throw Exception('AI 调用失败(模拟)');
  }
}

extension on WidgetTester {
  Future<void> _pressEnter() async {
    await sendKeyDownEvent(LogicalKeyboardKey.enter);
    await sendKeyUpEvent(LogicalKeyboardKey.enter);
    await pump();
  }

  /// 分段推进时钟直到 SnackBar 完全退场（fake-async 里时长 Timer 需分步触发）
  Future<void> _clearSnackBars() async {
    for (var i = 0; i < 12 && any(find.byType(SnackBar)); i++) {
      await pump(const Duration(seconds: 1));
    }
  }
}