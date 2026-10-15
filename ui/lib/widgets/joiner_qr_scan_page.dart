import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

/// 扫码读取已有设备出示的邀请二维码。只读取邀请码文本本身，画面不存储。
///
/// [scannerBuilder] 是测试注入缝：默认用相机（mobile_scanner）；测试里注入
/// 可模拟检测的替换件，避免在 CI 里真的打开相机。
class JoinerQrScanPage extends StatelessWidget {
  const JoinerQrScanPage({
    super.key,
    required this.onDetected,
    this.scannerBuilder,
  });

  /// 读到二维码后回调；重复检测会被忽略（本页只 pop 一次）。
  final void Function(String code) onDetected;

  /// 或：注入假的取景器；签名给出本页应调用的 code 回调。
  final Widget Function(BuildContext context, void Function(String code))?
  scannerBuilder;

  @override
  Widget build(BuildContext context) {
    final delivered = <String>{};
    void deliver(String code) {
      final trimmed = code.trim();
      if (trimmed.isEmpty || !delivered.add(trimmed)) return;
      onDetected(trimmed);
    }

    return Scaffold(
      appBar: AppBar(title: const Text('扫描邀请二维码')),
      body:
          scannerBuilder?.call(context, deliver) ??
          MobileScanner(
            onDetect: (capture) {
              for (final barcode in capture.barcodes) {
                final value = barcode.rawValue;
                if (value != null && value.isNotEmpty) {
                  deliver(value);
                  return;
                }
              }
            },
            // 不预判平台：相机打不开（无设备/无权限/无实现）在此可见失败。
            errorBuilder: (context, error) => Center(
              child: Padding(
                padding: const EdgeInsets.all(24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    const Icon(Icons.no_photography_outlined),
                    const SizedBox(height: 12),
                    const Text('无法访问相机，可用复制/粘贴邀请码代替。'),
                    const SizedBox(height: 8),
                    Text(
                      '$error',
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
            ),
          ),
    );
  }
}
