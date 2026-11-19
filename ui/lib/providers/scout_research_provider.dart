import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import 'scout_provider.dart';
import 'state_holder.dart';

/// Fact and inference are separate; the raw observation versions travel with a saved study.
class ScoutProduct {
  final Map<String, dynamic> data;
  ScoutProduct(this.data);
  String get id => '${data['product_id']}';
  String get title => '${data['title'] ?? ''}';
  Map<String, dynamic> get payload =>
      Map<String, dynamic>.from(data['payload'] as Map? ?? {});
  String get source =>
      '${(data['sources'] as List? ?? []).firstOrNull?['source_id'] ?? data['platform']}';
  List<Map<String, dynamic>> get trends => (data['trends'] as List? ?? [])
      .whereType<Map>()
      .map((e) => Map<String, dynamic>.from(e))
      .toList();
  List<Map<String, dynamic>> get reasons => (data['reasons'] as List? ?? [])
      .whereType<Map>()
      .map((e) => Map<String, dynamic>.from(e))
      .toList();
  String? get description =>
      ((payload['description'] ?? payload['summary']) as String?)
          ?.replaceAll(RegExp(r'<[^>]*>'), ' ')
          .replaceAll(RegExp(r'\s+'), ' ')
          .trim();
  String? get link =>
      (payload['html_url'] ?? payload['url'] ?? payload['link']) as String?;
}

/// Refreshing a snapshot or editing its title is not new decision evidence.
bool scoutHasNewEvidence(ScoutProduct product, Map<String, dynamic> study) {
  final previous = (study['evidence'] as Map?)?['payload'] as Map? ?? {};
  return product.reasons.isNotEmpty &&
      const [
        'stargazers_count',
        'forks_count',
        'rating_count',
        'ratings_count',
        'rating',
        'ratings_average',
        'rank',
        'price_cents',
        'currency_code',
      ].any((key) => product.payload[key] != previous[key]);
}

class ScoutProductPage {
  final String instance;
  final List<ScoutProduct> items;
  final bool stale;
  final String? next;
  final String asOf;
  final int indexPending;
  ScoutProductPage(Map<String, dynamic> data)
    : instance = '${data['instance_id']}',
      items = (data['items'] as List? ?? [])
          .whereType<Map>()
          .map((e) => ScoutProduct(Map<String, dynamic>.from(e)))
          .toList(),
      stale = data['stale'] == true,
      next = data['next'] as String?,
      asOf = '${data['as_of'] ?? ''}',
      indexPending = (data['index_pending'] as num?)?.toInt() ?? 0;
}

class ScoutResearchRepository {
  Future<ScoutProductPage> products(
    String query,
    String? source, {
    String? cursor,
    String? asOf,
    String view = 'signals',
  }) async => ScoutProductPage(
    jsonDecode(
      await api.scoutProducts(
        query: query,
        source: source,
        cursor: cursor,
        asOf: asOf,
        view: view,
      ),
    ) as Map<String, dynamic>,
  );
  Future<Map<String, dynamic>> detail(String id, {int? before}) async =>
      jsonDecode(await api.scoutProductDetail(productId: id, before: before))
          as Map<String, dynamic>;
  Future<String> fetchEvidence(String url) async =>
      (await api.fetchImportUrl(url: url)).contentMd;
  Future<Map<String, dynamic>> analysis(String slug) async =>
      jsonDecode(await api.scoutResearchAnalysis(studySlug: slug))
          as Map<String, dynamic>;
  Future<Map<String, dynamic>> preferences() async =>
      jsonDecode(await api.scoutPreferences()) as Map<String, dynamic>;
  Future<Map<String, dynamic>> savePreferences(
    Map<String, dynamic> value,
    String? token,
  ) async => jsonDecode(
    await api.saveScoutPreferences(
      documentJson: jsonEncode(value),
      expectedToken: token,
    ),
  ) as Map<String, dynamic>;
  Future<Map<String, dynamic>?> draft(String instance, String product) async =>
      jsonDecode(
        await api.scoutResearchDraft(instanceId: instance, productId: product),
      ) as Map<String, dynamic>?;
  Future<List<Map<String, dynamic>>> drafts() async =>
      (jsonDecode(await api.scoutResearchDrafts()) as List)
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();
  Future<void> saveDraft(Map<String, dynamic> document) =>
      api.saveScoutResearchDraft(documentJson: jsonEncode(document));
  Future<void> deleteDraft(String instance, String product) =>
      api.deleteScoutResearchDraft(instanceId: instance, productId: product);
  Future<List<Map<String, dynamic>>> studies() async =>
      (jsonDecode(await api.listScoutResearch()) as List)
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();
  Future<Map<String, dynamic>> save(
    Map<String, dynamic> value,
    String? token,
  ) async {
    final document = {...value}
      ..remove('token')
      ..remove('slug');
    return jsonDecode(
      await api.saveScoutResearch(
        documentJson: jsonEncode(document),
        expectedToken: token,
      ),
    ) as Map<String, dynamic>;
  }
}

final scoutResearchRepositoryProvider = Provider(
  (ref) => ScoutResearchRepository(),
);

enum ScoutWorkspace { discover, studies, activity }

final scoutWorkspaceProvider =
    NotifierProvider<StateHolder<ScoutWorkspace>, ScoutWorkspace>(
      () => StateHolder(ScoutWorkspace.discover),
    );
final scoutProductQueryProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder(''),
);
final scoutProductViewProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder('signals'),
);

class ScoutProductsNotifier extends AsyncNotifier<ScoutProductPage> {
  bool _loadingMore = false;
  @override
  Future<ScoutProductPage> build() {
    ref.watch(scoutRevisionProvider);
    return ref
        .watch(scoutResearchRepositoryProvider)
        .products(
          ref.watch(scoutProductQueryProvider),
          ref.watch(scoutSourceFilterProvider),
          view: ref.watch(scoutProductViewProvider),
        );
  }

  Future<void> more() async {
    final page = state.value;
    if (_loadingMore || page?.next == null) return;
    _loadingMore = true;
    try {
      final next = await ref
          .read(scoutResearchRepositoryProvider)
          .products(
            ref.read(scoutProductQueryProvider),
            ref.read(scoutSourceFilterProvider),
            cursor: page!.next,
            asOf: page.asOf,
            view: ref.read(scoutProductViewProvider),
          );
      if (!ref.mounted || !identical(state.value, page)) return;
      state = AsyncData(
        ScoutProductPage({
          'instance_id': page.instance,
          'items': [
            ...page.items.map((i) => i.data),
            ...next.items
                .where((i) => !page.items.any((p) => p.id == i.id))
                .map((i) => i.data),
          ],
          'as_of': page.asOf,
          'index_pending': next.indexPending,
          'next': next.next,
          'stale': next.stale,
        }),
      );
    } finally {
      _loadingMore = false;
    }
  }
}

final scoutProductPageProvider =
    AsyncNotifierProvider<ScoutProductsNotifier, ScoutProductPage>(
      ScoutProductsNotifier.new,
    );
final scoutStudiesProvider = FutureProvider.autoDispose(
  (ref) => ref.watch(scoutResearchRepositoryProvider).studies(),
);

class ScoutProductDetailNotifier extends AsyncNotifier<Map<String, dynamic>> {
  ScoutProductDetailNotifier(this.id);
  final String id;
  bool _loading = false;
  @override
  Future<Map<String, dynamic>> build() =>
      ref.watch(scoutResearchRepositoryProvider).detail(id);
  Future<void> more() async {
    final current = state.value;
    if (_loading || current?['next'] == null) return;
    _loading = true;
    try {
      final next = await ref
          .read(scoutResearchRepositoryProvider)
          .detail(id, before: current!['next'] as int);
      if (ref.mounted && identical(state.value, current)) {
        state = AsyncData({
          ...current,
          'history': [
            ...current['history'] as List,
            ...next['history'] as List,
          ],
          'next': next['next'],
          'stale': current['stale'] == true || next['stale'] == true,
        });
      }
    } finally {
      _loading = false;
    }
  }
}

final scoutProductDetailProvider = AsyncNotifierProvider.autoDispose
    .family<ScoutProductDetailNotifier, Map<String, dynamic>, String>(
      ScoutProductDetailNotifier.new,
    );

final scoutResearchAnalysisProvider = FutureProvider.autoDispose
    .family<Map<String, dynamic>, String>((ref, slug) {
      ref.watch(scoutRevisionProvider);
      return ref.watch(scoutResearchRepositoryProvider).analysis(slug);
    });

final scoutPreferencesProvider = FutureProvider.autoDispose(
  (ref) => ref.watch(scoutResearchRepositoryProvider).preferences(),
);

List<String> scoutStudyThemes(Map<String, dynamic> study) {
  final themes = study['themes'];
  return themes is List
      ? themes
            .whereType<String>()
            .where((s) => s.trim().isNotEmpty)
            .toSet()
            .toList()
      : '${study['theme'] ?? ''}'.trim().isEmpty
      ? []
      : ['${study['theme']}'];
}

final scoutDraftsProvider = FutureProvider.autoDispose(
  (ref) => ref.watch(scoutResearchRepositoryProvider).drafts(),
);
