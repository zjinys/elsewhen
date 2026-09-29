import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

import 'support/isolated_bridge.dart';

void main() {
  test('真实桥接：承诺不能结束任务，问号接回原要求且不确认草稿', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    var mode = 'promise';
    var drafted = false;
    final requests = <Map<String, dynamic>>[];
    server.listen((request) async {
      requests.add(jsonDecode(await utf8.decoder.bind(request).join()));
      final Map<String, dynamic> message;
      if (mode == 'promise') {
        message = {'content': '我去处理，一会给你回复。'};
      } else if (mode == 'answer') {
        message = {'content': '这是保留稳定镜头的改进稿：固定机位，刺绣逐步形成。'};
      } else if (mode == 'draft' && !drafted) {
        drafted = true;
        message = {
          'content': '已整理出要点，先形成待确认草稿。',
          'tool_calls': [
            {
              'id': 'draft-call',
              'type': 'function',
              'function': {
                'name': 'save_knowledge_draft',
                'arguments': jsonEncode({
                  'title': '视频改进稿',
                  'content_md': '固定机位，刺绣逐步形成。',
                }),
              },
            },
          ],
        };
      } else {
        message = {'content': ''};
      }
      request.response.headers.contentType = ContentType.json;
      request.response.write(
        jsonEncode({
          'model': 'fixture',
          'choices': [
            {'message': message},
          ],
          'usage': {
            'prompt_tokens': 30,
            'completion_tokens': 10,
            'total_tokens': 40,
          },
        }),
      );
      await request.response.close();
    });
    await api.updateAiProviderConfig(
      baseUrl: 'http://127.0.0.1:${server.port}/v1',
      model: 'fixture',
      apiKey: 'test-key',
    );
    final conversation = await api.createConversation(title: '视频任务');
    Future<String> ask(String text) async {
      await api.sendMessage(
        conversationId: conversation.id,
        role: 'user',
        content: text,
      );
      return api.generateReply(conversationId: conversation.id);
    }

    const original = '改进我的视频 prompt，保留稳定镜头。';
    final stalled = await ask(original);
    expect(stalled, contains(original));
    expect(stalled, contains('还没有完成'));
    expect(stalled, isNot(contains('一会给你回复')));
    expect(stalled, isNot(contains('模型')));
    expect(requests, hasLength(3));
    expect(requests.last.containsKey('tools'), isFalse);

    mode = 'answer';
    final answer = await ask('？');
    expect(answer, contains('改进稿'));
    final context = (requests.last['messages'] as List)
        .map((m) => m['content'] as String)
        .join('\n');
    expect(context, contains('original_request'));
    expect(context, contains(original));

    mode = 'draft';
    final preview = await ask('把改进稿保存成知识草稿');
    expect(preview, contains('视频改进稿'));
    expect(preview, contains('尚未执行保存'));
    expect(await repo.listWikiPages(), isEmpty);
    expect(
      await api.listPendingActions(conversationId: conversation.id),
      hasLength(1),
    );

    mode = 'empty';
    final status = await ask('搞定了吗？');
    expect(status, contains('等待确认'));
    expect(
      await api.listPendingActions(conversationId: conversation.id),
      hasLength(1),
    );
    expect(await repo.listWikiPages(), isEmpty);

    final saved = await ask('好');
    expect(saved, contains('视频改进稿'));
    expect(saved, isNot(contains('还没有完成')));
    expect(
      await api.listPendingActions(conversationId: conversation.id),
      isEmpty,
    );
    expect(await repo.listWikiPages(), hasLength(1));
  });
}
