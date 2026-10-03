import 'dart:convert';
import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'support/isolated_bridge.dart';
import 'chat_run_bridge_test.dart' show waitForRun;

void main() {
  test('wiki material identity, bounded selection and stale input survive the real bridge', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final requests = <Map<String,dynamic>>[];
    server.listen((request) async {
      final body = jsonDecode(await utf8.decoder.bind(request).join()) as Map<String,dynamic>;
      requests.add(body);
      request.response.headers.contentType = ContentType('text','event-stream',charset:'utf-8');
      request.response.write('data: ${jsonEncode({'choices':[{'index':0,'delta':{'content':'所选材料的局部回答。'}}]})}\n\n');
      request.response.write('data: ${jsonEncode({'choices':[{'index':0,'delta':{},'finish_reason':'stop'}]})}\n\ndata: [DONE]\n\n');
      await request.response.close();
    });
    await api.updateAiProviderConfig(baseUrl: 'http://127.0.0.1:${server.port}/v1',model:'gpt-4o',apiKey:'synthetic');
    final page = await repo.saveTextPage(text:'前文😀\n选中段落\n${'不相关材料。' * 1400}\n尾部',title:'page task bridge',tags:[]);
    final conv = await api.ensureWikiPageChat(pageSlug:page.slug);
    final material = await api.getWikiChatMaterial(pageSlug:page.slug);
    final start = material.contentMd.substring(0,material.contentMd.indexOf('选中段落')).runes.length;
    final task = jsonEncode({'kind':'local_question','pages':[jsonDecode(material.basisJson)],'start_char':start,'end_char':start+4});
    final input = await api.submitWikiChatInput(conversationId:conv.id,content:'只解释选择的文字，不需要检索其他资料。',idempotencyKey:'page-task-input',taskJson:task);
    final again = await api.submitWikiChatInput(conversationId:conv.id,content:input.content,idempotencyKey:'page-task-input',taskJson:task);
    expect(again.id,input.id);
    final run = await api.startChatRun(conversationId:conv.id,inputMessageId:input.id,idempotencyKey:'page-task-run');
    final done = await waitForRun(conv.id,run.id,(r)=>!{'preparing','running','cancelling'}.contains(r.status));
    expect(done.status,'completed');
    final sent = (requests.single['messages'] as List).map((m)=>m['content'] ?? '').join('\n');
    expect(sent,contains('选中段落'));
    expect(sent,contains('用户选区原文'));
    expect(sent, isNot(contains('不相关材料。' * 100)));
    await api.setWikiOpinion(slug:page.slug,opinion:'reject');
    await expectLater(api.submitWikiChatInput(conversationId:conv.id,content:'旧版本新问题',idempotencyKey:'stale',taskJson:task),throwsA(anything));
    final repeated = await api.submitWikiChatInput(conversationId:conv.id,content:input.content,idempotencyKey:'page-task-input',taskJson:task);
    expect(repeated.id,input.id);
    expect((await repo.listMessages(conv.id)).where((m)=>m.role.name=='user').length,1);
  });
}
