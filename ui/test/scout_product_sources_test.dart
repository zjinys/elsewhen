import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:elsewhen_ui/providers/scout_provider.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/left_sidebar.dart';
import 'package:elsewhen_ui/widgets/scout_view.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'scout_research_workspace_test.dart' show FakeResearch, pump, capture;

const _sourceA = 'github-rust';
const _sourceB = 'appstore-us-top-free';
const _sourceC = 'producthunt-daily';

void main() {
  testWidgets(
    'current discovery sources do not resurrect retired mirror scopes',
    (tester) async {
      await pump(
        tester,
        FakeResearch(),
        sources: const [
          api.ScoutSourceDto(
            id: 'github-newcomers',
            name: 'GitHub recent projects',
            enabled: true,
            intervalMinutes: 720,
          ),
          api.ScoutSourceDto(
            id: 'cws-discover',
            name: 'Chrome extension discovery',
            enabled: true,
            intervalMinutes: 720,
          ),
        ],
        home: ProviderScope(
          overrides: [
            scoutSourceSummariesProvider.overrideWith(
              (ref) async => const [
                ScoutSourceSummary(
                  sourceId: 'github-rust',
                  unread: 5,
                  newCount: 5,
                  changedCount: 0,
                  goneCount: 0,
                  total: 5,
                  latestDetectedAt: null,
                ),
              ],
            ),
          ],
          child: const Scaffold(body: ScoutSourceNavigation()),
        ),
      );
      expect(find.text('近 90 天新项目'), findsOneWidget);
      expect(find.text('扩展发现'), findsOneWidget);
      expect(find.text('Rust'), findsNothing);
      expect(find.text('扩展关注清单'), findsNothing);
    },
  );

  test('multi-source paging stays globally ordered and deduplicates shared products', () async {
    final repo = _SourcesRepository();
    final ids = <String>[];
    String? cursor;
    String? asOf;
    do {
      final page = await repo.productsForSources(
        'needle',
        {_sourceB, _sourceA},
        cursor: cursor,
        asOf: asOf,
        view: 'growth',
      );
      ids.addAll(page.items.map((p) => p.id));
      expect(page.items.length, lessThanOrEqualTo(40));
      asOf ??= page.asOf;
      expect(page.asOf, asOf);
      cursor = page.next;
    } while (cursor != null);
    expect(ids, List.generate(90, (i) => _id(89 - i)));
    expect(ids.toSet().length, 90);
    expect(repo.requests.length, 6);
    expect(
      repo.requests.every((q) => q.query == 'needle' && q.view == 'growth'),
      isTrue,
    );
    expect(repo.requests.skip(1).every((q) => q.asOf == asOf), isTrue);
    expect(
      repo.requests.every((q) => q.source == _sourceA || q.source == _sourceB),
      isTrue,
    );
  });

  test(
    'zero/single selection uses one query and stale metadata propagates',
    () async {
      final repo = _SourcesRepository();
      await repo.productsForSources('', {});
      expect(repo.requests.single.source, isNull);
      repo.requests.clear();
      await repo.productsForSources('', {_sourceA});
      expect(repo.requests.single.source, _sourceA);
      repo.stale = true;
      expect(
        (await repo.productsForSources('', {_sourceA, _sourceB})).stale,
        isTrue,
      );
    },
  );

  test('different Scout instances are never combined', () async {
    final repo = _SourcesRepository()..mixedInstances = true;
    await expectLater(
      repo.productsForSources('', {_sourceA, _sourceB}),
      throwsStateError,
    );
  });

  for (final width in [1200.0, 360.0]) {
    testWidgets(
      'sidebar source panel combines, deselects and clears with compact cards ($width)',
      (tester) async {
        final repo = _SourcesRepository()..small = true;
        await pump(
          tester,
          repo,
          width: width,
          sources: const [
            api.ScoutSourceDto(
              id: _sourceA,
              name: 'Rust',
              enabled: true,
              intervalMinutes: 60,
            ),
            api.ScoutSourceDto(
              id: _sourceB,
              name: 'App Store',
              enabled: true,
              intervalMinutes: 60,
            ),
            api.ScoutSourceDto(
              id: _sourceC,
              name: 'Product Hunt',
              enabled: true,
              intervalMinutes: 60,
            ),
          ],
          polledAt: DateTime.now().subtract(const Duration(minutes: 5)),
          brightness: Brightness.light,
          // 来源筛选在左侧导航的情报上下文面板：模态抽屉挂载，与窄屏
          // 实际使用方式一致（停靠形态由桌面端主屏提供，面板组件相同）。
          home: const Scaffold(drawer: LeftSidebar(), body: ScoutView()),
        );
        // 切到情报入口：侧栏出现来源面板。
        ProviderScope.containerOf(tester.element(find.byType(ScoutView)))
            .read(sidebarTabProvider.notifier)
            .set(SidebarTab.scout);
        await tester.pumpAndSettle();
        tester.state<ScaffoldState>(find.byType(Scaffold)).openDrawer();
        await tester.pumpAndSettle();
        expect(find.text('上次拉取：5 分钟前'), findsOneWidget);
        final card = find.byType(Card).first;
        expect(tester.getSize(card).height, lessThan(320));

        // 切到情报入口后，左侧导航出现来源筛选面板；直接点面板行。
        Future<void> tapSource(String id) async {
          // 点选后面板会收起模态抽屉（停靠态无感知），再次点选先重新打开。
          final scaffold = tester.state<ScaffoldState>(find.byType(Scaffold));
          if (!scaffold.isDrawerOpen) {
            scaffold.openDrawer();
            await tester.pumpAndSettle();
          }
          await tester.tap(find.byKey(ValueKey('scout-source-$id')));
          await tester.pumpAndSettle();
        }

        await tapSource(_sourceA);
        expect(find.text('Product $_sourceA'), findsOneWidget);
        expect(find.text('Product $_sourceB'), findsNothing);
        await tapSource(_sourceB);
        final container = ProviderScope.containerOf(
          tester.element(find.byType(ScoutView)),
        );
        expect(container.read(scoutProductSourcesProvider), {
          _sourceA,
          _sourceB,
        });
        expect(find.text('Product $_sourceA'), findsOneWidget);
        expect(
          container
              .read(scoutProductPageProvider)
              .value!
              .items
              .map((p) => p.id)
              .toSet(),
          {_sourceA, _sourceB},
        );
        expect(find.text('Product $_sourceC'), findsNothing);
        await tapSource(_sourceA);
        expect(container.read(scoutProductSourcesProvider), {_sourceB});
        expect(find.text('Product $_sourceA'), findsNothing);
        await tapSource('all');
        expect(container.read(scoutProductSourcesProvider), isEmpty);
        expect(container.read(scoutProductPageProvider).value!.items.length, 3);
        if (Platform.environment['SCOUT_PREVIEW'] == '1') {
          await capture(
            tester,
            '/tmp/scout-discovery-updated-${width.toInt()}.png',
          );
        }
        expect(tester.takeException(), isNull);
      },
    );
  }
}

String _id(int number) => 'id-${number.toString().padLeft(3, '0')}';

class _SourcesRepository extends FakeResearch {
  final requests =
      <
        ({
          String query,
          String? source,
          String? cursor,
          String? asOf,
          String view,
        })
      >[];
  bool small = false;
  bool stale = false;
  bool mixedInstances = false;

  @override
  Future<ScoutProductPage> products(
    String query,
    String? source, {
    String? cursor,
    String? asOf,
    String view = 'signals',
  }) async {
    requests.add((
      query: query,
      source: source,
      cursor: cursor,
      asOf: asOf,
      view: view,
    ));
    final rows = small
        ? [
            for (final id in [_sourceA, _sourceB, _sourceC])
              {
                'product_id': id,
                'title': 'Product $id',
                'observed_at': '2026-11-19T10:00:00Z',
                'sources': [
                  {'source_id': id},
                ],
                'reasons': [
                  {'kind': 'new_scope', 'to_at': '2026-11-19'},
                ],
              },
          ]
        : List.generate(
            90,
            (i) => <String, dynamic>{
              'product_id': _id(i),
              'observed_at': DateTime.utc(
                2026,
                11,
                19,
                10,
                0,
                i,
              ).toIso8601String(),
              'sources': [
                if (i.isEven || i % 7 == 0) {'source_id': _sourceA},
                if (i.isOdd || i % 7 == 0) {'source_id': _sourceB},
              ],
            },
          );
    final before = cursor == null ? null : jsonDecode(cursor) as List;
    final matched =
        rows.where((row) {
          if (source != null &&
              !(row['sources'] as List).any((s) => s['source_id'] == source)) {
            return false;
          }
          final time = row['observed_at'] as String;
          return before == null ||
              time.compareTo(before[0] as String) < 0 ||
              (time == before[0] &&
                  (row['product_id'] as String).compareTo(before[1] as String) <
                      0);
        }).toList()..sort((a, b) {
          final cmp = (b['observed_at'] as String).compareTo(
            a['observed_at'] as String,
          );
          return cmp != 0
              ? cmp
              : (b['product_id'] as String).compareTo(
                  a['product_id'] as String,
                );
        });
    final page = matched.take(40).toList();
    return ScoutProductPage({
      'instance_id': mixedInstances && source == _sourceB ? 'other' : 'test',
      'as_of': asOf ?? '2026-11-20T00:00:00Z',
      'items': page,
      'next': page.length == 40
          ? jsonEncode([page.last['observed_at'], page.last['product_id']])
          : null,
      'stale': stale && source == _sourceB,
    });
  }
}
