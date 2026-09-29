/// 异常的统一出口：**诊断留痕，界面只见通用文案**。
///
/// 为什么要这一层：Rust 侧错误经 flutter_rust_bridge 原样冒到 Dart，
/// `rusqlite` 的 `?` 会把 SQL 语句、数据库绝对路径、provider 返回体一并
/// 带上来。`Text('删除失败：$e')` 这种写法等于把内部结构摊在界面上，而且
/// 排障时除了截图没有别的留痕。
///
/// 落点是 stderr 而不是文件：与 Rust 侧 `debug_eprintln!`
/// （`src/ai/conversation.rs`）共用 `ELSEWHEN_DEBUG` 这一个 gate 和
/// `scripts/debug-run.sh` 那一条 `tee` 管道，Dart 不自己开文件 sink。
///
/// **所以返回的文案不能说「详情已记录」**：不经 `scripts/debug-run.sh`
/// 启动时并不会留下任何记录，那种提示是假的。
library;

import 'dart:io' show Platform, stderr;

import 'package:flutter/foundation.dart';

/// 诊断开关，与 Rust 侧 `debug_eprintln!` 的 `std::env::var(..).is_ok()`
/// 语义一致——变量存在即开启（含空值）。
///
/// 两边必须同判。否则会出现「有 agent 轮次日志、却没有异常日志」这种
/// 半开状态，排障时比全关更容易把人带偏。
@visibleForTesting
bool debugLoggingEnabled([Map<String, String>? env]) =>
    (env ?? Platform.environment).containsKey('ELSEWHEN_DEBUG');

/// 记录一次异常，返回**可以安全显示给用户**的文案。
///
/// [context] 是中文短动作标签（「删除规则」「保存 Provider」）。它会进日志，
/// 也出现在用户看到的文案里，所以只放动作、不放细节。
///
/// [userMessage] 用于「标题已经说明了是什么失败」的场合（比如对话框标题
/// 写着「读取失败」，正文就不该再重复一遍）。
///
/// [showDetail] 是**逐站点显式开的例外**：默认隐藏，只有确认这条失败路径
/// 背后 Rust 侧只 `bail` 可读的领域文案（表单校验、来源变更提示之类）时
/// 才打开。打开后原文照登。
///
/// 为什么不给「干净文案」加自动判据：`bail!` 有 151 处，其中约 67 处是
/// 刻意写给用户看的中文，其余是内部失败。任何启发式（无换行 / 无
/// `Caused by` / 无 `SELECT`）都会在某天放行一条长得干净的内部错误，
/// 而且没人会发现。所以这里是**人逐站点确认**，默认方向选安全的那一侧。
String reportUiError(
  String context,
  Object error, {
  StackTrace? stack,
  String? userMessage,
  bool showDetail = false,
}) {
  if (debugLoggingEnabled()) {
    final now = DateTime.now().toIso8601String();
    try {
      stderr.writeln('[$now][ui] $context 失败：$error');
      if (stack != null) {
        stderr.writeln('[$now][ui] $context 堆栈：$stack');
      }
    } catch (_) {
      // 诊断是旁路：日志写失败绝不能把主流程带崩。
    }
  }

  if (userMessage != null) return userMessage;
  if (showDetail) {
    return error.toString().replaceFirst('Exception: ', '');
  }
  return '$context失败，请重试一次';
}

/// 装上兜底的全局异常处理，让**没人 try/catch 的**异常也走同一条诊断通道。
///
/// 调用点自己接住的异常不会到这里——那些站点各自调 [reportUiError]。
/// 这里接的是漏网的：widget 构建错误、没被 await 的 Future。
///
/// 两个 handler 都是「先记日志，再交回默认处理」，**不吞异常**：失败语义
/// 与改动前一致，本模块只增加可观测性。代价是诊断开启时同一异常会打两遍
/// （本模块一条结构化的，框架默认一条带彩色的），换来的是 `flutter run`
/// 里那份带完整堆栈的输出没有丢。
void installGlobalErrorHandlers() {
  final previous = FlutterError.onError;
  FlutterError.onError = (FlutterErrorDetails details) {
    reportUiError('未捕获的框架异常', details.exception, stack: details.stack);
    previous?.call(details);
  };

  PlatformDispatcher.instance.onError = (Object error, StackTrace stack) {
    reportUiError('未捕获的异步异常', error, stack: stack);
    // false = 交回默认处理。不返回 true 是有意的：吞掉异常会让一次崩溃
    // 完全静默，而这里只是加日志，不该顺手改变应用的失败行为。
    return false;
  };
}
