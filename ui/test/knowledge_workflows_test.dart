import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/providers/knowledge_provider.dart';
import 'package:elsewhen_ui/widgets/knowledge_workflows.dart';
import 'package:elsewhen_ui/widgets/suggestion_feedback.dart';
import 'package:elsewhen_ui/widgets/artifact_versions.dart';

import 'knowledge_flow_test.dart' show compiledPage;

class WorkflowFake extends KnowledgeRepository {
  @override
  Future<List<api.BatchReviewResult>> resolveBatch(
    List<String> ids,
    bool accept,
  ) async {
    batch = ids;
    return [
      for (final id in ids)
        api.BatchReviewResult(id: id, success: true, detail: '已采纳'),
    ];
  }

  String readingValue = 'unread';
  String? decision, adopted;
  List<String>? batch;
  final offsets = <int>[];
  final opened = <String>[];
  @override
  Future<String> reading(String slug) async => readingValue;
  @override
  Future<void> setReading(String slug, String state) async {
    readingValue = state;
  }

  @override
  Future<api.LibraryPage> browse(
    String query,
    String? area,
    String? kind,
    String? tag,
    String? state,
    int offset,
  ) async {
    offsets.add(offset);
    return api.LibraryPage(
      items: [
        api.LibraryEntry(
          slug: 'page-$offset',
          title: '页面 $offset',
          summary: '摘要',
          kind: 'source',
          area: 'imported',
          readingState: state ?? 'unread',
        ),
      ],
      hasMore: offset == 0,
    );
  }

  @override
  Future<api.SuggestionFeedback?> feedback(String message) async => null;
  @override
  Future<void> saveFeedback(
    String message,
    String value,
    String suggestion,
    String? rewrite,
  ) async {
    decision = value;
  }

  @override
  Future<List<api.ArtifactVersion>> versions(String slug, int offset) async => [
    for (var i = 1; i <= 2; i++)
      api.ArtifactVersion(
        slug: 'v$i',
        title: '版本 $i',
        contentType: '脚本',
        version: i,
        createdAt: '2026-09-29',
        adopted: adopted == 'v$i',
      ),
  ];
  @override
  Future<api.KnowledgeRevisionDto> artifact(
    String slug,
    String? revision,
  ) async {
    opened.add(slug);
    return api.KnowledgeRevisionDto(
      id: revision ?? 'revision-$slug',
      contentMd: '正文 $slug',
      reason: '测试',
      createdAt: '2026-09-29',
    );
  }

  @override
  Future<api.WikiPageDto?> page(String slug) async {
    opened.add(slug);
    return compiledPage;
  }

  @override
  Future<List<api.KnowledgeRevisionDto>> revisions(String slug) async => [
    api.KnowledgeRevisionDto(
      id: 'revision-$slug',
      contentMd: '正文 $slug',
      reason: '测试',
      createdAt: '2026-09-29',
    ),
  ];
  @override
  Future<void> adopt(String slug, String revision, bool value) async {
    adopted = value ? slug : null;
  }
}

Future<void> mount(
  WidgetTester tester,
  WorkflowFake fake,
  Widget child, {
  bool scroll = true,
}) async {
  await tester.binding.setSurfaceSize(const Size(380, 1200));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    ProviderScope(
      overrides: [knowledgeRepositoryProvider.overrideWithValue(fake)],
      child: MaterialApp(
        home: Scaffold(
          body: scroll ? SingleChildScrollView(child: child) : child,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('批量审阅识别方法页修订并提交所选项', (tester) async {
    final fake = WorkflowFake();
    const p = api.KnowledgeProposal(
      id: 'r',
      pageId: 'page',
      baseHash: 'basis',
      targetSlug: 'method/one',
      kind: 'method',
      title: '方法修订',
      contentMd: '正文',
      applicableWhen: '条件',
      snapshotIds: [],
      eventIds: [],
      reason: '修订',
      status: 'pending',
      createdAt: '2026-09-29',
    );
    await mount(
      tester,
      fake,
      KnowledgeBatchReview(
        proposals: const [p],
        preview: (p) => Text(p.contentMd),
      ),
    );
    await tester.tap(find.text('批量审阅修订'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('方法修订'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('确认采纳 1 项'));
    await tester.pumpAndSettle();
    expect(fake.batch, ['r']);
    expect(tester.takeException(), isNull);
  });
  testWidgets('阅读状态需要显式选择且可重新标记', (tester) async {
    final fake = WorkflowFake();
    await mount(tester, fake, const KnowledgeReadingControl(slug: 's'));
    expect(fake.readingValue, 'unread');
    await tester.tap(find.text('待读'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('已读').last);
    await tester.pumpAndSettle();
    expect(fake.readingValue, 'read');
    expect(tester.takeException(), isNull);
  });
  testWidgets('浏览分页继续加载，不从主列表读全部正文', (tester) async {
    final fake = WorkflowFake();
    await mount(tester, fake, const KnowledgeLibrary(), scroll: false);
    expect(find.text('页面 0'), findsOneWidget);
    await tester.tap(find.text('加载更多'));
    await tester.pumpAndSettle();
    expect(fake.offsets, [0, 1]);
    expect(find.text('页面 1'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets('建议反馈展开后才加载并支持撤回', (tester) async {
    final fake = WorkflowFake();
    await mount(
      tester,
      fake,
      const SuggestionFeedbackControl(messageId: 'm', content: '建议先备份'),
    );
    await tester.tap(find.text('反馈这条建议'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('忽略'));
    await tester.pumpAndSettle();
    expect(fake.decision, 'ignored');
    await tester.tap(find.text('撤回反馈'));
    await tester.pumpAndSettle();
    expect(fake.decision, 'cleared');
    expect(tester.takeException(), isNull);
  });
  testWidgets('产物按需加载正文，可在窄屏比较两个版本并采用', (tester) async {
    final fake = WorkflowFake();
    await mount(tester, fake, const ArtifactVersions(slug: 's'));
    expect(fake.opened, isEmpty);
    await tester.tap(find.text('版本 1 · v1'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('加入比较').first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('版本 2 · v2'));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('加入比较').last);
    await tester.tap(find.text('加入比较').last);
    await tester.pumpAndSettle();
    expect(find.text('版本比较'), findsOneWidget);
    await tester.ensureVisible(find.text('采用这个版本').last);
    await tester.tap(find.text('采用这个版本').last);
    await tester.pumpAndSettle();
    expect(fake.adopted, 'v2');
    expect(tester.takeException(), isNull);
  });
}
