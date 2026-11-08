import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// 洞见浮现卡只做展示与状态判断；动作落库由 Rust 侧存储测试覆盖
/// （见 src/storage/tests/digest_insights.rs），这里覆盖 provider 不碰 FFI。
void main() {
  for (final locale in [const Locale('zh'), const Locale('en')]) {
    final strings = lookupAppLocalizations(locale);
    group(locale.languageCode, () {
      testWidgets('活跃洞见卡展示标题/观察/建议/来源与四重出口', (tester) async {
        await tester.pumpWidget(
          _wrap(locale: locale, message: _card(), surfacings: [_surfacing()]),
        );
        await tester.pumpAndSettle();

        expect(find.text('午饭后散步已固化'), findsOneWidget);
        expect(find.text('连续 12 天午饭后散步。'), findsOneWidget);
        expect(
          find.text(strings.insightSuggestion('散步后直接做 5 分钟拉伸')),
          findsOneWidget,
        );
        expect(find.text(strings.insightSource('habit/walk')), findsOneWidget);
        expect(find.text(strings.adoptAsRule), findsOneWidget);
        expect(find.text(strings.enableCheckins), findsOneWidget);
        expect(find.text(strings.makeTodo), findsOneWidget);
        expect(find.text(strings.acknowledge), findsOneWidget);
      });

      testWidgets('非习惯来源页不提供「开启习惯问候」', (tester) async {
        await tester.pumpWidget(
          _wrap(
            locale: locale,
            message: _card(),
            surfacings: [_surfacing(pageKind: 'insight')],
          ),
        );
        await tester.pumpAndSettle();

        expect(find.text(strings.enableCheckins), findsNothing);
        expect(find.text(strings.adoptAsRule), findsOneWidget);
        expect(find.text(strings.makeTodo), findsOneWidget);
        expect(find.text(strings.acknowledge), findsOneWidget);
      });

      testWidgets('已采纳为规则的卡片收起按钮并显示状态', (tester) async {
        await tester.pumpWidget(
          _wrap(
            locale: locale,
            message: _card(),
            surfacings: [_surfacing(status: 'acted', actionType: 'rule')],
          ),
        );
        await tester.pumpAndSettle();

        expect(find.text(strings.insightRuleApplied), findsOneWidget);
        expect(find.text(strings.adoptAsRule), findsNothing);
        expect(find.text(strings.acknowledge), findsNothing);
      });

      testWidgets('已了解的卡片只作只读展示', (tester) async {
        await tester.pumpWidget(
          _wrap(
            locale: locale,
            message: _card(),
            surfacings: [
              _surfacing(status: 'dismissed', actionType: 'dismiss'),
            ],
          ),
        );
        await tester.pumpAndSettle();
        expect(find.text(strings.insightDismissed), findsOneWidget);
        expect(find.text(strings.adoptAsRule), findsNothing);
        expect(find.text(strings.enableCheckins), findsNothing);
        expect(find.text(strings.makeTodo), findsNothing);
        expect(find.text(strings.acknowledge), findsNothing);
      });

      testWidgets('已过期的卡片只作只读展示', (tester) async {
        await tester.pumpWidget(
          _wrap(
            locale: locale,
            message: _card(),
            surfacings: [_surfacing(status: 'expired')],
          ),
        );
        await tester.pumpAndSettle();
        expect(find.text(strings.insightExpired), findsOneWidget);
        expect(find.text(strings.adoptAsRule), findsNothing);
        expect(find.text(strings.enableCheckins), findsNothing);
        expect(find.text(strings.makeTodo), findsNothing);
        expect(find.text(strings.acknowledge), findsNothing);
      });

      testWidgets('副设备无本机台账时按卡片正文兜底渲染并可动作', (tester) async {
        await tester.pumpWidget(
          _wrap(locale: locale, message: _card(), surfacings: const []),
        );
        await tester.pumpAndSettle();

        // 正文解析出标题/观察/建议/来源；习惯出口需要台账页信息，不显示。
        expect(find.text('午饭后散步已固化'), findsOneWidget);
        expect(find.text('连续 12 天午饭后散步。'), findsOneWidget);
        expect(
          find.text(strings.insightSuggestion('散步后直接做 5 分钟拉伸')),
          findsOneWidget,
        );
        expect(find.text(strings.insightSource('habit/walk')), findsOneWidget);
        expect(find.text(strings.enableCheckins), findsNothing);
        expect(find.text(strings.adoptAsRule), findsOneWidget);
        expect(find.text(strings.makeTodo), findsOneWidget);
        expect(find.text(strings.acknowledge), findsOneWidget);
      });
    });
  }
}

Widget _wrap({
  required Locale locale,
  required Message message,
  required List<api.DigestInsightSurfacingDto> surfacings,
}) {
  return ProviderScope(
    overrides: [
      digestInsightSurfacingsProvider.overrideWith((ref) async => surfacings),
    ],
    child: MaterialApp(
      locale: locale,
      supportedLocales: AppLocalizations.supportedLocales,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      home: Scaffold(body: DigestInsightBubble(message: message)),
    ),
  );
}

Message _card({String id = 'm1', String? ref = 'surfacing:s1'}) => Message(
  id: id,
  conversationId: 'conv-1',
  role: MessageRole.assistant,
  content:
      '💡 认知洞察｜午饭后散步已固化\n\n连续 12 天午饭后散步。'
      '\n\n建议行动：散步后直接做 5 分钟拉伸\n\n来源：[[habit/walk]]',
  createdAt: DateTime.now(),
  kind: 'digest_insight',
  messageRef: ref,
);

api.DigestInsightSurfacingDto _surfacing({
  String status = 'surfaced',
  String? actionType,
  String? pageKind = 'habit',
  bool pageActive = true,
}) => api.DigestInsightSurfacingDto(
  id: 's1',
  insightId: null,
  pageSlug: 'habit/walk',
  kind: 'habit',
  title: '午饭后散步已固化',
  observation: '连续 12 天午饭后散步。',
  actionSuggestion: '散步后直接做 5 分钟拉伸',
  status: status,
  surfacedAt: '2026-11-05T02:00:00Z',
  surfacedDate: '2026-11-05',
  messageId: 'm1',
  actionType: actionType,
  createdAt: '2026-11-05T01:59:00Z',
  updatedAt: '2026-11-05T02:00:00Z',
  pageKind: pageKind,
  pageActive: pageActive,
);
