import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/services.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/models/settings.dart';

import 'package:flutter/rendering.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/scout_provider.dart';
import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:elsewhen_ui/widgets/scout_view.dart';

import 'support/localizations.dart';

import 'package:elsewhen_ui/providers/state_holder.dart';

ScoutFindingItem item(int id, String source, {String kind = 'changed'}) =>
    ScoutFindingItem(
      id: id,
      sourceId: source,
      kind: kind,
      itemKey: 'typst/typst',
      payload: {
        'full_name': id == 1 ? 'typst/typst' : 'Local research tools',
        'description': 'A typesetting system.',
        'stargazers_count': 57225,
        'license_spdx_id': 'Apache-2.0',
        'topics': ['compiler', 'markup'],
      },
      delta: kind == 'changed'
          ? {
              'stargazers_count': {'from': 57191, 'to': 57225, 'diff': 34},
            }
          : {},
      detectedAt: '2026-11-17T10:00:00Z',
      readAt: null,
    );

class FakeScout extends ScoutRepository {
  FakeScout({this.many = false});
  final bool many;
  final marks = <List<int>>[];
  final queries = <({String? source, String? kind, bool unread})>[];
  @override
  Future<List<ScoutFindingItem>> findings({
    String? source,
    String? kind,
    bool unread = false,
    int offset = 0,
    int limit = 40,
  }) async {
    queries.add((source: source, kind: kind, unread: unread));
    return [
          item(1, 'github-rust'),
          item(2, 'product-hunt', kind: 'new'),
          if (many)
            ...List.generate(
              38,
              (i) => item(
                i + 3,
                [
                  'github-rust',
                  'product-hunt',
                  'gumroad-notion',
                  'cws-watchlist',
                  'appstore-us-top-free',
                ][i % 5],
                kind: i.isEven ? 'new' : 'changed',
              ),
            ),
        ]
        .where(
          (i) =>
              (source == null || i.sourceId == source) &&
              (kind == null || i.kind == kind),
        )
        .toList();
  }

  @override
  Future<void> markRead(WidgetRef ref, List<int> ids) async {
    marks.add(ids);
  }

  @override
  Future<Map<String, dynamic>> detail(int id) async => {
    'history': [
      {
        'taken_at': '2026-11-16T10:00:00Z',
        'item': {'stargazers_count': 57191},
      },
    ],
  };
}

Future<void> pumpScout(
  WidgetTester tester,
  FakeScout repo, {
  double width = 1400,
}) async {
  tester.view.physicalSize = Size(width, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  if (Platform.environment['SCOUT_PREVIEW'] == '1') {
    await tester.runAsync(() async {
      final bytes = await File(
        '/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc',
      ).readAsBytes();
      for (final family in ['Noto Sans SC', 'Ahem', 'Inter', 'Roboto']) {
        await (FontLoader(
          family,
        )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
      }
      final icons = await File(
        '/home/pp/fvm/versions/3.50.0/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf',
      ).readAsBytes();
      await (FontLoader(
        'MaterialIcons',
      )..addFont(Future.value(ByteData.sublistView(icons)))).load();
    });
  }
  AppTheme.apply(Brightness.dark, accent: const Color(0xFFE9A568));
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        scoutRepositoryProvider.overrideWithValue(repo),
        scoutWorkspaceProvider.overrideWith(
          () => StateHolder(ScoutWorkspace.activity),
        ),
        scoutRevisionProvider.overrideWithValue(0),
        scoutUnreadProvider.overrideWith((ref) async => 2),
        scoutSourcesProvider.overrideWith(
          (ref) async => [
            const api.ScoutSourceDto(
              id: 'github-rust',
              name: 'GitHub · Rust',
              intervalMinutes: 60,
              enabled: true,
            ),
            const api.ScoutSourceDto(
              id: 'product-hunt',
              name: 'Product Hunt',
              intervalMinutes: 60,
              enabled: true,
            ),
          ],
        ),
        scoutSourceSummariesProvider.overrideWith((ref) async => []),
        scoutSettingsProvider.overrideWith(
          (ref) async => const api.ScoutSettingsDto(
            baseUrl: 'https://scout.example.com',
            pollIntervalMinutes: 30,
            enabled: true,
            hasToken: true,
          ),
        ),
      ],
      child: RepaintBoundary(
        key: const ValueKey('preview'),
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
          locale: const Locale('zh'),
          localizationsDelegates: appTestDelegates(),
          supportedLocales: const [Locale('zh'), Locale('en')],
          home: Scaffold(
            body: RepaintBoundary(
              key: const ValueKey('workspace'),
              child: Material(
                color: AppTheme.surface0,
                child: width >= 1400
                    ? const Row(
                        children: [
                          SizedBox(width: 240, child: ScoutSourceNavigation()),
                          Expanded(child: ScoutView()),
                        ],
                      )
                    : const ScoutView(),
              ),
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('阅读在站内展开；指标在详情，列表突出变化', (tester) async {
    final repo = FakeScout();
    await pumpScout(tester, repo);
    expect(find.textContaining('Star 57191 → 57225'), findsOneWidget);
    expect(find.textContaining('许可 Apache-2.0'), findsNothing);
    await tester.tap(find.text('typst/typst'));
    await tester.pumpAndSettle();
    expect(repo.marks, [
      [1],
    ]);
    expect(find.text('本次变化'), findsOneWidget);
    expect(find.text('57191 → 57225'), findsOneWidget);
    expect(find.text('许可 Apache-2.0'), findsOneWidget);
    expect(find.text('存为素材'), findsOneWidget);
    expect(tester.takeException(), isNull);
    expect(find.byKey(const ValueKey('scout-detail-sheet')), findsOneWidget);
    final sheet = tester.getRect(
      find.byKey(const ValueKey('scout-detail-sheet')),
    );
    expect(sheet.right, 1400);
    expect(sheet.width, 620);
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('scout-detail-sheet')), findsNothing);
  });
  testWidgets('未读与变化类型组合，批量已读限定当前来源', (tester) async {
    final repo = FakeScout();
    await pumpScout(tester, repo);
    await tester.tap(find.text('GitHub').first);
    ProviderScope.containerOf(tester.element(find.byType(ScoutView)))
        .read(scoutWorkspaceProvider.notifier)
        .set(ScoutWorkspace.activity);
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(FilterChip, '未读'));
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ChoiceChip, '有变化'));
    await tester.pumpAndSettle();
    expect(repo.queries.last, (
      source: 'github-rust',
      kind: 'changed',
      unread: true,
    ));
    await tester.tap(find.text('本页标为已读'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('确认'));
    await tester.pumpAndSettle();
    expect(repo.marks, [
      [1],
    ]);
    expect(tester.takeException(), isNull);
  });
  testWidgets('窄屏能阅读详情并返回列表', (tester) async {
    final repo = FakeScout();
    await pumpScout(tester, repo, width: 360);
    await tester.tap(find.text('typst/typst'));
    await tester.pumpAndSettle();
    expect(find.text('本次变化'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    expect(find.text('最新动态'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets('大量动态按多列卡片展示，抽屉关闭保留滚动位置', (tester) async {
    final repo = FakeScout(many: true);
    await pumpScout(tester, repo);
    final first = tester.getRect(find.byKey(const ValueKey('scout-card-1')));
    final second = tester.getRect(find.byKey(const ValueKey('scout-card-2')));
    expect(first.top, second.top);
    expect(second.left, greaterThan(first.right));
    expect(find.byType(ScoutSourceBadge), findsWidgets);
    if (Platform.environment['SCOUT_PREVIEW'] == '1') {
      await screenshot(tester, '/tmp/scout-cards.png');
    }
    await tester.drag(find.byType(CustomScrollView), const Offset(0, -520));
    await tester.pumpAndSettle();
    final scroll = tester
        .state<ScrollableState>(
          find
              .descendant(
                of: find.byType(CustomScrollView),
                matching: find.byType(Scrollable),
              )
              .first,
        )
        .position;
    final before = scroll.pixels;
    final card = find.byType(Card).hitTestable().first;
    await tester.tap(card);
    await tester.pumpAndSettle();
    if (Platform.environment['SCOUT_PREVIEW'] == '1') {
      await screenshot(tester, '/tmp/scout-sidesheet.png');
    }
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    expect(scroll.pixels, before);
    await tester.tap(find.byType(Card).hitTestable().first);
    await tester.pumpAndSettle();
    await tester.tapAt(const Offset(300, 300));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('scout-detail-sheet')), findsNothing);
    await tester.tap(find.byType(Card).hitTestable().first);
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('scout-detail-sheet')), findsNothing);
    expect(tester.takeException(), isNull);
  });
}

Future<void> screenshot(WidgetTester tester, String path) async {
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byKey(const ValueKey('preview')),
  );
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1.5);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    await File(path).writeAsBytes(bytes!.buffer.asUint8List());
    image.dispose();
  });
}
