import 'dart:io';

import 'package:flutter/material.dart';

import '../theme/app_theme.dart';
import '../l10n/localization.dart';

/// Describes address scope only: never resolves DNS, probes a provider or rewrites
/// its URL. Absence of a notice is not a claim that a remote address is reachable.
String? providerEndpointNotice(String baseUrl, {AppLocalizations? strings}) {
  final text = strings ?? lookupAppLocalizations(const Locale('zh'));
  final uri = Uri.tryParse(baseUrl);
  if (uri == null ||
      !['http', 'https'].contains(uri.scheme) ||
      uri.host.isEmpty) {
    return text.endpointMalformed;
  }
  final host = uri.host.toLowerCase().replaceFirst(RegExp(r'\.$'), '');
  final loopback = text.endpointLoopback;
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
    return text.endpointUnspecified;
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
    return text.endpointPrivate;
  }
  return null;
}

class ProviderEndpointNotice extends StatelessWidget {
  const ProviderEndpointNotice({super.key, required this.baseUrl});

  final String baseUrl;

  @override
  Widget build(BuildContext context) {
    final notice = providerEndpointNotice(baseUrl, strings: context.l10n);
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
