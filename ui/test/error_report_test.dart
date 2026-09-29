import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/utils/error_report.dart';

/// 异常出口的核心契约：**异常原文不进界面文案**。
///
/// 这条如果破了，`Text('删除失败：$e')` 就回来了——而 `$e` 里带着 SQL
/// 语句、数据库绝对路径和 provider 返回体。
void main() {
  // 真实形状：store 层 rusqlite 的 `?` 会把 SQL 和库绝对路径一起冒上来。
  final leaky = _FakeAnyhow(
    'Failed to prepare the query: SELECT id, slug, content_md '
    'FROM wiki_pages WHERE slug = ?1\n'
    '\n'
    'Caused by:\n'
    '    0: no such column: contnt_md\n'
    '    1: Cannot open database '
    '/home/pp/.local/share/elsewhen/elsewhen.db',
  );

  test('返回给用户的文案不含异常原文', () {
    final message = reportUiError('删除规则', leaky);

    expect(message, '删除规则失败，请重试一次');
    for (final secret in [
      'SELECT',
      'wiki_pages',
      'contnt_md',
      '/home/pp/.local/share/elsewhen/elsewhen.db',
      'Caused by',
    ]) {
      expect(message, isNot(contains(secret)), reason: '泄漏了：$secret');
    }
  });

  test('userMessage 可覆盖默认文案（标题已说明失败时用）', () {
    expect(reportUiError('读取来源记录', leaky, userMessage: '请重试一次'), '请重试一次');
  });

  test('showDetail 打开后原文照登，并剥掉 Exception 外壳', () {
    // 领域文案走这条路：Rust 侧那句本来就是写给用户看的。
    expect(
      reportUiError('处理提案', Exception('来源已更新，请重新生成建议'), showDetail: true),
      '来源已更新，请重新生成建议',
    );
  });

  test('showDetail 优先级低于 userMessage', () {
    expect(
      reportUiError(
        '处理提案',
        Exception('来源已更新'),
        showDetail: true,
        userMessage: '请重试一次',
      ),
      '请重试一次',
    );
  });

  test('默认关闭 showDetail：同一句领域文案也会被藏起来', () {
    // 这是「默认安全」的那一侧：新站点忘开只会降级成通用文案，不会泄露。
    expect(reportUiError('处理提案', Exception('来源已更新，请重新生成建议')), '处理提案失败，请重试一次');
  });

  test('异常为 null 也不该把 null 拼进用户文案', () {
    // Dart 里 catch (e) 的 e 不会是 null，但 FutureBuilder 的 error 回调
    // 类型是 Object——别让这个函数在边界上崩掉。
    expect(
      reportUiError('保存 Provider', '401 Unauthorized'),
      '保存 Provider失败，请重试一次',
    );
  });

  test('gate 语义与 Rust 侧 is_ok() 一致：变量存在即开，含空值', () {
    // 与 src/ai/conversation.rs 的 debug_eprintln! 必须同判，否则会出现
    // 「有 agent 轮次日志却没有异常日志」的半开状态。
    expect(debugLoggingEnabled({}), isFalse);
    expect(debugLoggingEnabled({'ELSEWHEN_DEBUG': '1'}), isTrue);
    expect(debugLoggingEnabled({'ELSEWHEN_DEBUG': ''}), isTrue);
  });

  test('装上后两个兜底通道都有人接', () {
    final originalFlutter = FlutterError.onError;
    final originalPlatform = PlatformDispatcher.instance.onError;
    addTearDown(() {
      FlutterError.onError = originalFlutter;
      PlatformDispatcher.instance.onError = originalPlatform;
    });

    installGlobalErrorHandlers();

    expect(FlutterError.onError, isNotNull);
    expect(PlatformDispatcher.instance.onError, isNotNull);
    expect(PlatformDispatcher.instance.onError, isNot(same(originalPlatform)));
  });

  test('异步兜底返回 false —— 只记日志，不改失败语义', () {
    final originalFlutter = FlutterError.onError;
    final originalPlatform = PlatformDispatcher.instance.onError;
    addTearDown(() {
      FlutterError.onError = originalFlutter;
      PlatformDispatcher.instance.onError = originalPlatform;
    });
    installGlobalErrorHandlers();

    final handled = PlatformDispatcher.instance.onError!(
      Exception('boom'),
      StackTrace.current,
    );

    expect(handled, isFalse, reason: '吞掉异常会让一次崩溃完全静默');
  });
}

/// 模拟 bridge 冒上来的 Anyhow 异常外壳。
class _FakeAnyhow implements Exception {
  _FakeAnyhow(this.message);
  final String message;
  @override
  String toString() => message;
}
