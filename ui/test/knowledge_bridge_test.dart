import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

import 'support/isolated_bridge.dart';

void main() {
  test('真实桥接：来源更新预览、过期确认与精确历史版本', () async {
    final repo = await createIsolatedBridge();
    const url = 'https://example.com/knowledge';
    final initial = await api.previewKnowledgeSource(
      sourceUrl: url,
      contentMd: '旧原文',
    );
    expect(initial.existingSlug, isNull);
    final page = await api.confirmKnowledgeSource(
      title: '事务材料',
      contentMd: '旧原文',
      sourceUrl: url,
      sourceKind: 'webpage',
      tags: [],
    );
    final preview = await api.previewKnowledgeSource(
      sourceUrl: '$url#section',
      contentMd: '新原文',
    );
    expect(preview.existingSlug, page.slug);
    expect(preview.changed, isTrue);
    // 预览不写库，用户取消无需补偿。
    expect((await repo.getWikiPage(page.slug))!.contentMd, '旧原文');
    await api.confirmKnowledgeSource(
      title: '事务材料',
      contentMd: '新原文',
      sourceUrl: url,
      sourceKind: 'webpage',
      tags: [],
      expectedSnapshotId: preview.previousSnapshotId,
    );
    await expectLater(
      api.confirmKnowledgeSource(
        title: '过期保存',
        contentMd: '另一个版本',
        sourceUrl: url,
        sourceKind: 'webpage',
        tags: [],
        expectedSnapshotId: preview.previousSnapshotId,
      ),
      throwsA(anything),
    );
    final details = await api.getKnowledgePageDetails(slug: page.slug);
    expect(details.history, hasLength(2));
    expect(details.sources.single.contentMd, '新原文');
    expect(
      (await api.getKnowledgeSourceSnapshot(id: preview.previousSnapshotId!))!
          .contentMd,
      '旧原文',
    );
    expect(await repo.listWikiPages(), hasLength(1));
    final derivative = await repo.createWikiDerivative(
      basedOnSlug: page.slug,
      contentType: 'summary',
      title: '整理稿',
      contentMd: '整理观点',
    );
    expect(
      (await api.getKnowledgePageDetails(slug: derivative.slug))
          .sources
          .single
          .id,
      details.sources.single.id,
    );
    // Exercise real FRB + HTTP provider + proposal confirmation using only a
    // loopback fixture and the isolated database, never a configured account.
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    final requests = <String>[];
    server.listen((request) async {
      requests.add(await utf8.decoder.bind(request).join());
      request.response.headers.contentType = ContentType.json;
      request.response.write(
        jsonEncode({
          'id': 'fixture',
          'object': 'chat.completion',
          'model': 'fixture',
          'choices': [
            {
              'index': 0,
              'message': {
                'role': 'assistant',
                'content': jsonEncode({
                  'kind': 'method',
                  'title': '事务方法',
                  'content_md': '多步写入在同一个事务中完成。',
                  'applicable_when': '执行多步数据库写入',
                  'reason': '由导入原文整理',
                }),
              },
              'finish_reason': 'stop',
            },
          ],
          'usage': {
            'prompt_tokens': 25,
            'completion_tokens': 25,
            'total_tokens': 50,
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
    final proposalId = await api.proposeKnowledgePage(
      slug: page.slug,
      kind: 'method',
    );
    final proposals = await api.listKnowledgeProposals();
    expect(proposals.single.id, proposalId);
    expect(proposals.single.status, 'pending');
    expect(proposals.single.snapshotIds, [details.sources.single.id]);
    expect(requests.single, contains('新原文'));
    expect(await repo.getWikiPage(proposals.single.targetSlug), isNull);
    final method = await api.resolveKnowledgeProposal(
      id: proposalId,
      accept: true,
    );
    expect(method!.kind, 'method');
    expect(
      (await api.getKnowledgePageDetails(slug: page.slug)).outputPages
          .map((p) => p.slug),
      contains(method.slug),
    );
    expect(
      (await api.getKnowledgePageDetails(slug: method.slug))
          .sourcePages
          .single
          .slug,
      page.slug,
    );
    expect(
      (await api.getKnowledgePageDetails(slug: method.slug)).metadata.strength,
      'reference',
    );
    await api.updateKnowledgeMetadata(
      slug: method.slug,
      applicableWhen: '执行多步数据库写入',
      strength: 'rule',
    );
    expect(
      (await api.getKnowledgePageDetails(slug: method.slug)).metadata.strength,
      'rule',
    );
    await api.setWikiOpinion(slug: page.slug, opinion: 'reject');
    expect(
      (await api.getKnowledgePageDetails(slug: derivative.slug)).issues
          .any((i) => i.kind == 'rejected_source'),
      isTrue,
    );
    final automaticSource = await api.confirmKnowledgeSource(
      title: '自动整理原料',
      contentMd: '同一事务里的多步操作保持原子性。',
      sourceUrl: 'https://example.com/automatic',
      sourceKind: 'webpage',
      tags: [],
    );
    final before = requests.length;
    expect(await api.tickKnowledgeInsights(), 1);
    final automaticDetails = await api.getKnowledgePageDetails(
      slug: automaticSource.slug,
    );
    final automaticPage = automaticDetails.outputPages.single;
    expect(automaticPage.humanEditedAt, isNull);
    expect(automaticPage.kind, 'method');
    expect(
      (await api.getKnowledgePageDetails(slug: automaticPage.slug))
          .metadata
          .strength,
      'reference',
    );
    expect(
      (await repo.getWikiPage(automaticSource.slug))!.contentMd,
      automaticSource.contentMd,
    );
    expect(await api.tickKnowledgeInsights(), 0);
    expect(requests.length, before + 1);
    expect(
      (await api.listKnowledgeBackgroundRuns()).any(
        (r) => r.task == 'source-compilation' && r.status == 'succeeded',
      ),
      isTrue,
    );
  });
}
