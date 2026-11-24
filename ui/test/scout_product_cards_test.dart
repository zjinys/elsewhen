import 'dart:io';

import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'scout_research_workspace_test.dart' show FakeResearch, pump, capture;

const _intro =
    'Use Microsoft Authenticator for easy, secure sign-ins for all your online accounts using multi-factor authentication, passwordless, or password autofill.';
const _pitch = 'Ready to ship? One agent tests, translates and audits it';

void main() {
  for (final width in [1200.0, 360.0]) {
    testWidgets(
      'cards explain purpose, show facts and keep raw evidence in detail ($width)',
      (tester) async {
        await pump(
          tester,
          _CardsRepository(),
          width: width,
          brightness: Brightness.light,
        );
        expect(find.text('Microsoft Authenticator'), findsOneWidget);
        expect(find.text('App Store'), findsNothing);
        expect(find.byTooltip('App Store · 美国免费榜'), findsOneWidget);
        expect(find.text(_intro), findsOneWidget);
        expect(
          find.text('Productivity · Microsoft Corporation'),
          findsOneWidget,
        );
        expect(find.text('#1'), findsOneWidget);
        expect(find.textContaining('4.7/5'), findsOneWidget);
        expect(find.text('免费下载'), findsOneWidget);
        expect(find.text('收费信息待核实'), findsNothing);
        expect(find.textContaining('目标买方待核实'), findsNothing);
        expect(find.text('2026-11-20 首次进入监控'), findsWidgets);
        if (width > 1000) {
          expect(find.text('Affiliate · 可申请推广返佣'), findsOneWidget);
          expect(find.text(_pitch), findsOneWidget);
          expect(find.textContaining('&lt;'), findsNothing);
          expect(find.textContaining('utm_medium='), findsNothing);
          expect(find.text('来源未提供用途简介，可查看原始介绍。'), findsOneWidget);
          expect(find.text('查看原始介绍'), findsOneWidget);
        }
        if (Platform.environment['SCOUT_PREVIEW'] == '1') {
          await capture(tester, '/tmp/scout-product-info-${width.toInt()}.png');
        }
        await tester.tap(find.text('Microsoft Authenticator'));
        await tester.pumpAndSettle();
        expect(find.text('标价（不代表成交）: USD 0.00'), findsOneWidget);
        expect(find.text('目标买方待核实 · 销量未知 · 尚未分析评价正文'), findsOneWidget);
        expect(tester.takeException(), isNull);
      },
    );
  }
}

class _CardsRepository extends FakeResearch {
  @override
  Future<ScoutProductPage> products(
    String query,
    String? source, {
    String? cursor,
    String? asOf,
    String view = 'signals',
  }) async => ScoutProductPage({
    'instance_id': 'test',
    'as_of': '2026-11-20T10:00:00Z',
    'items':
        [
              {
                'product_id': 'authenticator',
                'title': 'Microsoft Authenticator',
                'sources': [
                  {'source_id': 'appstore-us-top-free'},
                ],
                'payload': {
                  'description': _intro,
                  'genres_0_name': 'Productivity',
                  'artistName': 'Microsoft Corporation',
                  'rating': 4.7,
                  'rating_count': 728903,
                  'rank': 1,
                  'price_cents': 0,
                  'currency_code': 'USD',
                },
              },
              {
                'product_id': 'kadeep',
                'affiliate': {
                  'status': 'confirmed',
                  'reward_type': 'cash',
                  'stale': false,
                  'checked_at': '2026-11-20T10:00:00Z',
                  'program_url': 'https://example.com/affiliate',
                  'evidence_quote':
                      r'Join and earn 20% commission, capped at $150.',
                },
                'title': 'Affiliate demo · 合成示例',
                'sources': [
                  {'source_id': 'producthunt-daily'},
                ],
                'payload': {
                  'summary':
                      '&lt;p&gt;$_pitch&lt;/p&gt;&lt;p&gt;&lt;a href="https://www.producthunt.com/r?utm_medium=rss"&gt;Discussion&lt;/a&gt; | &lt;a href="/out"&gt;Link&lt;/a&gt;&lt;/p&gt;',
                },
              },
              {
                'product_id': 'unknown',
                'title': 'New product',
                'sources': [
                  {'source_id': 'cws-watchlist'},
                ],
                'payload': {'url': 'https://example.com/product'},
              },
            ]
            .map(
              (item) => {
                ...item,
                'reasons': [
                  {'kind': 'new_scope', 'to_at': '2026-11-20'},
                ],
              },
            )
            .toList(),
  });
}
