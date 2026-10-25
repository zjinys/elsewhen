import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// 配对与初始快照传输期间的「别把我冻死」闸门（仅 Android 有实际动作）。
///
/// 背景：OPPO / 一加（OplusHansManager）等 ROM 会冻结没有前台服务的后台进程。
/// 进程一冻，Dart 的定时器、socket 和 Rust 的分片传输全部停摆——配对会停在
/// 「等待成员授权与初始同步」，初始快照传到一半再也不动，用户看到的就是
/// 「切到别的应用就假死」。带常驻通知的前台服务进程默认豁免冻结，所以这里
/// 在传输期间挂一个 FGS，结束后立刻撤掉。
///
/// 用法是**计数**而不是布尔：配对请求、快照传输、成员名册交换可能同时都在跑，
/// 谁最后释放谁关灯，避免一方提前把服务关掉让另一方重新被冻。
///
/// 刻意只包住「需要连续跑」的窗口（几秒到几分钟），不做常驻保活：常驻通知
/// 是明确的产品代价，不能因为一次配对就长期占着通知栏。
class SyncKeepAlive {
  SyncKeepAlive._();

  static const MethodChannel _channel = MethodChannel(
    'dev.elsewhen/sync_keepalive',
  );

  static final Map<String, String> _reasonTexts = <String, String>{};

  /// 测试钩子：绕开平台判断（单测跑在 Linux 上，默认为空操作）。
  @visibleForTesting
  static bool? debugSupportedOverride;

  /// 测试钩子：接管真实通道调用，只记录调用序列。
  @visibleForTesting
  static void Function(String method, Object? arguments)? debugInvoke;

  /// 桌面/测试环境没有前台服务概念，所有调用都是空操作。
  static bool get supported =>
      debugSupportedOverride ?? (!kIsWeb && Platform.isAndroid);

  /// 当前是否已经拉起前台服务。
  static bool get active => _reasonTexts.isNotEmpty;

  /// 申请一段保活，[reason] 是幂等的持有者键。
  ///
  /// 刻意做成同步 void：调用点大多在配对状态机里（`_action` / `_tick`），
  /// 中间插一个 await 会多让出一次微任务，改变「关闭先于下一步」这类既有
  /// 顺序语义。真正失败也只记日志，没有调用方需要等它的结果。
  static void hold(String reason, {String? text}) {
    if (!supported) return;
    final alreadyHeld = _reasonTexts.isNotEmpty;
    _reasonTexts[reason] = text ?? '正在与另一台设备同步；请保持应用运行。';
    if (alreadyHeld) return;
    unawaited(_invoke('start', <String, Object?>{'text': _reasonTexts[reason]}));
  }

  /// 释放 [reason] 的保活；最后一个持有者离开时才真正停掉服务。
  static void release(String reason) {
    if (!supported) return;
    _reasonTexts.remove(reason);
    if (_reasonTexts.isNotEmpty) return;
    unawaited(_invoke('stop'));
  }

  /// 退出登录/锁定/关闭应用等收尾场景一次清空，避免前台服务漏挂。
  static void releaseAll() {
    if (!supported) return;
    final wasHeld = _reasonTexts.isNotEmpty;
    _reasonTexts.clear();
    if (!wasHeld) return;
    unawaited(_invoke('stop'));
  }

  static Future<void> _invoke(String method, [Object? arguments]) async {
    final recorder = debugInvoke;
    if (recorder != null) {
      recorder(method, arguments);
      return;
    }
    try {
      await _channel.invokeMethod<bool>(method, arguments);
    } on MissingPluginException {
      // 非 Android 宿主或插件未注册（例如单测/桌面构建）：静默降级。
    } on PlatformException catch (failure) {
      // 保活是优化项，任何失败都不能让配对流程抛错。
      debugPrint('[sync] keep-alive $method failed: ${failure.message}');
    }
  }
}
