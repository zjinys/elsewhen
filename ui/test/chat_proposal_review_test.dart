import 'dart:convert';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/widgets/chat_proposal_review.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

api.ChatProposalDto proposal(String id, {int version = 1, String? args}) =>
    api.ChatProposalDto(
      kind: 'action',
      id: id,
      conversationId: 'conversation',
      action: 'create_todo',
      argsJson:
          args ?? jsonEncode({'title': id, 'related_wiki_slug': 'source'}),
      version: version,
      createdAt: '2026-09-30T10:00:00Z',
      approvalToken: '$id-v$version',
      targetBasisJson: '[]',
    );

class ReviewRepo extends ConversationRepository {
  ReviewRepo() : super(RustBridgeRepository());
  final requests = <api.ChatReviewRequest>[];
  final edits = <Map<String, dynamic>>[];
  List<api.ChatProposalDto> pending = [
    proposal('A'),
    proposal('B'),
    proposal('C'),
  ];

  /// 处理记录（回执）列表；默认空，需要测「处理记录」页签时填。
  List<api.ChatDecisionDto> receipts = [];
  bool stale = false;
  String? failId;
  String status = 'succeeded';

  /// 回执口径控制：null=默认短回执；长回执用于折叠摘要测试。
  String? receiptResult;

  @override
  Future<List<api.ChatProposalDto>> listChatProposals(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) async => pending.skip(offset).take(limit).toList();
  @override
  Future<List<api.ChatDecisionDto>> listChatDecisions(
    String? conversationId, {
    int offset = 0,
    int limit = 25,
  }) async => [if (receipts.isNotEmpty) ...receipts.skip(offset).take(limit)];
  @override
  Future<List<dynamic>> listPendingActions(String conversationId) async => [];

  @override
  Future<api.ChatProposalDto> editChatProposal(
    api.ChatReviewRequest request,
    String argsJson,
  ) async {
    edits.add(Map<String, dynamic>.from(jsonDecode(argsJson) as Map));
    return proposal(
      request.proposalId,
      version: request.expectedVersion + 1,
      args: argsJson,
    );
  }

  @override
  Future<api.ChatDecisionDto> decideChatProposal(
    api.ChatReviewRequest request, {
    required bool accept,
  }) async {
    requests.add(request);
    if (stale) throw StateError('提议已变化，请重新审阅');
    if (request.proposalId == failId) throw StateError('连接中断，结果未知');
    return api.ChatDecisionDto(
      kind: request.kind,
      id: request.proposalId,
      conversationId: request.conversationId,
      version: request.expectedVersion,
      decision: accept ? 'accept' : 'reject',
      status: accept ? status : 'declined',
      result: receiptResult ?? '回执 ${request.proposalId}',
      action: 'create_todo',
      argsJson: '{}',
      startedAt: '2026-11-04T08:00:00Z',
    );
  }
}

Future<void> mount(
  WidgetTester tester,
  ReviewRepo repo,
  Widget child, {
  Size size = const Size(900, 850),
  Locale locale = const Locale('zh'),
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    ProviderScope(
      overrides: [conversationRepositoryProvider.overrideWithValue(repo)],
      child: MaterialApp(
        locale: locale,
        supportedLocales: AppLocalizations.supportedLocales,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(body: child),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  for (final locale in [const Locale('zh'), const Locale('en')]) {
    testWidgets(
      '${locale.languageCode}: revision previews new evidence before approval',
      (tester) async {
        final repo = ReviewRepo();
        final item = api.ChatProposalDto(
          kind: 'action',
          id: 'evidence-update',
          conversationId: 'conversation',
          action: 'save_wiki_revision',
          version: 1,
          createdAt: '2026-11-09T10:00:00Z',
          approvalToken: 'evidence-token',
          targetBasisJson: '[]',
          argsJson: jsonEncode({
            'title': '客户访谈方法',
            'content_md': '补入最近一次实际行为的提问方法',
            'save_as': 'revision',
            'verified_evidence': [
              {
                'title': '访谈观察',
                'page_slug': 'method/observation',
                'excerpt': '先问最近一次实际行为',
                'applicable_when': '小店初期访谈',
              },
            ],
            'added_input_evidence': {
              'event_id': 'event-1',
              'occurred_at': '2026-11-09T10:00:00Z',
              'excerpt': '今天访谈了三位店主',
            },
          }),
        );
        await mount(
          tester,
          repo,
          ChatProposalDetailDialog(proposal: item),
          locale: locale,
          size: const Size(390, 850),
        );
        final strings = lookupAppLocalizations(locale);
        expect(find.text(strings.wikiAddedEvidence), findsOneWidget);
        expect(find.text('访谈观察 · method/observation'), findsOneWidget);
        expect(find.text('先问最近一次实际行为'), findsOneWidget);
        expect(find.text('今天访谈了三位店主'), findsOneWidget);
        expect(repo.requests, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );
  }
  testWidgets(
    'English review keeps the displayed version and original JSON keys',
    (tester) async {
      final repo = ReviewRepo()..stale = true;
      await mount(
        tester,
        repo,
        ChatProposalDetailDialog(proposal: proposal('用户标题')),
        locale: const Locale('en'),
        size: const Size(420, 850),
      );
      expect(find.text('用户标题'), findsOneWidget);
      await tester.tap(find.text('Accept this'));
      await tester.pumpAndSettle();
      expect(repo.requests.single.proposalId, '用户标题');
      expect(repo.requests.single.expectedVersion, 1);
      expect(repo.requests.single.approvalToken, '用户标题-v1');
      expect(
        find.textContaining(
          'A new revision will not be accepted automatically',
        ),
        findsOneWidget,
      );
      await tester.tap(find.text('Accept this'));
      await tester.pumpAndSettle();
      expect(repo.requests.map((r) => r.approvalToken), ['用户标题-v1', '用户标题-v1']);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'accept uses the displayed version and never refreshes authorization on error',
    (tester) async {
      final repo = ReviewRepo()..stale = true;
      await mount(
        tester,
        repo,
        ChatProposalDetailDialog(proposal: proposal('A')),
      );
      await tester.tap(find.text('接受这项'));
      await tester.pumpAndSettle();
      expect(repo.requests.single.proposalId, 'A');
      expect(repo.requests.single.conversationId, 'conversation');
      expect(repo.requests.single.expectedVersion, 1);
      expect(repo.requests.single.approvalToken, 'A-v1');
      expect(find.textContaining('不会自动接受新的版本'), findsOneWidget);
      await tester.tap(find.text('接受这项'));
      await tester.pumpAndSettle();
      expect(repo.requests.map((r) => r.approvalToken), ['A-v1', 'A-v1']);
    },
  );

  testWidgets('editing saves a new preview but needs separate acceptance', (
    tester,
  ) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      ChatProposalDetailDialog(proposal: proposal('A')),
    );
    await tester.tap(find.text('编辑'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('title')),
      'Changed title',
    );
    await tester.tap(find.text('保存修改，不执行'));
    await tester.pumpAndSettle();
    expect(repo.requests, isEmpty);
    expect(repo.edits.single['title'], 'Changed title');
    expect(repo.edits.single['priority'], 'normal');
    expect(repo.edits.single['related_wiki_slug'], 'source');
    expect(find.textContaining('版本 2'), findsOneWidget);
    await tester.tap(find.text('接受这项'));
    await tester.pumpAndSettle();
    expect(repo.requests.single.expectedVersion, 2);
    expect(repo.requests.single.approvalToken, 'A-v2');
  });

  testWidgets(
    'uncertain result is not displayed as success or offered for blind replay',
    (tester) async {
      final repo = ReviewRepo()..status = 'uncertain';
      await mount(
        tester,
        repo,
        ChatProposalDetailDialog(proposal: proposal('A')),
      );
      await tester.tap(find.text('接受这项'));
      await tester.pumpAndSettle();
      expect(find.text('结果待核实'), findsOneWidget);
      expect(find.text('已完成'), findsNothing);
      expect(find.text('接受这项'), findsNothing);
      expect(repo.requests, hasLength(1));
    },
  );

  testWidgets('面板形态：点提议进详情后能「返回列表」回到列表窗格', (tester) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      const ChatProposalReviewDialog(conversationId: 'conversation'),
      size: const Size(900, 850),
    );
    // 列表窗格：点第一条提议
    await tester.tap(find.text('A'));
    await tester.pumpAndSettle();
    // 详情窗格出现，顶部「返回列表」入口（底部重复入口已删 2026-11-04）
    expect(find.text('返回列表'), findsOneWidget);
    await tester.tap(find.text('返回列表'));
    await tester.pumpAndSettle();
    // 回到列表窗格：详情收起，列表重新可见
    expect(find.text('返回列表'), findsNothing);
    expect(find.text('A'), findsOneWidget);
    expect(find.text('选择左侧一条提议查看详情'), findsOneWidget);
  });

  testWidgets('面板形态：处理记录详情也能「返回列表」', (tester) async {
    final repo = ReviewRepo()..receiptResult = '回执 A';
    await mount(
      tester,
      repo,
      const ChatProposalReviewDialog(conversationId: 'conversation'),
      size: const Size(900, 850),
    );
    // 先选中并进入详情，再接受
    await tester.tap(find.text('A'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('接受这项'));
    await tester.pumpAndSettle();
    // 决策完成后详情面板显示回执；顶部返回列表入口应仍可用
    expect(find.text('返回列表'), findsOneWidget);
    await tester.tap(find.text('返回列表'));
    await tester.pumpAndSettle();
    expect(find.text('返回列表'), findsNothing);
    expect(find.text('选择左侧一条提议查看详情'), findsOneWidget);
  });

  testWidgets('处理记录详情：顶部有「返回列表」，系统返回也回列表', (tester) async {
    final repo = ReviewRepo()
      ..receipts = [
        api.ChatDecisionDto(
          kind: 'action',
          id: 'A',
          conversationId: 'conversation',
          version: 1,
          decision: 'accept',
          status: 'succeeded',
          result: '回执 A',
          action: 'create_todo',
          argsJson: '{}',
          startedAt: '2026-11-04T08:00:00Z',
        ),
      ];
    await mount(
      tester,
      repo,
      const ChatProposalReviewDialog(conversationId: 'conversation'),
      size: const Size(900, 850),
    );
    // 切到处理记录页签，打开一条回执详情（内联路径，此前没有返回按钮）
    await tester.tap(find.text('处理记录'));
    await tester.pumpAndSettle();
    await tester.tap(find.textContaining('创建待办 · 已完成'));
    await tester.pumpAndSettle();
    expect(find.text('返回列表'), findsOneWidget);
    // 系统返回手势：回列表而不是关面板
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();
    expect(find.text('返回列表'), findsNothing);
    expect(find.text('回执 A'), findsOneWidget);
  });

  testWidgets('系统返回手势：详情态回列表而不是关掉整个面板', (tester) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      const ChatProposalReviewDialog(conversationId: 'conversation'),
      size: const Size(900, 850),
    );
    await tester.tap(find.text('A'));
    await tester.pumpAndSettle();
    expect(find.text('返回列表'), findsOneWidget);
    // 模拟 Android 返回键/手势
    await tester.binding.handlePopRoute();
    await tester.pumpAndSettle();
    // 回到列表态：详情收起，面板整体没有被关（还能看到列表与占位）
    expect(find.text('返回列表'), findsNothing);
    expect(find.text('A'), findsOneWidget);
    expect(find.text('选择左侧一条提议查看详情'), findsOneWidget);
  });

  testWidgets('窄屏页内切换：详情页内也能「返回列表」', (tester) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      const ChatProposalReviewDialog(conversationId: 'conversation'),
      size: const Size(360, 760),
    );
    await tester.tap(find.text('A'));
    await tester.pumpAndSettle();
    expect(find.text('返回列表'), findsOneWidget);
    await tester.tap(find.text('返回列表'));
    await tester.pumpAndSettle();
    // 窄屏：返回后应回到列表（页内切换，不叠对话框）
    expect(find.text('返回列表'), findsNothing);
    expect(find.text('A'), findsOneWidget);
  });

  testWidgets(
    'batch review authorizes only explicitly selected snapshots and reports partial failure',
    (tester) async {
      final repo = ReviewRepo()..failId = 'C';
      await mount(
        tester,
        repo,
        const ChatProposalReviewDialog(conversationId: 'conversation'),
      );
      await tester.tap(find.byKey(const ValueKey('select-action-A')));
      await tester.tap(find.byKey(const ValueKey('select-action-C')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('审阅选中（2）'));
      await tester.pumpAndSettle();
      expect(find.textContaining('不是整批原子提交'), findsOneWidget);
      await tester.tap(find.text('接受这些项'));
      await tester.pumpAndSettle();
      expect(repo.requests.map((r) => r.proposalId), ['A', 'C']);
      expect(repo.requests.map((r) => r.approvalToken), ['A-v1', 'C-v1']);
      expect(find.textContaining('已完成：回执 A'), findsOneWidget);
      expect(find.textContaining('本项未确认完成'), findsOneWidget);
      expect(find.text('接受这些项'), findsNothing);
    },
  );

  testWidgets('corrupt proposal cannot be accepted but can be rejected', (
    tester,
  ) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      ChatProposalDetailDialog(proposal: proposal('bad', args: 'broken-json')),
    );
    final button = tester.widget<FilledButton>(
      find.widgetWithText(FilledButton, '接受这项'),
    );
    expect(button.onPressed, isNull);
    await tester.tap(find.text('拒绝这项'));
    await tester.pumpAndSettle();
    expect(repo.requests.single.proposalId, 'bad');
    expect(find.text('已拒绝'), findsOneWidget);
  });

  testWidgets('review content and controls fit a narrow viewport', (
    tester,
  ) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      ChatProposalDetailDialog(proposal: proposal('A')),
      size: const Size(360, 760),
    );
    expect(find.text('接受这项'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  // 回执摘要化（事故 2026-10-19 根因 1.3）：长回执折叠预览可展开，
  // 短回执原样直显；完整内容保留可复制。
  testWidgets('long receipts collapse to preview with expand action', (
    tester,
  ) async {
    final repo = ReviewRepo()..receiptResult = '${'数据行\n' * 60}TAILMARKER';
    await mount(
      tester,
      repo,
      ChatProposalDetailDialog(proposal: proposal('A')),
    );
    await tester.tap(find.text('接受这项'));
    await tester.pumpAndSettle();
    expect(find.textContaining('展开完整回执'), findsOneWidget);
    expect(
      find.textContaining('TAILMARKER'),
      findsNothing,
      reason: '折叠态不糊全文尾部',
    );
    final expand = find.textContaining('展开完整回执');
    await tester.ensureVisible(expand);
    await tester.pumpAndSettle();
    await tester.tap(expand, warnIfMissed: true);
    await tester.pumpAndSettle();
    expect(
      find.textContaining('TAILMARKER'),
      findsOneWidget,
      reason: '展开后全文可见',
    );
  });

  testWidgets('short receipts render plainly without expand action', (
    tester,
  ) async {
    final repo = ReviewRepo();
    await mount(
      tester,
      repo,
      ChatProposalDetailDialog(proposal: proposal('A')),
    );
    await tester.tap(find.text('接受这项'));
    await tester.pumpAndSettle();
    expect(find.textContaining('回执 A'), findsOneWidget);
    expect(find.textContaining('展开完整回执'), findsNothing);
  });

  testWidgets('create_goal proposal renders facets card and supports editing', (
    tester,
  ) async {
    final repo = ReviewRepo();
    final goalProposal = api.ChatProposalDto(
      kind: 'action',
      id: 'goal-p1',
      conversationId: 'conversation',
      action: 'create_goal',
      argsJson: jsonEncode({
        'content': '发布 Elsewhen 1.0',
        'phase': 'near',
        'why': '验证核心认知闭环',
        'current_focus': '攻克多端同步快照收敛',
        'blocker': '真机证书配置复杂',
        'done_criteria': '多端测试全绿通过',
      }),
      version: 1,
      createdAt: '2026-11-02T10:00:00Z',
      approvalToken: 'goal-p1-v1',
      targetBasisJson: '[]',
    );

    await mount(tester, repo, ChatProposalDetailDialog(proposal: goalProposal));

    // 验证阶段卡片结构化呈现
    expect(find.text('发布 Elsewhen 1.0'), findsOneWidget);
    expect(find.text('近期目标'), findsOneWidget);
    expect(find.textContaining('攻克多端同步快照收敛'), findsOneWidget);
    expect(find.textContaining('真机证书配置复杂'), findsOneWidget);
    expect(find.textContaining('验证核心认知闭环'), findsOneWidget);
    expect(find.textContaining('多端测试全绿通过'), findsOneWidget);

    // 点击「编辑」
    await tester.tap(find.text('编辑'));
    await tester.pumpAndSettle();

    // 验证可编辑阶段字段输入框存在
    expect(find.widgetWithText(TextField, '攻克多端同步快照收敛'), findsOneWidget);
    expect(find.text('当前 1~2 周具体行动'), findsOneWidget);
    expect(find.text('目标愿景'), findsOneWidget);

    // 修改行动
    await tester.enterText(
      find.widgetWithText(TextField, '攻克多端同步快照收敛'),
      '跑通 Android 真机自同步',
    );
    await tester.tap(find.text('保存修改，不执行'));
    await tester.pumpAndSettle();

    expect(repo.edits.single['current_focus'], '跑通 Android 真机自同步');
  });
}

void main2() {}
