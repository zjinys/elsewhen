import 'dart:convert';

import 'package:elsewhen_ui/bridge/api.dart' show PendingActionDto;
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/message_area.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'chat_run_widget_test.dart' show settleFrames;
import 'support/chat_run_fixture.dart';

void main() {
  testWidgets('保存联系人选择后收起横幅，仍保留待审提议', (tester) async {
    final repo = _PendingRelationsRepository();
    Widget app() => ProviderScope(
      overrides: [
        conversationRepositoryProvider.overrideWithValue(repo),
        selectedConversationIdProvider.overrideWith(
          () => StateHolder('conv-1'),
        ),
        wikiPagesProvider.overrideWith(
          (_) async => [_contact('客户代表'), _contact('付款对接')],
        ),
      ],
      child: const MaterialApp(home: Scaffold(body: MessageArea())),
    );

    await tester.pumpWidget(app());
    await settleFrames(tester);
    final banner = find.textContaining('发现联系人：小林');
    final choose = find.text('选择');
    expect(banner, findsOneWidget);

    await tester.tap(choose);
    await settleFrames(tester);
    await tester.tap(find.text('取消'));
    await tester.pump(const Duration(milliseconds: 350));
    expect(repo.attempts, 0);
    expect(banner, findsOneWidget);

    Future<void> selectContact() async {
      await tester.tap(choose);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 350));
      await tester.tap(find.textContaining('付款对接'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 350));
      await settleFrames(tester);
    }

    await selectContact();
    expect(repo.attempts, 1);
    expect(banner, findsOneWidget);
    expect(choose, findsOneWidget);

    repo.saveSucceeds = true;
    await selectContact();
    expect(repo.attempts, 2);
    expect(banner, findsNothing);
    final pending = (await repo.listPendingActions('conv-1')).single;
    expect(
      jsonDecode(pending.argsJson)['relations'][0]['from_slug'],
      'person/付款对接',
    );

    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpWidget(app());
    await settleFrames(tester);
    expect(banner, findsNothing);
  });
}

WikiPageMeta _contact(String summary) {
  final date = DateTime(2026, 11, 19);
  return WikiPageMeta(
    id: summary,
    slug: 'person/$summary',
    kind: 'person',
    title: '小林',
    summary: summary,
    tags: const [],
    evidenceCount: 0,
    firstSeenAt: date,
    lastSeenAt: date,
    status: 'active',
    createdAt: date,
    updatedAt: date,
    area: 'network',
  );
}

class _PendingRelationsRepository extends RunTestRepository {
  var attempts = 0;
  var saveSucceeds = false;
  var args =
      '{"people":[{"name":"小林"}],"relations":[{"person":"小林","target":"项目甲"}]}';

  @override
  Future<List<PendingActionDto>> listPendingActions(
    String conversationId,
  ) async => [
    PendingActionDto(
      id: 'proposal-1',
      action: 'propose_people_relations',
      argsJson: args,
      createdAt: '2026-11-19T00:00:00Z',
      version: 1,
      approvalToken: 'token',
      targetBasisJson: '[]',
    ),
  ];

  @override
  Future<bool> updatePendingActionArgs(
    String conversationId,
    PendingActionDto action,
    String argsJson,
  ) async {
    attempts++;
    if (saveSucceeds) args = argsJson;
    return saveSucceeds;
  }
}
