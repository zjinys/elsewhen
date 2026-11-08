import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'chat_run_bridge_test.dart' show waitForRun;
import 'support/isolated_bridge.dart';

void main() {
  test('actual model requests and immutable bundles use independent instruction/reply/knowledge languages', () async {
    final repo = await createIsolatedBridge();
    await api.setRuntimeUiLocale(locale: 'en');
    final preferences = await api.getPromptLanguagePreferences();
    await api.updatePromptLanguagePreferences(
      instructionLocale: null,
      responseMode: 'fixed',
      responseLocale: 'zh-Hans',
      expectedGeneration: preferences.generation,
    );
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final firstArrived = Completer<void>();
    final release = Completer<void>();
    addTearDown(() {
      if (!release.isCompleted) release.complete();
    });
    final requests = <Map<String, dynamic>>[];
    server.listen((request) async {
      requests.add(
        jsonDecode(await utf8.decoder.bind(request).join())
            as Map<String, dynamic>,
      );
      if (!firstArrived.isCompleted) {
        firstArrived.complete();
        await release.future;
      }
      request.response.headers.contentType = ContentType(
        'text',
        'event-stream',
        charset: 'utf-8',
      );
      if (requests.length == 1) {
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {
                'index': 0,
                'delta': {
                  'tool_calls': [
                    {
                      'index': 0,
                      'id': 'read-rules',
                      'type': 'function',
                      'function': {'name': 'list_rules', 'arguments': '{}'},
                    },
                  ],
                },
                'finish_reason': 'tool_calls',
              },
            ],
          })}\n\ndata: [DONE]\n\n',
        );
      } else {
        request.response.write(
          'data: ${jsonEncode({
            'choices': [
              {
                'index': 0,
                'delta': {'content': 'A bounded explanation of the supplied question.'},
                'finish_reason': 'stop',
              },
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
    final conversation = await repo.createConversation(
      title: 'Language policy fixture',
    );
    final input = await repo.sendMessage(
      conversation.id,
      'user',
      '请用英文回答。Explain the difference between a record and an inference.',
    );
    final run = await api.startChatRun(
      conversationId: conversation.id,
      inputMessageId: input.id,
      idempotencyKey: 'language-1',
    );
    await firstArrived.future.timeout(const Duration(seconds: 10));
    final changed = await api.getPromptLanguagePreferences();
    await api.updatePromptLanguagePreferences(
      instructionLocale: 'zh-Hans',
      responseMode: 'fixed',
      responseLocale: 'zh-Hans',
      expectedGeneration: changed.generation,
    );
    await api.setRuntimeUiLocale(locale: 'zh-Hans');
    release.complete();
    final done = await waitForRun(
      conversation.id,
      run.id,
      (r) => r.status == 'completed' || r.status == 'failed',
    );
    expect(done.status, 'completed', reason: done.errorText);
    final bundles = await api.listPromptBundles(offset: 0);
    final before = bundles.singleWhere((b) => b.ownerId == run.id);
    expect(before.uiLocale, 'en');
    expect(before.instructionLocale, 'en');
    expect(before.knowledgeOutputLocale, 'zh-Hans');
    expect(jsonDecode(before.responsePolicyJson), containsPair('locale', 'en'));
    expect(
      jsonDecode(before.responsePolicyJson),
      containsPair('source', 'explicit_request'),
    );
    final chatIndex = before.keys.indexOf('chat.personal.policy');
    final summaryIndex = before.keys.indexOf('summary.policy');
    expect(before.resolvedLocales[chatIndex], 'en');
    expect(before.resolvedLocales[summaryIndex], 'zh-Hans');
    final system = (requests.first['messages'] as List)
        .where((m) => m['role'] == 'system')
        .map((m) => m['content'])
        .join('\n');
    expect(system, contains('You are Elsewhen:'));
    expect(system, contains('Output language: en.'));
    expect(system, contains('[工具调用]'));
    expect(requests.length, 2);
    final secondSystem = (requests[1]['messages'] as List)
        .where((m) => m['role'] == 'system')
        .map((m) => m['content'])
        .join('\n');
    expect(secondSystem, contains('You are Elsewhen:'));
    expect(secondSystem, contains('Output language: en.'));
    expect(
      (requests[1]['messages'] as List).any(
        (m) => m['role'] == 'tool' && m['tool_call_id'] == 'read-rules',
      ),
      isTrue,
    );
    final tools = (requests.first['tools'] as List)
        .map((t) => t['function'])
        .toList();
    expect(
      tools.singleWhere((t) => t['name'] == 'enable_tools')['description'],
      startsWith('Load one or more skill packages'),
    );
    final search = tools.singleWhere(
      (t) => t['name'] == 'search_knowledge_base',
    );
    expect(
      search['description'],
      startsWith('Search knowledge pages and personal event records'),
    );
    expect(
      search['parameters']['properties']['query']['description'],
      'Required search keywords',
    );
    expect(
      (requests.first['messages'] as List).any(
        (m) => m['role'] == 'user' && m['content'] == input.content,
      ),
      isTrue,
    );
    final secondInput = await repo.sendMessage(conversation.id, 'user', '继续');
    final next = await api.startChatRun(
      conversationId: conversation.id,
      inputMessageId: secondInput.id,
      idempotencyKey: 'language-2',
    );
    final nextDone = await waitForRun(
      conversation.id,
      next.id,
      (r) => r.status == 'completed' || r.status == 'failed',
    );
    expect(nextDone.status, 'completed', reason: nextDone.errorText);
    final after = (await api.listPromptBundles(offset: 0))
        .singleWhere((b) => b.ownerId == next.id);
    expect(after.uiLocale, 'zh-Hans');
    expect(after.instructionLocale, 'zh-Hans');
    expect(
      jsonDecode(after.responsePolicyJson),
      containsPair('locale', 'zh-Hans'),
    );
    final oldAgain = (await api.listPromptBundles(offset: 0))
        .singleWhere((b) => b.id == before.id);
    expect(oldAgain.revisionIds, before.revisionIds);
    expect(oldAgain.responsePolicyJson, before.responsePolicyJson);
    expect(
      (await repo.listMessages(conversation.id))
          .where((m) => m.isUser)
          .map((m) => m.content),
      [input.content, '继续'],
    );
  });
}
