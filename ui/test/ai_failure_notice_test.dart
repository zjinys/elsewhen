import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';

/// AI 失败文案整理：只展示一行根因，不打印 Anyhow 的 Caused by 调用链。
void main() {
  test('Anyhow 调用链只保留根因一行（不含 stacktrace）', () {
    final err = _FakeAnyhow(
      'Failed to send request to AI provider\n'
      '\n'
      'Caused by:\n'
      '    0: error sending request for url (https://api.fengwind.com/v1/chat/completions)\n'
      '    1: client error (Connect)\n'
      '    2: Connection timed out (os error 110)',
    );
    expect(aiFailureNotice(err), 'AI 回复失败：Connection timed out (os error 110)');
  });

  test('无 Caused by 链时取首行', () {
    expect(
      aiFailureNotice(Exception('网络请求失败\n第二行细节')),
      'AI 回复失败：Exception: 网络请求失败',
    );
  });

  test('单行 AnyhowException 去掉外壳', () {
    expect(
      aiFailureNotice(_FakeAnyhow('401 Unauthorized')),
      'AI 回复失败：401 Unauthorized',
    );
  });

  test('未配置 provider：引导去设置页', () {
    expect(
      aiFailureNotice(_FakeAnyhow('No active AI provider')),
      '尚未配置 AI Provider，请到设置页填写后重试',
    );
  });
}

/// 模拟 flutter_rust_bridge 的 AnyhowException（其 toString 形如
/// `AnyhowException(<anyhow Debug 链>)`，含 Caused by 逐级原因）。
class _FakeAnyhow implements Exception {
  final String message;
  _FakeAnyhow(this.message);

  @override
  String toString() => 'AnyhowException($message)';
}
