import '../l10n/localization.dart';

import 'package:material_ui/material_ui.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

/// 扫码读取已有设备出示的邀请二维码。只读取邀请码文本本身，画面不存储。
///
/// [scannerBuilder] 是测试注入缝：默认用相机（mobile_scanner）；测试里注入
/// 可模拟检测的替换件，避免在 CI 里真的打开相机。
class JoinerQrScanPage extends StatefulWidget {
  const JoinerQrScanPage({
    super.key,
    required this.onDetected,
    this.scannerBuilder,
    this.onPaste,
  });

  /// 读到二维码后回调；重复检测会被忽略（本页只 pop 一次）。
  final void Function(String code) onDetected;

  /// 或：注入假的取景器；签名给出本页应调用的 code 回调。
  final Widget Function(BuildContext context, void Function(String code))?
  scannerBuilder;

  final VoidCallback? onPaste;

  @override
  State<JoinerQrScanPage> createState() => _JoinerQrScanPageState();
}

class _JoinerQrScanPageState extends State<JoinerQrScanPage> {
  bool _delivered = false;

  void _deliver(String code) {
    final trimmed = code.trim();
    if (!mounted || trimmed.isEmpty || _delivered) return;
    _delivered = true;
    widget.onDetected(trimmed);
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(context.l10n.l6ScanInvitationQrCode4882e),
        actions: [
          if (widget.onPaste != null)
            TextButton(
              onPressed: widget.onPaste,
              child: Text(context.l10n.pairingPaste),
            ),
        ],
      ),
      body:
          widget.scannerBuilder?.call(context, _deliver) ??
          MobileScanner(
            onDetect: (capture) {
              for (final barcode in capture.barcodes) {
                final value = barcode.rawValue;
                if (value != null && value.isNotEmpty) {
                  _deliver(value);
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
                    Text(context.l10n.l6CameraAccessIsUnavailableCopyAnd95be5),
                    const SizedBox(height: 8),
                    if (widget.onPaste != null)
                      FilledButton.icon(
                        onPressed: widget.onPaste,
                        icon: const Icon(Icons.content_paste),
                        label: Text(context.l10n.pairingPaste),
                      ),
                  ],
                ),
              ),
            ),
          ),
    );
  }
}
