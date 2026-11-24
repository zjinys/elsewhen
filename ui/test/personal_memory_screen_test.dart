import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:ui' as image;

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/l10n/localization.dart';
import 'package:elsewhen_ui/screens/personal_memory_screen.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/localizations.dart';

bool _previewFontLoaded = false;

class MemoryManagementFake extends RustBridgeRepository {
  final items = <Map<String, dynamic>>[
    for (var index = 0; index < 36; index++)
      {
        'id': 'memory-$index',
        'version': 7,
        'page_slug': 'memory-$index',
        'title': index == 0 ? '每周学习时间' : '项目经验 $index',
        'subject': 'user',
        'predicate': 'learning-$index',
        'kind': 'procedural',
        'state': 'active',
        'scope': 'personal',
        'support': 'supported',
        'confirmed': true,
        'validity': 'current',
        'pinned': false,
        'excluded': index == 1,
        'salience': 'normal',
        'conversation_id': 'owner-$index',
        'conversation_title': index == 1 ? '学习目标讨论' : '主对话流',
        'conversation_kind': index == 1 ? 'goal' : 'main',
      },
  ];
  String? editedContent;
  @override
  Future<void> editPersonalMemory(String claimId, int version, String content) async { editedContent = content; items.firstWhere((item) => item['id'] == claimId)['content'] = content; }

  final preparations = <Map<String, dynamic>>[];
  final requests = <api.ChatReviewRequest>[];
  final accepts = <bool>[];
  final detailsWaiting = <String, Completer<String>>{};
  bool failList = false, failApproval = false;

  Map<String, dynamic> detail(String id) {
    final item = Map<String, dynamic>.from(
      items.firstWhere((item) => item['id'] == id),
    );
    return {
      'claim': {
        ...item,
        'basis': [
          {'kind': 'message', 'id': 'source-$id', 'quote': '我每周有六小时学习。'},
        ],
      },
      'title': item['title'],
      'content': item['content'] ?? '每周六小时，用于持续学习和练习。',
      'complete': true,
      'validity': 'current',
      'detail_token': 'detail-$id-${item['version']}',
      'can_review': true,
      'conversation_title': item['conversation_title'],
      'conversation_kind': item['conversation_kind'],
      'sources': [
        {
          'kind': 'message',
          'id': 'source-$id',
          'quote': '我每周有六小时学习。',
          'current': true,
        },
      ],
      'goal_links': <dynamic>[],
      'decisions': <dynamic>[],
    };
  }

  @override
  Future<String> listPersonalMemories(
    String filterJson, {
    int offset = 0,
    int limit = 30,
  }) async {
    if (failList) throw StateError('isolated list failure');
    final filter = jsonDecode(filterJson) as Map;
    final filtered = items
        .where(
          (item) =>
              [
                'state',
                'kind',
                'excluded',
                'conversation_kind',
                'conversation_id',
                'subject',
                'predicate',
              ].every(
                (key) => filter[key] == null || item[key] == filter[key],
              ) &&
              (item['title'] as String).contains(
                filter['query'] as String? ?? '',
              ),
        )
        .toList();
    return jsonEncode({
      'items': filtered.skip(offset).take(limit).toList(),
      'total': filtered.length,
    });
  }

  @override
  Future<String> getPersonalMemoryDetail(String claimId) async =>
      detailsWaiting[claimId]?.future ?? jsonEncode(detail(claimId));

  @override
  Future<api.ChatProposalDto> preparePersonalMemoryAction({
    required String claimId,
    required int expectedVersion,
    required String detailToken,
    required String argsJson,
  }) async {
    final args = Map<String, dynamic>.from(jsonDecode(argsJson) as Map);
    preparations.add({
      'claim_id': claimId,
      'version': expectedVersion,
      'detail_token': detailToken,
      ...args,
    });
    final current = detail(claimId);
    return api.ChatProposalDto(
      kind: 'action',
      id: 'prepared-${preparations.length}',
      conversationId: (current['claim'] as Map)['conversation_id'] as String,
      action: 'review_memory',
      version: 3,
      createdAt: '2026-11-23T00:00:00Z',
      approvalToken: 'preview-token-$expectedVersion',
      targetBasisJson: '[]',
      argsJson: jsonEncode({
        ...args,
        'claim_id': claimId,
        'reviewed_basis': {
          'claim': current['claim'],
          'title': current['title'],
          'content_excerpt': current['content'],
          'complete': true,
          'validity': 'current',
          if (args['replacement_id'] != null)
            'replacement': {
              'claim': detail(args['replacement_id'] as String)['claim'],
              'content_excerpt': '每周八小时，用于新的学习计划。',
              'complete': true,
            },
          if (args['goal_id'] != null)
            'goal': {
              'goal': {'title': '完成学习计划'},
            },
        },
      }),
    );
  }

  @override
  Future<api.ChatDecisionDto> decidePersonalMemoryAction(
    api.ChatReviewRequest request, {
    required bool accept,
  }) async {
    requests.add(request);
    accepts.add(accept);
    if (failApproval) throw StateError('stale version');
    final prepared = preparations.last;
    final item = items.firstWhere((item) => item['id'] == prepared['claim_id']);
    if (accept) {
      if (prepared['operation'] == 'pin') item['pinned'] = true;
      if (prepared['operation'] == 'exclude') item['excluded'] = true;
      item['version'] = (item['version'] as int) + 1;
    }
    return api.ChatDecisionDto(
      kind: 'action',
      id: request.proposalId,
      conversationId: request.conversationId,
      version: request.expectedVersion,
      decision: accept ? 'accept' : 'reject',
      status: 'succeeded',
      result: 'applied',
      action: 'review_memory',
      argsJson: '{}',
      startedAt: '2026-11-23T00:00:00Z',
    );
  }

  @override
  Future<List<api.GoalDto>> listMemoryLinkGoals() async => [
    const api.GoalDto(
      id: 'goal-1',
      content: '完成学习计划',
      phase: 'near',
      status: 'active',
      createdAt: '2026-11-23T00:00:00Z',
      updatedAt: '2026-11-23T00:00:00Z',
    ),
  ];

  @override
  Future<String> inspectPersonalMemoryMaintenance(
    String conversationId,
  ) async => jsonEncode({
    'scanned': 2,
    'exact_body_groups': [
      ['memory-0', 'memory-1'],
    ],
    'shared_source_groups': [],
    'near_duplicate_hints': [],
    'unavailable_or_stale': [],
  });
}

Future<void> host(
  WidgetTester tester,
  MemoryManagementFake repo, {
  Size size = const Size(1120, 1000),
  Locale locale = const Locale('zh'),
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));
  if (!_previewFontLoaded &&
      Platform.environment['MEMORY_SCREENSHOT_DIR'] != null) {
    await tester.runAsync(() async {
      final font = File('/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc');
      if (!await font.exists()) return;
      final bytes = await font.readAsBytes();
      for (final family in ['Ahem', 'Roboto', 'Inter', 'Noto Sans CJK SC']) {
        await (FontLoader(
          family,
        )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
      }
      final icons = await File(
        'build/unit_test_assets/fonts/MaterialIcons-Regular.otf',
      ).readAsBytes();
      await (FontLoader(
        'MaterialIcons',
      )..addFont(Future.value(ByteData.sublistView(icons)))).load();
      _previewFontLoaded = true;
    });
  }
  await tester.pumpWidget(
    ProviderScope(
      overrides: [storageRepositoryProvider.overrideWithValue(repo)],
      child: RepaintBoundary(
        key: const ValueKey('memory-capture'),
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: AppTheme.darkTheme,
          locale: locale,
          localizationsDelegates: appTestLocalizationDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: const PersonalMemoryScreen(),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> selectFirst(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('memory-row-memory-0')));
  await tester.pumpAndSettle();
}

Future<void> capture(WidgetTester tester, String name) async {
  final directory = Platform.environment['MEMORY_SCREENSHOT_DIR'];
  if (directory == null) return;
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byKey(const ValueKey('memory-capture')),
  );
  await tester.runAsync(() async {
    final rendered = await boundary.toImage();
    final bytes = await rendered.toByteData(format: image.ImageByteFormat.png);
    await Directory(directory).create(recursive: true);
    await File('$directory/$name.png')
        .writeAsBytes(bytes!.buffer.asUint8List());
    rendered.dispose();
  });
}

void main() {
  testWidgets('editing independent memory saves actual content and keeps selection', (tester) async {
    final repo = MemoryManagementFake();
    repo.items.first['content'] = '原记忆正文';
    await host(tester, repo);
    await selectFirst(tester);

    await tester.tap(find.widgetWithText(OutlinedButton, '编辑'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, '修改后的实际记忆');
    await tester.tap(find.widgetWithText(FilledButton, '保存'));
    await tester.pumpAndSettle();
    expect(repo.editedContent, '修改后的实际记忆');
    expect(find.text('修改后的实际记忆'), findsWidgets);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'desktop shows sources and paginates the entire local collection',
    (tester) async {
      final repo = MemoryManagementFake();
      await host(tester, repo);
      expect(find.text('1–30 / 36 条记忆'), findsOneWidget);
      await selectFirst(tester);
      expect(find.text('每周六小时，用于持续学习和练习。'), findsOneWidget);
      expect(find.text('个人通用'), findsOneWidget);
      expect(repo.requests, isEmpty);
      await capture(tester, 'desktop');
      await tester.tap(find.text('返回记忆列表'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('下一页'));
      await tester.pumpAndSettle();
      expect(find.text('31–36 / 36 条记忆'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('memory-row-memory-35')),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('narrow detail returns to the list and localizes in English', (
    tester,
  ) async {
    final repo = MemoryManagementFake();
    await host(
      tester,
      repo,
      size: const Size(380, 844),
      locale: const Locale('en'),
    );
    await selectFirst(tester);
    expect(find.text('Back to memories'), findsOneWidget);
    expect(find.text('每周六小时，用于持续学习和练习。'), findsOneWidget);
    await capture(tester, 'mobile-en');
    await tester.tap(find.text('Back to memories'));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('memory-row-memory-0')), findsOneWidget);
    expect(repo.requests, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets('pin waits for approval and submits the exact preview token', (
    tester,
  ) async {
    final repo = MemoryManagementFake();
    await host(tester, repo);
    await selectFirst(tester);
    await tester.tap(find.widgetWithText(OutlinedButton, '固定'));
    await tester.pumpAndSettle();
    expect(find.text('核对记忆修改'), findsOneWidget);
    expect(repo.items.first['pinned'], false);
    expect(repo.requests, isEmpty);
    expect(repo.preparations.single['detail_token'], 'detail-memory-0-7');
    await capture(tester, 'review');
    await tester.tap(find.byKey(const ValueKey('memory-approve')));
    await tester.pumpAndSettle();
    expect(repo.items.first['pinned'], true);
    expect(repo.requests.single.approvalToken, 'preview-token-7');
    expect(repo.requests.single.expectedVersion, 3);
    expect(repo.requests.single.conversationId, 'owner-0');
    expect(repo.accepts, [true]);
  });

  testWidgets(
    'cancelling a prepared exclusion rejects it without changing memory',
    (tester) async {
      final repo = MemoryManagementFake();
      await host(tester, repo);
      await selectFirst(tester);
      await tester.tap(find.widgetWithText(OutlinedButton, '排除自动使用'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();
      expect(repo.accepts, [false]);
      expect(repo.items.first['excluded'], false);
    },
  );

  testWidgets(
    'failed approval cannot silently fetch or approve a new version',
    (tester) async {
      final repo = MemoryManagementFake()..failApproval = true;
      await host(tester, repo);
      await selectFirst(tester);
      await tester.tap(find.widgetWithText(OutlinedButton, '固定'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('memory-approve')));
      await tester.pumpAndSettle();
      expect(find.text('修改未完成，请关闭后刷新详情重新审阅。'), findsOneWidget);
      expect(
        tester
            .widget<FilledButton>(find.byKey(const ValueKey('memory-approve')))
            .onPressed,
        isNull,
      );
      expect(repo.preparations, hasLength(1));
      expect(repo.requests, hasLength(1));
      expect(repo.items.first['pinned'], false);
      await tester.tap(find.text('关闭'));
      await tester.pumpAndSettle();
    },
  );

  testWidgets('plain memory detail does not require source or goal controls', (tester) async {
    final repo = MemoryManagementFake();
    await host(tester, repo);
    await selectFirst(tester);
    expect(find.text('每周六小时，用于持续学习和练习。'), findsOneWidget);
    expect(find.widgetWithText(OutlinedButton, '关联目标'), findsNothing);
    expect(repo.preparations, isEmpty);
  });

  testWidgets(
    'search filters on server and request failures are not empty states',
    (tester) async {
      final repo = MemoryManagementFake()..failList = true;
      await host(tester, repo);
      expect(find.text('没有符合条件的记忆'), findsNothing);
      expect(find.text('尚未创建记忆'), findsNothing);
      expect(find.text('重试'), findsOneWidget);
      repo.failList = false;
      await tester.tap(find.text('重试'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).first, '每周学习');
      await tester.pump(const Duration(milliseconds: 350));
      await tester.pumpAndSettle();
      expect(find.text('1–1 / 1 条记忆'), findsOneWidget);
      expect(find.byKey(const ValueKey('memory-row-memory-1')), findsNothing);
    },
  );

  testWidgets('empty memory list remains readable and search does not mutate storage', (tester) async {
    final repo = MemoryManagementFake()..items.clear();
    await host(tester, repo);
    expect(find.text('尚未创建记忆'), findsOneWidget);
    await tester.enterText(find.byType(TextField).first, '学习');
    await tester.pumpAndSettle();
    expect(find.text('尚未创建记忆'), findsOneWidget);
    expect(repo.preparations, isEmpty);
    expect(repo.requests, isEmpty);
  });

  testWidgets(
    'replacement picker previews the selected newer version before approval',
    (tester) async {
      final repo = MemoryManagementFake();
      repo.items[1].addAll({
        'predicate': 'learning-0',
        'conversation_id': 'owner-0',
        'state': 'pending',
        'excluded': false,
      });
      await host(tester, repo);
      await selectFirst(tester);
      await tester.tap(find.byTooltip('更多记忆操作'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('由新记忆替代'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(ListTile, '项目经验 1').last);
      await tester.pumpAndSettle();
      expect(find.text('每周八小时，用于新的学习计划。'), findsOneWidget);
      expect(repo.preparations.single['replacement_id'], 'memory-1');
      expect(repo.requests, isEmpty);
      await tester.tap(find.text('取消'));
      await tester.pumpAndSettle();
      expect(repo.accepts, [false]);
    },
  );

  testWidgets('cancelling an edit keeps actual memory unchanged', (tester) async {
    final repo = MemoryManagementFake();
    repo.items.first['content'] = '原正文';
    await host(tester, repo);
    await selectFirst(tester);
    await tester.tap(find.widgetWithText(OutlinedButton, '编辑'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextField).last, '未保存正文');
    await tester.tap(find.text('取消'));
    await tester.pumpAndSettle();
    expect(repo.editedContent, isNull);
    expect(repo.items.first['content'], '原正文');
    expect(tester.takeException(), isNull);
  });

  testWidgets('late detail result cannot overwrite a newer selection', (
    tester,
  ) async {
    final repo = MemoryManagementFake();
    final first = Completer<String>();
    repo.detailsWaiting['memory-0'] = first;
    await host(tester, repo);
    await tester.tap(find.byKey(const ValueKey('memory-row-memory-0')));
    await tester.pump();
    await tester.tap(find.text('返回记忆列表'));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('memory-row-memory-1')));
    await tester.pumpAndSettle();
    first.complete(jsonEncode(repo.detail('memory-0')));
    await tester.pumpAndSettle();
    expect(
      find.byKey(const ValueKey('memory-detail-memory-1')),
      findsOneWidget,
    );
    expect(find.byKey(const ValueKey('memory-detail-memory-0')), findsNothing);
  });
}
