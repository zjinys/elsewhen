import 'dart:convert';
import 'dart:io';

import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

// Age only synthetic jobs in the helper's isolated directory; every actual
// queue transition and progress query uses the production bridge.
Future<void> ageJobs(RustBridgeRepository repo) async {
  final files = Directory(repo.databasePath!)
      .listSync(recursive: true)
      .whereType<File>()
      .where((f) => f.path.endsWith('/elsewhen.db'))
      .toList();
  expect(files, hasLength(1));
  final result = await Process.run('sqlite3', [
    files.single.path,
    "UPDATE knowledge_digest_jobs SET available_at='2000-01-01T00:00:00Z';UPDATE event_reading_tasks SET available_at=0;UPDATE event_reading_nodes SET available_at=0 WHERE status='pending';",
  ]);
  expect(result.exitCode, 0, reason: '${result.stderr}');
}

void main() {
  test(
    'real bridge: bounded reading, retry progress and original final sources',
    () async {
      final repo = await createIsolatedBridge();
      final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => server.close(force: true));
      var calls = 0;
      var fail = true;
      server.listen((request) async {
        final payload = jsonDecode(
          await utf8.decoder.bind(request).join(),
        ) as Map<String, dynamic>;
        final user =
            ((payload['messages'] as List).last as Map)['content'] as String;
        calls++;
        Object output;
        if (fail) {
          fail = false;
          output = '{invalid json';
        } else if (user.contains('[1] ')) {
          expect(user, contains('verified_original_quotes'));
          output = [
            {
              'kind': 'topic',
              'slug': 'topic/long-event',
              'title': 'Long event',
              'content': 'START 17; MIDDLE not approved; TAIL 2026-12-01',
              'source_event_ids': ['1'],
            },
          ];
        } else {
          final input = jsonDecode(
            user.substring(user.indexOf('\n') + 1),
          ) as Map<String, dynamic>;
          if (input['text'] case final String text) {
            final start = input['start'] as int;
            final quote = String.fromCharCodes(text.runes.take(20));
            output = {
              'summary': 'Generated facts: START 17; MIDDLE not approved; TAIL 2026-12-01',
              'quotes': [
                {
                  'start': start,
                  'end': start + quote.runes.length,
                  'text': quote,
                },
              ],
            };
          } else {
            output = {
              'summary': 'Generated merged facts: START 17; MIDDLE not approved; TAIL 2026-12-01',
              'quotes': (input['children'] as List)
                  .expand((c) => c['reading']['quotes'] as List)
                  .take(8)
                  .toList(),
            };
          }
        }
        request.response.headers.contentType = ContentType.json;
        request.response.write(
          jsonEncode({
            'id': 'synthetic',
            'model': 'unknown-byte-budget-model',
            'choices': [
              {
                'index': 0,
                'message': {
                  'role': 'assistant',
                  'content': output is String ? output : jsonEncode(output),
                },
                'finish_reason': 'stop',
              },
            ],
            'usage': {
              'prompt_tokens': 10,
              'completion_tokens': 10,
              'total_tokens': 20,
            },
          }),
        );
        await request.response.close();
      });
      await repo.saveAiProviderConfig(
        api.AiProviderConfigDto(
          id: '',
          name: 'synthetic reading',
          providerType: 'openai-compatible',
          baseUrl: 'http://127.0.0.1:${server.port}/v1',
          model: 'unknown-byte-budget-model',
          apiKeySource: '',
          hasApiKey: true,
          apiKey: 'synthetic',
          isActive: true,
          temperature: 0,
          maxTokens: 1024,
          contextWindow: 6500,
        ),
      );
      final original =
          'START 17\n${List.filled(300, '中文🙂 not approved 42 `code`\n').join()}MIDDLE not approved\n${List.filled(300, 'quoted text 2026-11-09\n').join()}TAIL 2026-12-01';
      final event = await repo.recordEvent(original);
      await repo.setEventRecordability(event.id, true);
      await ageJobs(repo);
      expect(
        await api.triggerKnowledgeDigest(),
        isA<api.KnowledgeDigestTickResult_Reading>(),
      );
      expect(calls, 0);
      var job = (await repo.listKnowledgeDigestJobs()).single;
      expect(job.attempts, 0);
      expect(job.reading!.readChars, 0);
      await api.triggerKnowledgeDigest();
      job = (await repo.listKnowledgeDigestJobs()).single;
      expect(job.reading!.error, isNotNull);
      expect(job.status, 'pending');
      expect(job.attempts, 0);
      await ageJobs(repo);
      await api.triggerKnowledgeDigest();
      final beforeReopen =
          (await repo.listKnowledgeDigestJobs()).single.reading!;
      await reinitializeIsolatedBridge(repo);
      final afterReopen =
          (await repo.listKnowledgeDigestJobs()).single.reading!;
      expect(afterReopen.readChars, beforeReopen.readChars);
      expect(afterReopen.completedLeaves, beforeReopen.completedLeaves);
      var processed = false;
      for (var i = 0; i < 80; i++) {
        final before = calls;
        final result = await api.triggerKnowledgeDigest();
        expect(calls - before, lessThanOrEqualTo(1));
        job = (await repo.listKnowledgeDigestJobs()).single;
        if (result is api.KnowledgeDigestTickResult_Processed) {
          processed = true;
          break;
        }
        expect(job.status, isNot('succeeded'));
        expect(await repo.getWikiPage('topic/long-event'), isNull);
      }
      expect(processed, isTrue);
      expect(job.reading!.phase, 'succeeded');
      expect(job.reading!.readChars, original.runes.length);
      expect((await repo.getWikiPage('topic/long-event'))!.sourceEventIds, [
        event.id,
      ]);
      expect((await repo.listEvents()).single.rawText, original);
      expect(
        await api.triggerKnowledgeDigest(),
        isA<api.KnowledgeDigestTickResult_Idle>(),
      );
    },
    skip: Platform.isLinux
        ? false
        : 'Requires sqlite3 CLI to age isolated synthetic jobs',
  );
}
