import 'dart:convert';

import 'package:elsewhen_ui/bridge/api.dart' as api;
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
  bool stale = false;
  String? failId;
  String status = 'succeeded';

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
  }) async => [];
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
      result: '回执 ${request.proposalId}',
      action: 'create_todo',
      argsJson: '{}',
    );
  }
}

Future<void> mount(
  WidgetTester tester,
  ReviewRepo repo,
  Widget child, {
  Size size = const Size(900, 850),
}) async {
  await tester.binding.setSurfaceSize(size);
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    ProviderScope(
      overrides: [conversationRepositoryProvider.overrideWithValue(repo)],
      child: MaterialApp(home: Scaffold(body: child)),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
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
}
