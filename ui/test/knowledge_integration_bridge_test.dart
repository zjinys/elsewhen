import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;

import 'support/isolated_bridge.dart';

void main() {
  test('真实桥接：自动跨资料主题、来源关系与内容检查', () async {
    final repo = await createIsolatedBridge();
    final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    addTearDown(() => server.close(force: true));
    var calls = 0;
    server.listen((request) async {
      calls++;
      final body = jsonDecode(await utf8.decoder.bind(request).join()) as Map;
      final prompt = (body['messages'] as List)[1]['content'] as String;
      final Object content;
      if (prompt.contains('整理共享主题')) {
        final inputs = jsonDecode(prompt.split('材料：').last) as List;
        final ids = inputs
            .expand((p) => p['snapshot_ids'] as List)
            .toSet()
            .toList();
        content = prompt.contains('模式 split')
            ? [
                for (var i = 0; i < ids.length; i++)
                  {
                    'title': '拆分主题 $i',
                    'content_md': '按阶段分析 IP 内容生产。',
                    'applicable_when': '制定 IP 计划',
                    'snapshot_ids': [ids[i]],
                    'event_ids': [],
                  },
              ]
            : [
                {
                  'title': '重新归纳的 IP 主题',
                  'content_md': '将阶段和定位结合。',
                  'applicable_when': 'IP 规划',
                  'snapshot_ids': ids,
                  'event_ids': [],
                },
              ];
      } else if (prompt.contains('跨资料主题整理和检查')) {
        final data = prompt.split('原料：').last.split('\n已有主题：');
        final sources = jsonDecode(data[0]) as List;
        final topics = jsonDecode(data[1]) as List;
        content = {
          'topics': [
            {
              'existing_slug': topics.isEmpty ? null : topics.first['slug'],
              'title': 'IP 定位与内容生产',
              'content_md': '先确定定位，再比较内容生产的频率和质量；两种建议需按阶段取舍。',
              'applicable_when': '制定 IP 定位和内容生产计划',
              'snapshot_ids': sources.map((s) => s['snapshot_id']).toList(),
            },
          ],
          'issues': [
            {
              'page_slug': sources.first['page_slug'],
              'kind': 'conflict',
              'description': '两份资料对内容频率的建议不同，需要核对各自适用阶段。',
              'evidence': sources
                  .take(2)
                  .map(
                    (s) => {
                      'snapshot_id': s['snapshot_id'],
                      'quote': s['excerpt'],
                    },
                  )
                  .toList(),
            },
          ],
        };
      } else {
        content = {
          'kind': 'method',
          'title': 'IP 内容安排方法',
          'content_md': '围绕明确定位安排内容，结合阶段选择频率。',
          'applicable_when': '制定 IP 内容计划',
          'reason': '依据提供的 IP 原文',
        };
      }
      request.response.headers.contentType = ContentType.json;
      request.response.write(
        jsonEncode({
          'model': 'fixture',
          'choices': [
            {
              'message': {'content': jsonEncode(content)},
              'finish_reason': 'stop',
            },
          ],
        }),
      );
      await request.response.close();
    });
    await api.updateAiProviderConfig(
      baseUrl: 'http://127.0.0.1:${server.port}/v1',
      model: 'fixture',
      apiKey: 'fixture-key',
    );
    final originals = <api.WikiPageDto>[];
    for (final (index, text) in [
      'IP 内容生产先确定定位，初期可提高发布频率积累反馈。',
      'IP 内容生产先确定定位，成熟阶段应减少频率提高深度。',
    ].indexed) {
      originals.add(
        await api.confirmKnowledgeSource(
          title: 'IP 内容生产 $index',
          contentMd: text,
          sourceUrl: 'https://example.com/ip-$index',
          sourceKind: 'webpage',
          tags: ['IP', '内容生产'],
        ),
      );
    }
    expect(await api.tickKnowledgeInsights(), 3);
    final topics = (await repo.listWikiPages())
        .where((p) => p.slug.startsWith('knowledge-topic/'))
        .toList();
    expect(topics, hasLength(1));
    final topic = topics.single;
    expect((await api.getWikiPage(slug: topic.slug))!.humanEditedAt, isNull);
    final details = await api.getKnowledgePageDetails(slug: topic.slug);
    expect(details.sourcePages, hasLength(2));
    expect(details.sources, hasLength(2));
    expect(details.metadata.strength, 'reference');
    expect(details.metadata.confirmedAt, isNull);
    for (final raw in originals) {
      expect((await repo.getWikiPage(raw.slug))!.contentMd, raw.contentMd);
      expect(
        (await api.getKnowledgePageDetails(slug: raw.slug)).outputPages
            .map((p) => p.slug),
        contains(topic.slug),
      );
    }
    final issues = await api.listKnowledgeIssues();
    expect(
      issues.any((i) => i.kind == 'conflict' && i.description.contains('待核对')),
      isTrue,
    );
    expect(
      (await api.listKnowledgeProposals()).every((p) => p.status == 'accepted'),
      isTrue,
    );
    await api.tickKnowledgeInsights();
    final before = calls;
    expect(await api.tickKnowledgeInsights(), 0);
    expect(calls, before);
    expect(
      (await repo.listWikiPages()).where(
        (p) => p.slug.startsWith('knowledge-topic/'),
      ),
      hasLength(1),
    );
    expect(
      (await api.listKnowledgeBackgroundRuns()).any(
        (r) => r.task == 'wiki-integration' && r.status == 'succeeded',
      ),
      isTrue,
    );
    // A source conflict can be resolved by a reviewed topic revision, with an
    // audit visible from both the original source and the resolving knowledge.
    final conflict = (await api.listKnowledgeIssues()).firstWhere(
      (i) => i.kind == 'conflict',
    );
    final targets = await api.listKnowledgeResolutionTargets(
      fingerprint: conflict.fingerprint,
    );
    expect(targets.map((p) => p.slug), contains(topic.slug));
    await api.saveWikiPageContent(
      slug: topic.slug,
      contentMd: 'IP 初期与成熟阶段分别采用不同内容频率。',
      reason: '人工区分适用阶段',
    );
    final revision = (await api.listKnowledgeRevisions(slug: topic.slug)).first;
    await api.resolveKnowledgeIssueWithRevision(
      fingerprint: conflict.fingerprint,
      slug: topic.slug,
      revisionId: revision.id,
      note: '按阶段分别保留两份材料的建议',
    );
    expect(
      (await api.listKnowledgeReviewHistory(
        slug: conflict.pageSlug,
        offset: 0,
      )).any((r) => r.revisionId == revision.id && r.note != null),
      isTrue,
    );
    final derivative = await api.createWikiDerivative(
      basedOnSlug: topic.slug,
      contentType: '摘要',
      title: '阶段摘要',
      contentMd: '按阶段安排频率',
    );
    await api.saveWikiPageContent(
      slug: topic.slug,
      contentMd: 'IP 初期需要结合可持续投入时间，成熟阶段强调深度。',
      reason: '人工纠正',
    );
    expect(
      (await api.listKnowledgeIssues()).any(
        (i) => i.pageSlug == derivative.slug && i.kind == 'upstream_changed',
      ),
      isTrue,
    );
    expect(
      (await api.listKnowledgeWorkQueue(
        offset: 0,
        status: 'waiting',
      )).items.any((i) => i.pageSlug == derivative.slug),
      isTrue,
    );
    final reviewId = await api.proposeKnowledgePage(
      slug: derivative.slug,
      kind: 'revision',
    );
    await api.resolveKnowledgeProposal(id: reviewId, accept: true);
    expect(
      (await api.getKnowledgePageDetails(slug: derivative.slug)).issues
          .any((i) => i.kind == 'upstream_changed'),
      isFalse,
    );
    final splitId = await api.prepareTopicOrganization(
      slugs: [topic.slug],
      mode: 'split',
    );
    expect((await api.getWikiPage(slug: topic.slug))!.status, 'active');
    final plan = (await api.listTopicOrganizations(slug: topic.slug)).single;
    expect(plan.id, splitId);
    expect(plan.topics, hasLength(2));
    final splitPages = await api.resolveTopicOrganization(
      id: splitId,
      accept: true,
    );
    expect(splitPages, hasLength(2));
    expect((await api.getWikiPage(slug: topic.slug))!.status, 'archived');
    final mergeId = await api.prepareTopicOrganization(
      slugs: splitPages,
      mode: 'merge',
    );
    final merged = await api.resolveTopicOrganization(
      id: mergeId,
      accept: true,
    );
    expect(merged, hasLength(1));
    final history = await api.topicOrganizationHistory(
      slug: merged.single,
      offset: 0,
    );
    expect(history.first.status, 'accepted');

    expect(
      (await api.getKnowledgePageDetails(slug: merged.single)).sources,
      hasLength(2),
    );
    await api.setWikiOpinion(slug: originals.last.slug, opinion: 'reject');
    final choices = await api.listKnowledgeRepairSources(slug: merged.single);
    expect(choices.where((s) => s.selected && !s.eligible), hasLength(1));
    final repair = await api.prepareKnowledgeSourceRepair(
      slug: merged.single,
      snapshotIds: choices
          .where((s) => s.eligible)
          .map((s) => s.snapshotId)
          .toList(),
    );
    expect(
      (await api.getKnowledgePageDetails(slug: merged.single)).sources,
      hasLength(2),
    );
    final batch = await api.resolveKnowledgeBatch(
      ids: [repair, 'missing-proposal'],
      accept: true,
    );
    expect(batch.map((r) => r.success).toList(), [true, false]);
    await expectLater(
      api.undoTopicOrganization(id: mergeId),
      throwsA(isA<Exception>()),
    );
    await api.setKnowledgeReadingState(
      slug: originals.first.slug,
      state: 'valuable',
    );
    expect(
      await api.knowledgeReadingState(slug: originals.first.slug),
      'valuable',
    );
    final library = await api.browseKnowledge(
      query: '',
      state: 'valuable',
      offset: 0,
    );
    expect(library.items.any((p) => p.slug == originals.first.slug), isTrue);
    final artifact = await api.createWikiDerivative(
      basedOnSlug: originals.first.slug,
      contentType: '脚本',
      title: '试用脚本',
      contentMd: '根据 IP 定位安排内容。',
    );
    final version = (await api.listArtifactVersions(
      slug: originals.first.slug,
      offset: 0,
    )).firstWhere((p) => p.slug == artifact.slug);
    expect(version.version, 1);
    final artifactRevision = (await api.listKnowledgeRevisions(
      slug: artifact.slug,
    )).first;
    await api.adoptArtifactVersion(
      slug: artifact.slug,
      revisionId: artifactRevision.id,
      adopt: true,
    );
    expect(
      (await api.listArtifactVersions(
        slug: originals.first.slug,
        offset: 0,
      )).any((p) => p.slug == artifact.slug && p.adopted),
      isTrue,
    );
    final conversation = await api.createConversation(title: '建议反馈');
    final msg = await api.sendMessage(
      conversationId: conversation.id,
      role: 'assistant',
      content: '建议先核对定位。',
    );
    await api.saveSuggestionFeedback(
      messageId: msg.id,
      decision: 'rewritten',
      suggestion: '建议先核对定位。',
      rewrite: '先确认面向哪类受众。',
    );
    expect(
      (await api.getSuggestionFeedback(messageId: msg.id))!.rewrite,
      '先确认面向哪类受众。',
    );

    expect(
      (await api.getKnowledgePageDetails(slug: merged.single)).sources,
      hasLength(1),
    );
    expect(
      (await api.listKnowledgeReviewHistory(
        slug: merged.single,
        offset: 0,
      )).any(
        (r) =>
            r.id == repair && r.action == 'accepted' && r.resultContent != null,
      ),
      isTrue,
    );
    expect(
      (await api.getWikiPage(slug: originals.last.slug))!.contentMd,
      originals.last.contentMd,
    );
  });
}
