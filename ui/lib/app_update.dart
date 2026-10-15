import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../bridge/api.dart' as api;
import 'utils/desktop_prefs.dart';

/// 应用内升级（设置页「检查更新」）。
///
/// Android 侧的安装/缓存目录通道（MainActivity.kt dev.elsewhen/app_update）。
const MethodChannel _androidChannel = MethodChannel('dev.elsewhen/app_update');

/// 分发走同步 relay 同域名（Worker + R2）：桌面/手机都已验证该域名在
/// clash 路径下可达；下载由 Rust 侧流式落盘 + sha256 校验 + 断点续传。
/// 桌面端是源码运行，「升级」语义是拉代码重跑脚本，故只显示引导命令；
/// Android 拉起系统安装器覆盖安装（debug 签名跨构建一致）。
class AppUpdate {
  AppUpdate._();

  /// 从设置发起一次完整升级流程（检查 → 展示 → 视平台下载/引导）。
  static Future<void> runFromSettings(BuildContext context) async {
    final relayUrl = const DesktopPrefsStore().loadLastOwnerRelayUrl();
    if (relayUrl == null) {
      await _show(
        context,
        title: '无法检查更新',
        message: '尚未配置中继地址：先在「设置 → 同步」完成一次配对（升级清单与同步共用同一中继）。',
      );
      return;
    }
    final api.AppUpdateDto? update;
    try {
      update = await api.checkAppUpdate(relayUrl: relayUrl);
    } catch (e) {
      if (!context.mounted) return;
      await _show(context, title: '检查更新失败', message: friendly(e.toString()));
      return;
    }
    if (!context.mounted) return;
    if (!update.hasUpdate) {
      await _show(
        context,
        title: '已是最新版本',
        message: '当前 ${update.localHash}（序号 ${update.localSeq}）就是发布清单里的最新构建。',
      );
      return;
    }
    if (!context.mounted) return;
    final goOn = await _showUpdateDialog(context, update);
    if (!goOn || !context.mounted) return;
    if (Platform.isAndroid) {
      await _downloadAndInstall(context, relayUrl, update);
    } else {
      await _showDesktopGuidance(context, update);
    }
  }

  static Future<void> _downloadAndInstall(
    BuildContext context,
    String relayUrl,
    api.AppUpdateDto update,
  ) async {
    final messenger = ScaffoldMessenger.of(context);
    unawaited(
      showDialog<void>(
        context: context,
        barrierDismissible: false,
        builder: (_) => const AlertDialog(
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              CircularProgressIndicator(),
              SizedBox(height: 16),
              Text('正在下载更新（支持断点续传）…'),
            ],
          ),
        ),
      ),
    );
    String apkPath;
    try {
      apkPath = await api.downloadUpdateApk(
        relayUrl: relayUrl,
        buildHash: update.latestHash,
        apkSha256: update.apkSha256,
        apkSize: update.apkSize,
        destDir: (await _updatesCacheDir()).path,
      );
    } catch (e) {
      if (context.mounted) Navigator.of(context, rootNavigator: true).pop();
      messenger.showSnackBar(
        SnackBar(content: Text('下载失败：${friendly(e.toString())}')),
      );
      return;
    }
    if (context.mounted) Navigator.of(context, rootNavigator: true).pop();
    try {
      await _androidChannel.invokeMethod<bool>('installApk', {'path': apkPath});
    } on PlatformException catch (e) {
      messenger.showSnackBar(
        SnackBar(content: Text('无法启动安装器：${e.message ?? e.code}')),
      );
    }
  }

  static Future<void> _showDesktopGuidance(
    BuildContext context,
    api.AppUpdateDto update,
  ) async {
    const command =
        'git pull --ff-only && ./scripts/regen.sh && ./scripts/release-run.sh';
    await _show(
      context,
      title: '发现新版本 ${update.version}',
      message:
          '${update.latestHash}（序号 ${update.latestSeq}，当前 ${update.localHash}）\n\n'
          '${update.changelog}\n\n桌面端在源码目录运行：\n$command',
      actionText: '复制命令',
      action: () => Clipboard.setData(const ClipboardData(text: command)),
    );
  }

  static Future<bool> _showUpdateDialog(
    BuildContext context,
    api.AppUpdateDto update,
  ) async {
    final sizeMb = (update.apkSize / 1024 / 1024).toStringAsFixed(1);
    final result = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('发现新版本 ${update.version}'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '${update.latestHash}（序号 ${update.latestSeq}，当前 ${update.localHash}）',
            ),
            const SizedBox(height: 8),
            Text(update.changelog),
            const SizedBox(height: 8),
            Text('大小约 $sizeMb MB'),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('暂不'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('更新'),
          ),
        ],
      ),
    );
    return result ?? false;
  }

  static Future<void> _show(
    BuildContext context, {
    required String title,
    required String message,
    String? actionText,
    VoidCallback? action,
  }) async {
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(title),
        content: Text(message),
        actions: [
          if (actionText != null && action != null)
            TextButton(
              onPressed: () {
                action();
                Navigator.of(context).pop();
              },
              child: Text(actionText),
            ),
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('好'),
          ),
        ],
      ),
    );
  }

  /// 把报错行话翻译成用户语言；内部错误码保留在原文里供日志比对。
  static String friendly(String error) {
    if (error.contains('release manifest failed validation')) {
      return '升级清单格式异常（中继数据不可信），放弃安装。';
    }
    if (error.contains('sha256 mismatch')) {
      return '下载内容校验未通过，已丢弃（网络或中继异常），可重试。';
    }
    if (error.contains('release apk stream ended early')) {
      return '下载中断（网络不稳），已保留进度，重试会接着下。';
    }
    if (error.contains('no release published') || error.contains('404')) {
      return '发布通道还没有任何版本（跑过 scripts/publish-release.sh 了吗？）';
    }
    return error.length > 200 ? '${error.substring(0, 200)}…' : error;
  }
}

/// 升级缓存目录：`<cacheDir>/updates`（生命周期随缓存，可随时清）。
/// Android 由 method channel 取应用私有 cacheDir（不硬编码路径）；桌面
/// 用 XDG_CACHE_HOME，避免为此引入 path_provider 依赖。
Future<Directory> _updatesCacheDir() async {
  final basePath = Platform.isAndroid
      ? await _androidChannel.invokeMethod<String>('getCacheDir')
      : () {
          final xdg = Platform.environment['XDG_CACHE_HOME'];
          final home = Platform.environment['HOME'];
          return xdg ?? (home != null ? '$home/.cache/elsewhen' : null);
        }();
  if (basePath == null) {
    throw StateError('app update cache directory unavailable');
  }
  final dir = Directory('$basePath${Platform.pathSeparator}updates');
  if (!dir.existsSync()) dir.createSync(recursive: true);
  return dir;
}
