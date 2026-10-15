import 'dart:io';

import 'package:flutter/material.dart';

import '../theme/app_theme.dart';

/// Describes address scope only: never resolves DNS, probes a provider or rewrites
/// its URL. Absence of a notice is not a claim that a remote address is reachable.
String? providerEndpointNotice(String baseUrl) {
  final uri = Uri.tryParse(baseUrl);
  if (uri == null ||
      !['http', 'https'].contains(uri.scheme) ||
      uri.host.isEmpty) {
    return 'Provider 地址格式需要检查；不会自动修正地址或发送连接测试。';
  }
  final host = uri.host.toLowerCase().replaceFirst(RegExp(r'\.$'), '');
  const loopback =
      '这是本机环回地址：在另一台设备上会指向那台设备本身，而非原设备。同步保留原地址，不会自动探测或改用其他 Provider。';
  if (host == 'localhost' || host.endsWith('.localhost')) return loopback;
  final address = InternetAddress.tryParse(host);
  if (address == null) return null;
  if (address.isLoopback) return loopback;
  var bytes = address.rawAddress;
  // IPv4-mapped IPv6 literals retain the embedded address's local scope.
  if (bytes.length == 16 &&
      bytes.take(10).every((b) => b == 0) &&
      bytes[10] == 255 &&
      bytes[11] == 255) {
    bytes = bytes.sublist(12);
  }
  if (bytes.every((b) => b == 0)) {
    return '这是未指定地址，通常用于服务监听；请确认本机及其他设备实际可访问的 Provider 地址。不会自动替换或探测。';
  }
  if (bytes.length == 4 && bytes[0] == 127) return loopback;
  final privateAddress = bytes.length == 4
      ? bytes[0] == 10 ||
            (bytes[0] == 172 && bytes[1] >= 16 && bytes[1] <= 31) ||
            (bytes[0] == 192 && bytes[1] == 168) ||
            (bytes[0] == 169 && bytes[1] == 254)
      : (bytes[0] & 0xfe) == 0xfc ||
            (bytes[0] == 0xfe && (bytes[1] & 0xc0) == 0x80);
  if (privateAddress) {
    return '这是内网或链路本地地址，其他设备需要相应的网络连接。同步保留原配置，尚未验证可达性，也不会自动发送模型请求。';
  }
  return null;
}

class ProviderEndpointNotice extends StatelessWidget {
  const ProviderEndpointNotice({super.key, required this.baseUrl});

  final String baseUrl;

  @override
  Widget build(BuildContext context) {
    final notice = providerEndpointNotice(baseUrl);
    if (notice == null) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: AppTheme.space2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Icon(Icons.info_outline, size: 14, color: AppTheme.textSecondary),
          const SizedBox(width: AppTheme.space1),
          Expanded(
            child: Text(
              notice,
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 11,
                height: 1.4,
              ),
            ),
          ),
        ],
      ),
    );
  }
}
