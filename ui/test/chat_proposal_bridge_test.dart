import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

api.ChatReviewRequest reviewed(
  api.ChatProposalDto p, {
  String? wrongConversation,
}) => api.ChatReviewRequest(
  kind: p.kind,
  proposalId: p.id,
  conversationId: wrongConversation ?? p.conversationId,
  expectedVersion: p.version,
  approvalToken: p.approvalToken,
);

void main() {
  test('isolated real bridge binds chat and wiki decisions to displayed proposal versions', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final responses = <Map<String, dynamic>>[];
    final requests = <Map<String, dynamic>>[];
    server.listen((request) async {
      final body = jsonDecode(
        await utf8.decoder.bind(request).join(),
      ) as Map<String, dynamic>;
      requests.add(body);
      request.response.headers.contentType = ContentType.json;
      if (responses.isEmpty) {
        request.response.statusCode = 500;
        request.response.write(
          jsonEncode({'error': 'fixture responses exhausted'}),
        );
      } else {
        request.response.write(
          jsonEncode({
            'model': 'fixture',
            'choices': [
              {'message': responses.removeAt(0), 'finish_reason': 'stop'},
            ],
          }),
        );
      }
      await request.response.close();
    });
    // This fixture exercises many review decisions, not a 4096-token answer.
    // Keep the real 32K input gate but reserve only the short scripted response.
    await api.saveAiProviderConfig(
      provider: api.AiProviderConfigDto(
        id: '',
        name: 'review fixture',
        providerType: 'openai-compatible',
        baseUrl: 'http://127.0.0.1:${server.port}/v1',
        model: 'fixture',
        apiKeySource: 'database',
        hasApiKey: true,
        apiKey: 'fixture-key',
        isActive: true,
        temperature: 0,
        maxTokens: 512,
        contextWindow: 32768,
      ),
    );

    // 延迟工具 → 打包它的内建技能；常驻工具无需 enable。
    const deferredSkill = {
      'create_todo': 'todo-actions',
      'rename_wiki_page': 'wiki-maintenance',
    };
    void tool(String name, Map<String, dynamic> args) {
      // 延迟加载后的真实链路：模型先 enable_tools 拿到 schema，同轮再发起调用；
      // 循环内逐个处理，enable 在前则后面的调用能过守卫。
      final skill = deferredSkill[name];
      responses.add({
        'role': 'assistant',
        'content': '',
        'tool_calls': [
          if (skill != null)
            {
              'id': 'call-enable-${requests.length}',
              'type': 'function',
              'function': {
                'name': 'enable_tools',
                'arguments': jsonEncode({
                  'skills': [skill],
                }),
              },
            },
          {
            'id': 'call-${requests.length}',
            'type': 'function',
            'function': {'name': name, 'arguments': jsonEncode(args)},
          },
        ],
      });
      responses.add({'role': 'assistant', 'content': '草稿已准备好，请在审阅卡确认具体内容。'});
    }

    Future<void> say(
      String conversation,
      String text, {
      bool wiki = false,
    }) async {
      if (wiki) {
        await repo.sendMessage(conversation, 'user', text);
      } else {
        await repo.submitConversationInput(
          conversation,
          text,
          idempotencyKey: 'input-${requests.length}',
        );
      }
      await repo.generateReply(conversation);
      expect(responses, isEmpty);
    }

    final main = await repo.createConversation(title: 'review fixture');
    tool('create_todo', {'title': 'A'});
    await say(main.id, '帮我建待办 A');
    tool('create_todo', {'title': 'B'});
    await say(main.id, '再建待办 B');
    final initial = await repo.listChatProposals(main.id);
    expect(initial, hasLength(2));
    final a = initial.firstWhere((p) => jsonDecode(p.argsJson)['title'] == 'A');
    final b = initial.firstWhere((p) => jsonDecode(p.argsJson)['title'] == 'B');
    responses.add({'role': 'assistant', 'content': '请在审阅卡选择具体提议。'});
    await say(main.id, '好');
    expect(await repo.listTodos(), isEmpty);
    expect(await repo.listChatProposals(main.id), hasLength(2));

    final editedArgs = Map<String, dynamic>.from(jsonDecode(a.argsJson) as Map)
      ..['title'] = 'A edited';
    final edited = await repo.editChatProposal(
      reviewed(a),
      jsonEncode(editedArgs),
    );
    expect(edited.version, greaterThan(a.version));
    await expectLater(
      repo.decideChatProposal(reviewed(a), accept: true),
      throwsA(anything),
    );
    final beforeReview = (await repo.listEvents())
        .map((e) => '${e.id}:${e.rawText}')
        .toList();
    final accepted = await repo.decideChatProposal(
      reviewed(edited),
      accept: true,
    );
    expect(accepted.status, 'succeeded');
    final repeated = await repo.decideChatProposal(
      reviewed(edited),
      accept: true,
    );
    expect(repeated.result, accepted.result);
    expect(await repo.listTodos(), hasLength(1));
    expect((await repo.listTodos()).single.title, 'A edited');
    expect(
      (await repo.decideChatProposal(reviewed(b), accept: false)).status,
      'declined',
    );
    expect(
      (await repo.listEvents()).map((e) => '${e.id}:${e.rawText}').toList(),
      beforeReview,
    );

    final page = await repo.saveTextPage(
      text: '本机测试材料，原文保持不变。',
      title: 'review source',
      tags: [],
    );
    final wiki = await api.ensureWikiPageChat(pageSlug: page.slug);
    tool('create_todo', {'title': 'Wiki action'});
    await say(wiki.id, '为这一页草拟待办', wiki: true);
    final wikiProposal = (await repo.listChatProposals(wiki.id)).single;
    await expectLater(
      repo.decideChatProposal(
        reviewed(wikiProposal, wrongConversation: main.id),
        accept: true,
      ),
      throwsA(anything),
    );
    expect(
      (await repo.decideChatProposal(
        reviewed(wikiProposal),
        accept: true,
      )).status,
      'succeeded',
    );
    expect(await repo.listTodos(), hasLength(2));

    responses.add({'role': 'assistant', 'content': '[规则提议]遇到执行状态不明确时先核对回执。'});
    await say(main.id, '把这条经验草拟为个人规则');
    final rule = (await repo.listChatProposals(main.id))
        .singleWhere((p) => p.kind == 'rule');
    expect(
      (await api.listRules()).singleWhere((r) => r.id == rule.id).status,
      'pending',
    );
    responses.add({'role': 'assistant', 'content': '规则仍待审阅。'});
    await say(main.id, '嗯');
    expect(
      (await api.listRules()).singleWhere((r) => r.id == rule.id).status,
      'pending',
    );
    expect(
      (await repo.decideChatProposal(reviewed(rule), accept: true)).status,
      'succeeded',
    );
    expect(
      (await api.listRules()).singleWhere((r) => r.id == rule.id).status,
      'active',
    );
    expect(await repo.listChatProposals(null), isEmpty);

    tool('rename_wiki_page', {
      'slug': page.slug,
      'new_title': 'renamed source',
      'reason': 'fixture',
    });
    await say(wiki.id, '把此页改名为 renamed source', wiki: true);
    final rename = (await repo.listChatProposals(wiki.id)).single;
    await api.updateWikiTags(slug: page.slug, tags: ['human-change']);
    await expectLater(
      repo.decideChatProposal(reviewed(rename), accept: true),
      throwsA(anything),
    );
    expect((await repo.getWikiPage(page.slug))!.title, 'review source');
    expect((await repo.getWikiPage(page.slug))!.contentMd, '本机测试材料，原文保持不变。');

    tool('save_knowledge_draft', {
      'title': 'Versioned draft',
      'content_md': '完整草稿正文',
    });
    await say(main.id, '将结论草拟成新知识页');
    final draft = (await repo.listPendingActions(main.id))
        .singleWhere((p) => p.action == 'save_knowledge_draft');
    final result = await repo.confirmKnowledgeDraft(main.id, draft);
    expect(result, contains('已保存知识页'));
    expect(await repo.confirmKnowledgeDraft(main.id, draft), result);
    expect(
      (await repo.listWikiPages()).where((p) => p.title == 'Versioned draft'),
      hasLength(1),
    );
    expect(
      (await repo.listChatDecisions(main.id))
          .where((r) => r.status == 'succeeded')
          .length,
      greaterThanOrEqualTo(3),
    );
  });
}
