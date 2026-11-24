import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

String _sql(String value) => "'${value.replaceAll("'", "''")}'";

api.ChatReviewRequest _request(api.ChatProposalDto proposal) =>
    api.ChatReviewRequest(
      kind: proposal.kind,
      proposalId: proposal.id,
      conversationId: proposal.conversationId,
      expectedVersion: proposal.version,
      approvalToken: proposal.approvalToken,
    );

void main() {
  test('real bridge manages isolated memories with exact-version approvals and no AI followup', () async {
    final repo = await createIsolatedBridge();
    expect(jsonDecode(await repo.listPersonalMemories('{}'))['total'], 0);
    final conversation = await repo.createConversation(title: '管理页隔离测试');
    const content = '我每周有六小时学习。';
    final message = await api.sendMessage(
      conversationId: conversation.id,
      role: 'user',
      content: content,
    );
    final page = await api.saveTopicPage(
      text: content,
      title: '学习时间',
      tags: [],
    );
    final bodyHash = sha256.convert(utf8.encode(page.contentMd)).toString();
    final basis = jsonEncode([
      {
        'kind': 'message',
        'id': message.id,
        'version': sha256.convert(utf8.encode(content)).toString(),
        'quote': content,
        'offset': 0,
      },
    ]);
    final databases = Directory(repo.databasePath!)
        .listSync(recursive: true)
        .whereType<File>()
        .where((file) => file.path.endsWith('/elsewhen.db'))
        .toList();
    expect(databases, hasLength(1));
    final seeded = await Process.run('sqlite3', [
      databases.single.path,
      "INSERT INTO personal_memory_claims(id,page_slug,body_hash,kind,subject,predicate,conversation_id,basis_json,updated_at,review_after) VALUES('bridge-memory',${_sql(page.slug)},${_sql(bodyHash)},'procedural','user','weekly-study',${_sql(conversation.id)},${_sql(basis)},'2026-11-23T00:00:00Z','2026-11-23T00:00:00Z');",
    ]);
    expect(seeded.exitCode, 0, reason: '${seeded.stderr}');
    final listed = jsonDecode(
      await repo.listPersonalMemories('{"state":"pending"}'),
    ) as Map;
    expect(listed['total'], 1);
    var detail =
        jsonDecode(await repo.getPersonalMemoryDetail('bridge-memory')) as Map;
    expect(detail['validity'], 'current');
    expect(detail['sources'][0]['quote'], content);
    Future<api.ChatProposalDto> prepare(Map<String, dynamic> args) =>
        repo.preparePersonalMemoryAction(
          claimId: 'bridge-memory',
          expectedVersion: detail['claim']['version'] as int,
          detailToken: detail['detail_token'] as String,
          argsJson: jsonEncode(args),
        );
    final confirm = await prepare({
      'operation': 'confirm',
      'scope': 'personal',
    });
    expect(
      jsonDecode(
        await repo.getPersonalMemoryDetail('bridge-memory'),
      )['claim']['state'],
      'pending',
    );
    final confirmed = await repo.decidePersonalMemoryAction(
      _request(confirm),
      accept: true,
    );
    expect(confirmed.status, 'succeeded', reason: confirmed.result);
    expect(
      (await repo.decidePersonalMemoryAction(
        _request(confirm),
        accept: true,
      )).id,
      confirmed.id,
    );
    detail =
        jsonDecode(await repo.getPersonalMemoryDetail('bridge-memory')) as Map;
    expect(detail['claim']['scope'], 'personal');
    final pin = await prepare({'operation': 'pin'});
    final excluded = await prepare({'operation': 'exclude'});
    final excludedResult = await repo.decidePersonalMemoryAction(
      _request(excluded),
      accept: true,
    );
    expect(excludedResult.status, 'succeeded', reason: excludedResult.result);
    var blocked = false;
    try {
      final stale = await repo.decidePersonalMemoryAction(
        _request(pin),
        accept: true,
      );
      blocked = stale.status != 'succeeded';
    } catch (_) {
      blocked = true;
    }
    expect(blocked, isTrue);
    detail =
        jsonDecode(await repo.getPersonalMemoryDetail('bridge-memory')) as Map;
    expect(detail['claim']['pinned'], false);
    expect(detail['claim']['excluded'], true);
    expect(detail['content'], isNotNull);
    final report = jsonDecode(
      await repo.inspectPersonalMemoryMaintenance(conversation.id),
    ) as Map;
    expect(report['automatic_mutation'], false);
    final inputs = await Process.run('sqlite3', [
      databases.single.path,
      "SELECT count(*) FROM messages WHERE conversation_id=${_sql(conversation.id)} AND role='user'; SELECT content FROM messages WHERE id=${_sql(message.id)};",
    ]);
    expect(inputs.exitCode, 0, reason: '${inputs.stderr}');
    expect((inputs.stdout as String).trim(), '1\n$content');
  });
}
