import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/bridge/generated.dart/api/chat_runs.dart' as runs;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  test('streamed search and knowledge drafts require explicit review in both entrances', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final replies = <Map<String, Object?>>[];
    var calls = 0;
    server.listen((request) async {
      await utf8.decoder.bind(request).join();
      request.response.bufferOutput = false;
      request.response.headers.contentType = ContentType(
        'text',
        'event-stream',
        charset: 'utf-8',
      );
      if (replies.isEmpty) {
        request.response.statusCode = 500;
      } else {
        final delta = replies.removeAt(0);
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {'index': 0, 'delta': delta, 'finish_reason': delta.containsKey('tool_calls') ? 'tool_calls' : 'stop'},
            ],
          })}\n\ndata: [DONE]\n\n',
        );
      }
      await request.response.close();
    });
    await api.updateAiProviderConfig(
      baseUrl: 'http://127.0.0.1:${server.port}/v1',
      model: 'gpt-4o',
      apiKey: 'synthetic',
    );
    final main = await repo.createConversation(title: 'review run main');
    final source = await repo.saveTextPage(
      text: '测试资料：实践后记录观察。',
      title: 'review source',
      tags: [],
    );
    final wiki = await api.ensureWikiPageChat(pageSlug: source.slug);
    void tool(String name, Map<String, Object?> arguments) {
      replies.add({
        'tool_calls': [
          {
            'index': 0,
            'id': 'call-${calls++}',
            'type': 'function',
            'function': {'name': name, 'arguments': jsonEncode(arguments)},
          },
        ],
      });
    }

    for (final conversation in [main.id, wiki.id]) {
      for (final accept in [false, true]) {
        final title = '审阅样例-$conversation-$accept';
        tool('search_knowledge_base', {'query': '测试资料'});
        tool('save_knowledge_draft', {
          'title': title,
          'content_md': '实践后记录观察，并区分事实与假设。',
        });
        replies.add({'content': '草稿已准备好，请在审阅卡选择具体内容。'});
        final text = '查询测试资料后，整理一页名为$title 的知识草稿';
        final input = conversation == main.id
            ? await repo.submitConversationInput(
                conversation,
                text,
                idempotencyKey: title,
              )
            : await repo.sendMessage(conversation, 'user', text);
        final run = await runs.startChatRun(
          conversationId: conversation,
          inputMessageId: input.id,
          idempotencyKey: 'run-$title',
        );
        runs.ChatRunDto? finished;
        final until = DateTime.now().add(const Duration(seconds: 10));
        while (DateTime.now().isBefore(until)) {
          finished = await runs.getChatRun(
            conversationId: conversation,
            runId: run.id,
          );
          if (finished != null &&
              ![
                'preparing',
                'running',
                'cancelling',
              ].contains(finished.status)) {
            break;
          }
          await Future<void>.delayed(const Duration(milliseconds: 25));
        }
        expect(finished!.status, 'completed', reason: finished.errorText);
        expect(replies, isEmpty);
        expect(
          (await repo.listWikiPages()).where((p) => p.title == title),
          isEmpty,
        );
        final proposal = (await repo.listChatProposals(conversation)).single;
        final request = api.ChatReviewRequest(
          kind: proposal.kind,
          proposalId: proposal.id,
          conversationId: conversation,
          expectedVersion: proposal.version,
          approvalToken: proposal.approvalToken,
        );
        final decision = await repo.decideChatProposal(request, accept: accept);
        expect(decision.status, accept ? 'succeeded' : 'declined');
        expect(
          (await repo.listWikiPages()).where((p) => p.title == title),
          hasLength(accept ? 1 : 0),
        );
        if (accept) {
          final repeated = await repo.decideChatProposal(request, accept: true);
          expect(repeated.result, decision.result);
          expect(
            (await repo.listWikiPages()).where((p) => p.title == title),
            hasLength(1),
          );
        }
        final events = await runs.listChatRunEvents(
          conversationId: conversation,
          runId: run.id,
          afterSequence: 0,
          limit: 100,
        );
        final results = events.events
            .where((e) => e.kind == 'tool_result')
            .map((e) => jsonDecode(e.payloadJson) as Map<String, dynamic>);
        expect(
          results.map((r) => r['outcome']),
          containsAll(['read', 'proposal_ready']),
        );
        expect(results.map((r) => r['outcome']), isNot(contains('executed')));
      }
    }
    expect((await repo.getWikiPage(source.slug))!.contentMd, '测试资料：实践后记录观察。');
  });
}
