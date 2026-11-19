import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';

import 'dart:io';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/scout_provider.dart';
import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:elsewhen_ui/widgets/scout_view.dart';

import 'support/localizations.dart';

class FakeResearch extends ScoutResearchRepository {
  final saved = <Map<String, dynamic>>[];
  Map<String, dynamic> criteria = {};
  @override
  Future<Map<String, dynamic>> preferences() async => criteria;
  @override
  Future<Map<String, dynamic>> savePreferences(
    Map<String, dynamic> value,
    String? token,
  ) async => criteria = {...value, 'token': 'criteria-v1'};
  final localDrafts = <String, Map<String, dynamic>>{};
  @override
  Future<Map<String, dynamic>?> draft(String instance, String product) async =>
      localDrafts[jsonEncode([instance, product])];
  @override
  Future<List<Map<String, dynamic>>> drafts() async =>
      localDrafts.values.toList();
  @override
  Future<void> saveDraft(Map<String, dynamic> value) async {
    localDrafts[jsonEncode([value['instance_id'], value['product_id']])] = {
      ...value,
    };
  }

  @override
  Future<void> deleteDraft(String instance, String product) async {
    localDrafts.remove(jsonEncode([instance, product]));
  }

  bool quiet = false;
  int pages = 0;
  bool paging = false;
  @override
  Future<ScoutProductPage> products(
    String query,
    String? source, {
    String? cursor,
    String? asOf,
    String view = 'signals',
  }) async {
    pages++;
    final page = ScoutProductPage({
      'instance_id': 'test',
      'as_of': '2026-11-17T00:00:00Z',
      'items': [
        {
          'product_id': '["gumroad","one"]',
          'platform': 'gumroad',
          'title': 'Client organizer',
          'payload': {
            'name': 'Client organizer',
            'description': 'Organize client work',
            'price_cents': 2900,
            'currency_code': 'USD',
          },
          'sources': [
            {'source_id': 'gumroad-notion'},
          ],
          'reasons': [
            {
              'days': 7,
              'metric': 'ratings_count',
              'from': 4,
              'to': 18,
              'from_at': '2026-11-10',
              'to_at': '2026-11-17',
            },
          ],
          'trends': [
            {
              'days': 7,
              'metric': 'ratings_count',
              'from': 4,
              'to': 18,
              'from_at': '2026-11-10',
              'to_at': '2026-11-17',
              'samples': 15,
              'complete': true,
            },
          ],
        },
      ],
    });
    if (Platform.environment['SCOUT_PREVIEW'] == '1' && !paging && !quiet) {
      final first = page.items.single;
      for (var i = 1; i < 12; i++) {
        page.items.add(
          ScoutProduct({
            ...first.data,
            'product_id': 'preview-$i',
            'title': [
              'Team invoice workspace',
              'Freelancer delivery planner',
              'Sales call notes',
              'Design feedback board',
              'Client onboarding kit',
              'Remote team handbook',
              'Project handoff templates',
              'Creator operations kit',
              'Agency budget tracker',
              'Customer interview notes',
              'Weekly focus planner',
            ][i - 1],
            'payload': {
              ...first.payload,
              'description': [
                'Track invoices and payment follow-ups',
                'Plan client deliverables and timelines',
                'Organize buyer conversations',
              ][i % 3],
            },
            'sources': [
              {
                'source_id': [
                  'gumroad-notion',
                  'appstore-us-top-free',
                  'cws-watchlist',
                ][i % 3],
              },
            ],
          }),
        );
      }
    }
    if (paging && cursor == null) {
      return ScoutProductPage({
        'instance_id': 'test',
        'as_of': page.asOf,
        'next': 'page2',
        'items': List.generate(
          40,
          (i) => {
            ...page.items.single.data,
            'product_id': 'quiet-$i',
            'reasons': [],
          },
        ),
      });
    }
    if (quiet) {
      page.items.single.data['reasons'] = [];
    }
    return page;
  }

  @override
  Future<Map<String, dynamic>> analysis(String slug) async => {
    'status': 'waiting',
  };
  @override
  Future<Map<String, dynamic>> detail(String id, {int? before}) async => {
    'history': [],
  };
  @override
  Future<List<Map<String, dynamic>>> studies() async => saved;
  @override
  Future<Map<String, dynamic>> save(
    Map<String, dynamic> value,
    String? token,
  ) async {
    localDrafts.remove(jsonEncode([value['instance_id'], value['product_id']]));
    saved.add({...value, 'token': 'v1', 'slug': 'scout-research-one'});
    return saved.last;
  }
}

Future<void> pump(
  WidgetTester tester,
  FakeResearch repository, {
  double width = 1200,
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
        scoutResearchRepositoryProvider.overrideWithValue(repository),
        scoutRevisionProvider.overrideWithValue(0),
        scoutSettingsProvider.overrideWith(
          (ref) async => const api.ScoutSettingsDto(
            baseUrl: 'https://example.test',
            pollIntervalMinutes: 30,
            enabled: true,
            hasToken: true,
          ),
        ),
        scoutSourcesProvider.overrideWith((ref) async => []),
      ],
      child: RepaintBoundary(
        key: const ValueKey('research-preview'),
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: AppTheme.darkTheme,
          locale: const Locale('zh'),
          localizationsDelegates: appTestDelegates(),
          supportedLocales: const [Locale('zh'), Locale('en')],
          home: const Scaffold(body: ScoutView()),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('产品证据进入个人研究并保留观察版本', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    if (Platform.environment['SCOUT_PREVIEW'] == '1') {
      await capture(tester, '/tmp/scout-research-discover.png');
    }
    expect(find.text('Client organizer'), findsOneWidget);
    expect(find.textContaining('USD 29.00'), findsOneWidget);
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    if (Platform.environment['SCOUT_PREVIEW'] == '1') {
      await capture(tester, '/tmp/scout-research-detail.png');
    }
    expect(find.text('证据缺口'), findsOneWidget);
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, '关键未知 / 我要验证的问题'),
      'Will freelancers pay?',
    );
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.saved.single['question'], 'Will freelancers pay?');
    expect(repo.saved.single['evidence']['trends'].single['samples'], 15);
    expect(repo.saved.single['instance_id'], 'test');
    expect(tester.takeException(), isNull);
  });
  testWidgets('关闭研究草稿后重开仍保留输入，Esc 不丢失', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    if (Platform.environment['SCOUT_PREVIEW'] == '1') {
      await capture(tester, '/tmp/scout-research-editor.png');
    }
    final question = find.widgetWithText(TextField, '关键未知 / 我要验证的问题');
    await tester.enterText(question, 'Draft question');
    await tester.tap(find.text('保留草稿并关闭'));
    await tester.pumpAndSettle();
    expect(repo.saved, isEmpty);
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    expect(find.widgetWithText(TextField, 'Draft question'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    expect(find.widgetWithText(TextField, 'Draft question'), findsOneWidget);
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.saved.single['question'], 'Draft question');
    expect(tester.takeException(), isNull);
  });
  testWidgets('筛选后空白页仍可继续分页找到线索', (tester) async {
    final repo = FakeResearch()..paging = true;
    await pump(tester, repo);
    expect(find.text('Client organizer'), findsNothing);
    await tester.tap(find.text('加载更多'));
    await tester.pumpAndSettle();
    expect(repo.pages, 2);
    expect(find.text('Client organizer'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets('原文证据经确认与研究一起保存', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('添加证据'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('添加证据'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, '来源链接'),
      'https://example.test/review',
    );
    await tester.enterText(
      find.widgetWithText(TextField, '原文摘录（保留准确措辞）'),
      'I would pay for team collaboration.',
    );
    await tester.pumpAndSettle();
    await tester.tap(find.text('确认'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(
      repo.saved.single['materials'].single['body'],
      'I would pay for team collaboration.',
    );
    expect(
      repo.saved.single['materials'].single['origin'],
      'user-confirmed-excerpt',
    );
    expect(tester.takeException(), isNull);
  });
  testWidgets('无可靠线索时可切换全部产品，筛选不越界', (tester) async {
    await pump(tester, FakeResearch()..quiet = true);
    expect(find.text('Client organizer'), findsNothing);
    await tester.tap(find.text('全部产品'));
    await tester.pumpAndSettle();
    expect(find.text('Client organizer'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  test('仅刷新时间和标题不使放弃的产品重新浮现', () {
    final study = {
      'evidence': {
        'payload': {'ratings_count': 12},
      },
    };
    ScoutProduct product(int count) => ScoutProduct({
      'payload': {'ratings_count': count},
      'reasons': [
        {'kind': 'sustained_growth'},
      ],
    });
    expect(scoutHasNewEvidence(product(12), study), isFalse);
    expect(scoutHasNewEvidence(product(13), study), isTrue);
  });
  testWidgets('偏好由用户填写且可以撤回', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    await tester.tap(find.byTooltip('研究偏好'));
    await tester.pumpAndSettle();
    await tester.enterText(find.widgetWithText(TextField, '我关注的人群'), '独立设计师');
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.criteria['audience'], '独立设计师');
    await tester.tap(find.byTooltip('研究偏好'));
    await tester.pumpAndSettle();
    expect(find.widgetWithText(TextField, '独立设计师'), findsOneWidget);
    await tester.tap(find.text('清空偏好'));
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.criteria['audience'], '');
    expect(tester.takeException(), isNull);
  });
  testWidgets('同一产品可归属多个主题并固定个人判断依据', (tester) async {
    final repo = FakeResearch()
      ..criteria = {'audience': '独立设计师', 'constraints': '每周2小时'};
    await pump(tester, repo);
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('需求主题与个人适配'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('需求主题与个人适配'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(
      find.widgetWithText(TextField, '需求主题（每行一个，手动确认）'),
    );
    await tester.enterText(
      find.widgetWithText(TextField, '需求主题（每行一个，手动确认）'),
      '设计师交付项目\n自由职业者收款',
    );
    await tester.ensureVisible(find.text('将这些偏好记为本次判断依据'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('将这些偏好记为本次判断依据'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.saved.single['themes'], ['设计师交付项目', '自由职业者收款']);
    expect(repo.saved.single['preferences_basis']['audience'], '独立设计师');
    expect(tester.takeException(), isNull);
  });
  testWidgets('重建工作区后仍可从本机草稿恢复问题', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    await tester.tap(find.text('加入研究').first);
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, '关键未知 / 我要验证的问题'),
      'Restart-safe question',
    );
    await tester.pump(const Duration(milliseconds: 650));
    await tester.pumpAndSettle();
    expect(repo.localDrafts.values.single['question'], 'Restart-safe question');
    await tester.pumpWidget(const SizedBox.shrink());
    await pump(tester, repo);
    await tester.tap(find.text('跟进研究'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('本机草稿 · 1'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    expect(
      find.widgetWithText(TextField, 'Restart-safe question'),
      findsOneWidget,
    );
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.localDrafts, isEmpty);
    expect(repo.saved.single['question'], 'Restart-safe question');
    expect(tester.takeException(), isNull);
  });
  testWidgets('卡片上的观察和不相关操作需保存，旧证据仍保留', (tester) async {
    final repo = FakeResearch();
    await pump(tester, repo);
    await tester.tap(find.text('继续观察'));
    await tester.pumpAndSettle();
    expect(repo.saved, isEmpty);
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.saved.single['state'], 'watching');
    final frozen = repo.saved.single['evidence'];
    await tester.tap(find.byTooltip('不相关'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, '为什么关注 / 切入假设'),
      'Outside my available time',
    );
    await tester.tap(find.text('保存研究'));
    await tester.pumpAndSettle();
    expect(repo.saved.last['state'], 'rejected');
    expect(repo.saved.last['evidence'], frozen);
    expect(tester.takeException(), isNull);
  });
  testWidgets('窄屏可扫描产品并打开完整宽度详情', (tester) async {
    await pump(tester, FakeResearch(), width: 360);
    await tester.tap(find.text('Client organizer'));
    await tester.pumpAndSettle();
    expect(find.text('证据缺口'), findsOneWidget);
    await tester.tap(find.text('加入研究').last);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await tester.tap(find.text('保留草稿并关闭'));
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('关闭'));
    await tester.pumpAndSettle();
    expect(find.text('发现线索'), findsWidgets);
  });
}

Future<void> capture(WidgetTester tester, String path) async {
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byKey(const ValueKey('research-preview')),
  );
  await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1.5);
    final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
    await File(path).writeAsBytes(bytes!.buffer.asUint8List());
    image.dispose();
  });
}
