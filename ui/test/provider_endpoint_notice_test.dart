import 'dart:io';

import 'package:elsewhen_ui/widgets/provider_endpoint_notice.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('loopback includes reserved names and IPv4-mapped IPv6 without DNS', () {
    HttpOverrides.runZoned(() {
      for (final url in [
        'http://localhost:11434/v1',
        'http://LOCALHOST.:11434/v1',
        'https://model.localhost/v1',
        'http://127.0.0.1:8080',
        'http://127.10.20.30',
        'http://[::1]:8080/v1',
        'http://[0:0:0:0:0:0:0:1]',
        'http://[::ffff:127.0.0.1]',
      ]) {
        expect(providerEndpointNotice(url), contains('环回'), reason: url);
      }
      for (final url in [
        'https://localhost.example.invalid',
        'https://example.invalid/localhost',
        'https://example.invalid?host=127.0.0.1',
        'https://127.0.0.1.example.invalid',
        'https://model.invalid/v1',
      ]) {
        expect(providerEndpointNotice(url), isNull, reason: url);
      }
    }, createHttpClient: (_) => throw StateError('must not probe'));
  });

  test('private and link-local ranges use exact boundaries', () {
    for (final url in [
      'http://10.0.0.1',
      'http://172.16.0.1',
      'http://172.31.255.254',
      'http://192.168.1.1',
      'http://169.254.1.2',
      'http://[fc00::1]',
      'http://[fdff::1]',
      'http://[fe80::1]',
      'http://[febf::1]',
      'http://[::ffff:192.168.1.1]',
    ]) {
      expect(providerEndpointNotice(url), contains('内网或链路本地'), reason: url);
    }
    for (final url in [
      'https://172.15.255.254',
      'https://172.32.0.1',
      'https://192.169.1.1',
      'https://[2001:db8::1]',
    ]) {
      expect(providerEndpointNotice(url), isNull, reason: url);
    }
  });

  test('unspecified and malformed URLs get advice without echoing secrets', () {
    for (final url in ['http://0.0.0.0:8080', 'http://[::]:8080']) {
      expect(providerEndpointNotice(url), contains('未指定地址'));
    }
    for (final url in [
      '',
      'not-a-url',
      'file:///SYNTHETIC_SECRET',
      'http://[',
    ]) {
      final notice = providerEndpointNotice(url);
      expect(notice, contains('格式需要检查'));
      expect(notice, isNot(contains('SYNTHETIC_SECRET')));
    }
  });

  testWidgets(
    'notice wraps on a narrow screen and offers no automatic action',
    (tester) async {
      tester.view.physicalSize = const Size(320, 640);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      const url = 'http://localhost:11434/v1';
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: Padding(
              padding: EdgeInsets.all(16),
              child: ProviderEndpointNotice(baseUrl: url),
            ),
          ),
        ),
      );
      expect(find.textContaining('保留原地址'), findsOneWidget);
      expect(find.textContaining('不会自动探测'), findsOneWidget);
      expect(find.byType(TextButton), findsNothing);
      expect(find.byType(FilledButton), findsNothing);
      expect(
        tester
            .widget<ProviderEndpointNotice>(find.byType(ProviderEndpointNotice))
            .baseUrl,
        url,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(
        const MaterialApp(
          home: ProviderEndpointNotice(baseUrl: 'https://model.invalid/v1'),
        ),
      );
      expect(find.byIcon(Icons.info_outline), findsNothing);
    },
  );
}
