import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;

/// A replaceable boundary keeps widget tests off the real FFI/database.
class KnowledgeRepository {
  Future<api.KnowledgePageDetails> details(String slug) =>
      api.getKnowledgePageDetails(slug: slug);
  Future<List<api.KnowledgeProposal>> proposals() =>
      api.listKnowledgeProposals();
  Future<List<api.KnowledgeIssue>> issues() => api.listKnowledgeIssues();
  Future<List<api.KnowledgeBackgroundRunDto>> runs() =>
      api.listKnowledgeBackgroundRuns();
  Future<List<api.KnowledgeRevisionDto>> revisions(String slug) =>
      api.listKnowledgeRevisions(slug: slug);
  Future<api.SourceSnapshot?> snapshot(String id) =>
      api.getKnowledgeSourceSnapshot(id: id);
  Future<String> propose(String slug, String kind) =>
      api.proposeKnowledgePage(slug: slug, kind: kind);
  Future<api.WikiPageDto?> resolve(String id, bool accept) =>
      api.resolveKnowledgeProposal(id: id, accept: accept);
  Future<void> metadata(String slug, String applicableWhen, String strength) =>
      api.updateKnowledgeMetadata(
        slug: slug,
        applicableWhen: applicableWhen,
        strength: strength,
      );
  Future<void> dismiss(String fingerprint) =>
      api.dismissKnowledgeIssue(fingerprint: fingerprint);
  Future<List<api.KnowledgeCitation>> citations(String messageId) =>
      api.getMessageKnowledgeCitations(messageId: messageId);
  Future<api.SourceUpdatePreview> preview(String url, String content) =>
      api.previewKnowledgeSource(sourceUrl: url, contentMd: content);
  Future<api.WikiPageDto> confirmSource({
    required String title,
    required String contentMd,
    required String sourceUrl,
    required String sourceKind,
    required List<String> tags,
    String? expectedSnapshotId,
  }) => api.confirmKnowledgeSource(
    title: title,
    contentMd: contentMd,
    sourceUrl: sourceUrl,
    sourceKind: sourceKind,
    tags: tags,
    expectedSnapshotId: expectedSnapshotId,
  );
}

final knowledgeRepositoryProvider = Provider<KnowledgeRepository>(
  (ref) => KnowledgeRepository(),
);
final knowledgeDetailsProvider = FutureProvider.autoDispose
    .family<api.KnowledgePageDetails, String>(
      (ref, slug) => ref.read(knowledgeRepositoryProvider).details(slug),
    );
final knowledgeProposalsProvider =
    FutureProvider.autoDispose<List<api.KnowledgeProposal>>(
      (ref) => ref.read(knowledgeRepositoryProvider).proposals(),
    );
final knowledgeIssuesProvider =
    FutureProvider.autoDispose<List<api.KnowledgeIssue>>(
      (ref) => ref.read(knowledgeRepositoryProvider).issues(),
    );
final knowledgeRunsProvider =
    FutureProvider.autoDispose<List<api.KnowledgeBackgroundRunDto>>(
      (ref) => ref.read(knowledgeRepositoryProvider).runs(),
    );
final knowledgeRevisionsProvider = FutureProvider.autoDispose
    .family<List<api.KnowledgeRevisionDto>, String>(
      (ref, slug) => ref.read(knowledgeRepositoryProvider).revisions(slug),
    );
