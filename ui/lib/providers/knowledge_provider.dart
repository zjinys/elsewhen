import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show Int64List;
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';

/// A replaceable boundary keeps widget tests off the real FFI/database.
class KnowledgeRepository {
  Future<api.KnowledgeRevisionDto> artifact(String slug, String? revision) =>
      api.readArtifactVersion(slug: slug, revisionId: revision);

  Future<api.LibraryPage> browse(
    String query,
    String? area,
    String? kind,
    String? tag,
    String? state,
    int offset,
  ) => api.browseKnowledge(
    query: query,
    area: area,
    kind: kind,
    tag: tag,
    state: state,
    offset: offset,
  );
  Future<String> reading(String slug) => api.knowledgeReadingState(slug: slug);
  Future<void> setReading(String slug, String state) =>
      api.setKnowledgeReadingState(slug: slug, state: state);
  Future<List<api.ArtifactVersion>> versions(String slug, int offset) =>
      api.listArtifactVersions(slug: slug, offset: offset);
  Future<void> adopt(String slug, String revision, bool value) =>
      api.adoptArtifactVersion(slug: slug, revisionId: revision, adopt: value);
  Future<List<api.BatchReviewResult>> resolveBatch(
    List<String> ids,
    bool accept,
  ) => api.resolveKnowledgeBatch(ids: ids, accept: accept);
  Future<void> undoOrganization(String id) => api.undoTopicOrganization(id: id);
  Future<List<api.TopicOrganizationPreview>> organizationHistory(
    String slug,
    int offset,
  ) => api.topicOrganizationHistory(slug: slug, offset: offset);
  Future<api.SuggestionFeedback?> feedback(String message) =>
      api.getSuggestionFeedback(messageId: message);
  Future<void> saveFeedback(
    String message,
    String decision,
    String suggestion,
    String? rewrite,
  ) => api.saveSuggestionFeedback(
    messageId: message,
    decision: decision,
    suggestion: suggestion,
    rewrite: rewrite,
  );
  Future<api.WikiPageDto?> page(String slug) => api.getWikiPage(slug: slug);

  Future<List<api.KnowledgeRepairSource>> repairSources(String slug) =>
      api.listKnowledgeRepairSources(slug: slug);
  Future<String> repair(String slug, List<String> ids) =>
      api.prepareKnowledgeSourceRepair(slug: slug, snapshotIds: ids);
  Future<List<api.WikiPageDto>> resolutionTargets(String fingerprint) =>
      api.listKnowledgeResolutionTargets(fingerprint: fingerprint);
  Future<void> resolveIssue(
    String fingerprint,
    String slug,
    String revision,
    String note,
  ) => api.resolveKnowledgeIssueWithRevision(
    fingerprint: fingerprint,
    slug: slug,
    revisionId: revision,
    note: note,
  );
  Future<List<api.KnowledgeReviewRecord>> history(String slug, int offset) =>
      api.listKnowledgeReviewHistory(slug: slug, offset: offset);
  Future<api.KnowledgeQueuePage> queue(int offset, String? status) =>
      api.listKnowledgeWorkQueue(offset: offset, status: status);
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
  Future<List<api.KnowledgeDiffPart>> diff(String id) =>
      api.getKnowledgeProposalDiff(id: id);
  Future<api.WikiPageDto> acceptParts(
    String id,
    List<int> parts,
    bool applicability,
    List<String> issues,
  ) => api.acceptKnowledgeProposalParts(
    id: id,
    selectedParts: Int64List.fromList(parts),
    acceptApplicability: applicability,
    resolvedIssues: issues,
  );
  Future<String> restore(String slug, String revision) =>
      api.prepareKnowledgeRestore(slug: slug, revisionId: revision);
  Future<List<api.TopicOrganizationPreview>> organizations(String slug) =>
      api.listTopicOrganizations(slug: slug);
  Future<String> organize(List<String> slugs, String mode) =>
      api.prepareTopicOrganization(slugs: slugs, mode: mode);
  Future<List<String>> resolveOrganization(String id, bool accept) =>
      api.resolveTopicOrganization(id: id, accept: accept);
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
    .family<api.KnowledgePageDetails, String>((ref, slug) {
      ref.watch(knowledgeRevisionProvider);
      return ref.read(knowledgeRepositoryProvider).details(slug);
    });
final knowledgeProposalsProvider =
    FutureProvider.autoDispose<List<api.KnowledgeProposal>>((ref) {
      ref.watch(knowledgeRevisionProvider);
      return ref.read(knowledgeRepositoryProvider).proposals();
    });
final knowledgeIssuesProvider =
    FutureProvider.autoDispose<List<api.KnowledgeIssue>>((ref) {
      ref.watch(knowledgeRevisionProvider);
      return ref.read(knowledgeRepositoryProvider).issues();
    });
final knowledgeRunsProvider =
    FutureProvider.autoDispose<List<api.KnowledgeBackgroundRunDto>>((ref) {
      ref.watch(knowledgeDigestBusyProvider);
      return ref.read(knowledgeRepositoryProvider).runs();
    });
final knowledgeRevisionsProvider = FutureProvider.autoDispose
    .family<List<api.KnowledgeRevisionDto>, String>((ref, slug) {
      ref.watch(knowledgeRevisionProvider);
      return ref.read(knowledgeRepositoryProvider).revisions(slug);
    });

void notifyKnowledgeChanged(WidgetRef ref) {
  final repo = ref.read(storageRepositoryProvider);
  if (repo is RustBridgeRepository) {
    repo.knowledgeRevision.value++;
  }
  ref.invalidate(knowledgeRevisionProvider);
  ref.invalidate(knowledgeProposalsProvider);
}
