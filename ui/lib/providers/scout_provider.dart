import 'dart:async';
import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import 'state_holder.dart';

// ── Scout 情报消费端 providers（提案 2026-11-15 §4）────────────────
//
// 镜像是本地派生物：providers 全部围绕 RustBridgeRepository 的 scout
// revision（后台拉取/已读操作后自增）失效重建。payload 保持 JSON 字符串
// 到展示层才解析（随源变化，镜像层不建模）。

/// 阅读状态和变化类型独立，可组合筛选。
enum ScoutKindFilter { all, new_, changed, gone }

final scoutKindFilterProvider =
    NotifierProvider<StateHolder<ScoutKindFilter>, ScoutKindFilter>(
      () => StateHolder(ScoutKindFilter.all),
    );
final scoutUnreadOnlyProvider = NotifierProvider<StateHolder<bool>, bool>(
  () => StateHolder(false),
);

/// 可替换的数据边界，测试不访问个人数据库。
class ScoutRepository {
  Future<List<ScoutFindingItem>> findings({
    String? source,
    String? kind,
    bool unread = false,
    int offset = 0,
    int limit = 40,
  }) async => (await api.listScoutFindings(
    sourceId: source,
    kind: kind,
    unreadOnly: unread,
    offset: offset,
    limit: limit,
  )).map(ScoutFindingItem.fromDto).toList();
  Future<void> markRead(WidgetRef ref, List<int> ids) async {
    if (ids.isEmpty) return; // API 空数组代表全库，界面禁止传入。
    await (ref.read(storageRepositoryProvider) as RustBridgeRepository)
        .markScoutFindingsRead(ids);
  }

  Future<Map<String, dynamic>> detail(int id) async =>
      jsonDecode(await api.scoutFindingDetail(id: id)) as Map<String, dynamic>;
}

final scoutRepositoryProvider = Provider((ref) => ScoutRepository());
final scoutDetailProvider = FutureProvider.autoDispose
    .family<Map<String, dynamic>, int>(
      (ref, id) => ref.watch(scoutRepositoryProvider).detail(id),
    );

/// scout 连接设置（null = 未配置）。
final scoutSettingsProvider = FutureProvider<api.ScoutSettingsDto?>((
  ref,
) async {
  ref.watch(scoutRevisionProvider);
  return await api.getScoutSettings();
});

/// 未读计数（侧栏徽标 + 今天栏卡片）。revision 由后台拉取/已读操作推动。
final scoutUnreadProvider = FutureProvider<int>((ref) {
  ref.watch(scoutRevisionProvider);
  final bridge = ref.watch(storageRepositoryProvider);
  if (bridge is! RustBridgeRepository) return Future.value(0);
  return bridge.scoutUnreadCount.value;
});

/// 镜像变更 revision：后台拉取、手动同步、已读操作都会自增，驱动列表刷新。
final scoutRevisionProvider = Provider<int>((ref) {
  final bridge = ref.watch(storageRepositoryProvider);
  if (bridge is! RustBridgeRepository) return 0;
  return bridge.scoutRevision.value;
});

/// 一条镜像 finding 的展示模型（payload 预解析为松散 map）。
class ScoutFindingItem {
  final int id;
  final String sourceId;
  final String kind;
  final String itemKey;
  final Map<String, dynamic> payload;

  /// 字段级变化（仅 kind=changed）：`{字段: {from, to, diff?}}`。
  /// 空 map = 无明细（new/gone，或改造前的服务端）。
  final Map<String, dynamic> delta;
  final String detectedAt;
  final String? readAt;

  ScoutFindingItem({
    required this.id,
    required this.sourceId,
    required this.kind,
    required this.itemKey,
    required this.payload,
    this.delta = const {},
    required this.detectedAt,
    required this.readAt,
  });

  /// 标题的跨源兜底链：name → title → full_name → payload.id → itemKey。
  /// gone 类 finding 的 payload 可能只有 id（老数据），至少剥出可读的 id
  /// 而不是展示原始 item_key（如 tag:www.producthunt.com,2005:Post/…）。
  String get title {
    final p = payload;
    for (final key in const ['name', 'title', 'full_name']) {
      final v = p[key];
      if (v is String && v.trim().isNotEmpty) {
        return _decodeEntities(v.trim());
      }
    }
    return itemKey;
  }

  /// 副标题兜底链：summary → description → artistName。
  /// summary 常是 HTML（RSS/Atom 原文）：剥标签、解实体、去空段，
  /// 去掉尾部「Discussion | Link」这类导航噪音后截断。
  String? get subtitle {
    final p = payload;
    for (final key in const ['summary', 'description', 'artistName']) {
      final v = p[key];
      if (v is String && v.trim().isNotEmpty) {
        return _stripHtml(v.trim());
      }
    }
    return null;
  }

  /// 极简 HTML 清理：剥标签 + 解常用实体 + 压空白。不做完整 DOM 解析——
  /// 来源是已知白名单域的 RSS 摘要，正则足够且不引入依赖。
  static String _stripHtml(String s) {
    // summary 常是双重转义的 RSS 摘要（&lt;p&gt;…&lt;/p&gt;），先解实体
    // 才能露出真实标签，再剥一次。
    var t = s;
    for (var i = 0; i < 2; i++) {
      t = _decodeEntities(t.replaceAll(RegExp(r'<[^>]*>'), ' '));
    }
    t = t.replaceAll(RegExp(r'\s+'), ' ').trim();
    // 去掉 RSS 尾部的导航词残留（Discussion/Link/Comments 等孤词结尾）。
    t = t.replaceAll(
      RegExp(r'\s*\|\s*(Discussion|Link|Comments)\s*$', caseSensitive: false),
      '',
    );
    return t.length > 140 ? '${t.substring(0, 140)}…' : t;
  }

  /// 解常用 HTML 实体（title/summary 来自 RSS 原文，转义很常见）。
  static String _decodeEntities(String s) => s
      .replaceAll('&amp;', '&')
      .replaceAll('&lt;', '<')
      .replaceAll('&gt;', '>')
      .replaceAll('&quot;', '"')
      .replaceAll('&#39;', "'")
      .replaceAll('&apos;', "'")
      .replaceAll('&nbsp;', ' ');

  /// 一条 finding 的「关键指标」行（可跨源归一）。
  ///
  /// 各源字段名不同（stars / rating / price_cents），但读者关心的就那么
  /// 几类：热度、口碑、价格、活跃度、排名。这里把它们统一成「标签 + 值」
  /// 的短列表，UI 直接平铺。delta 能算出来时顺带给出变化方向，这样
  /// 「+34 stars」比裸的「57225 stars」有用得多。
  List<ScoutSignal> get signals {
    final p = payload;
    final out = <ScoutSignal>[];

    void addNum(String key, String label, {String suffix = ''}) {
      final v = p[key];
      if (v is! num) return;
      final d = delta[key];
      final diff = d is Map ? d['diff'] : null;
      out.add(
        ScoutSignal(
          label: label,
          value: '${_compactNumber(v)}$suffix',
          diff: diff is num ? diff : null,
        ),
      );
    }

    addNum('stargazers_count', 'star');
    addNum('forks_count', 'fork');
    addNum('open_issues_count', 'issue');
    addNum('rating', '评分');
    addNum('rating_count', '评价', suffix: ' 条');
    addNum('ratings_average', '评分');
    addNum('ratings_count', '评价', suffix: ' 条');
    addNum('rank', '榜位', suffix: ' 名');

    // 价格：分转元/美元，取整到分位；0 显示「免费」。
    final cents = p['price_cents'];
    if (cents is num) {
      final cur = (p['currency_code'] as String?)?.toUpperCase() ?? 'USD';
      out.add(
        ScoutSignal(
          label: '价格',
          value: cents == 0 ? '免费' : '$cur ${(cents / 100).toStringAsFixed(2)}',
          diff: null,
        ),
      );
      if (p['is_pay_what_you_want'] == true) {
        out.add(const ScoutSignal(label: '定价', value: '随意付', diff: null));
      }
    }

    // 活跃度：pushed_at 距今多久（GitHub）。
    final pushed = p['pushed_at'];
    if (pushed is String) {
      final rel = _relativeDays(pushed);
      if (rel != null) {
        out.add(ScoutSignal(label: '最后更新', value: rel, diff: null));
      }
    }
    final updated = p['updated'];
    if (updated is String && p['pushed_at'] == null) {
      final rel = _relativeDays(updated);
      if (rel != null) {
        out.add(ScoutSignal(label: '更新', value: rel, diff: null));
      }
    }

    // 状态标记：归档 / 许可证 / 语言——短文本，直接给。
    if (p['archived'] == true) {
      out.add(const ScoutSignal(label: '状态', value: '已归档', diff: null));
    }
    final lic = p['license_spdx_id'];
    if (lic is String && lic.isNotEmpty) {
      out.add(ScoutSignal(label: '许可', value: lic, diff: null));
    }
    final lang = p['language'];
    if (lang is String && lang.isNotEmpty) {
      out.add(ScoutSignal(label: '语言', value: lang, diff: null));
    }
    final genre = p['genres_0_name'];
    if (genre is String && genre.isNotEmpty) {
      out.add(ScoutSignal(label: '分类', value: genre, diff: null));
    }
    final seller = p['seller_name'];
    if (seller is String && seller.isNotEmpty) {
      out.add(ScoutSignal(label: '卖家', value: seller, diff: null));
    }
    final dev = p['developer'];
    if (dev is String && dev.isNotEmpty) {
      out.add(ScoutSignal(label: '开发者', value: _stripHtml(dev), diff: null));
    }

    return out;
  }

  /// 主题标签（GitHub topics）：直接读，不走 signals（列表会太长）。
  List<String> get topics {
    final t = payload['topics'];
    if (t is! List) return const [];
    return t.whereType<String>().take(4).toList(growable: false);
  }

  /// 发布/创建日期（GitHub created_at、App Store releaseDate）。
  String? get since {
    for (final k in const ['created_at', 'releaseDate', 'published']) {
      final v = payload[k];
      if (v is String && v.isNotEmpty) return _shortDate(v);
    }
    return null;
  }

  /// 1.2k / 3.4M 这类紧凑数字。
  static String _compactNumber(num v) {
    if (v.abs() < 1000) return v is int ? '$v' : v.toStringAsFixed(1);
    if (v.abs() < 1000000) return '${(v / 1000).toStringAsFixed(1)}k';
    return '${(v / 1000000).toStringAsFixed(1)}M';
  }

  static String _shortDate(String iso) {
    final d = DateTime.tryParse(iso);
    if (d == null) return iso;
    return '${d.year}-${d.month.toString().padLeft(2, '0')}';
  }

  /// 「3 天前」这类相对时间；解析失败返回 null（宁可不显示，不显示错误）。
  static String? _relativeDays(String iso) {
    final d = DateTime.tryParse(iso);
    if (d == null) return null;
    final diff = DateTime.now().toUtc().difference(d.toUtc());
    if (diff.isNegative) return '刚刚';
    if (diff.inMinutes < 60) return '${diff.inMinutes} 分钟前';
    if (diff.inHours < 24) return '${diff.inHours} 小时前';
    if (diff.inDays < 30) return '${diff.inDays} 天前';
    if (diff.inDays < 365) return '${diff.inDays ~/ 30} 个月前';
    return '${diff.inDays ~/ 365} 年前';
  }

  /// 外链兜底链：url → html_url → permalink → link。
  String? get link {
    final p = payload;
    for (final key in const ['html_url', 'permalink', 'link', 'url']) {
      final v = p[key];
      if (v is String && v.startsWith('http')) return v;
    }
    return null;
  }

  factory ScoutFindingItem.fromDto(api.ScoutFindingDto dto) {
    Map<String, dynamic> payload = const {};
    try {
      final decoded = jsonDecode(dto.payloadJson);
      if (decoded is Map<String, dynamic>) payload = decoded;
    } catch (_) {
      // payload 解析失败不阻塞列表展示；title 落回 itemKey。
    }
    Map<String, dynamic> delta = const {};
    final rawDelta = dto.deltaJson;
    if (rawDelta != null) {
      try {
        final decoded = jsonDecode(rawDelta);
        if (decoded is Map<String, dynamic>) delta = decoded;
      } catch (_) {
        // delta 是展示增强，解析失败就当没有——不能因此丢整条 finding。
      }
    }
    return ScoutFindingItem(
      id: dto.id,
      sourceId: dto.sourceId,
      kind: dto.kind,
      itemKey: dto.itemKey,
      payload: payload,
      delta: delta,
      detectedAt: dto.detectedAt,
      readAt: dto.readAt,
    );
  }

  bool get isUnread => readAt == null;
}

/// 一条跨源归一的关键指标（用于 finding 卡片的一行摘要）。
class ScoutSignal {
  final String label;
  final String value;

  /// 相比上次的数值变化（仅 changed 且字段是数字时有）。
  final num? diff;

  const ScoutSignal({required this.label, required this.value, this.diff});

  bool get hasDiff => diff != null && diff != 0;
}

/// 发现页选中的源（null = 全部源）。
final scoutSourceFilterProvider =
    NotifierProvider<StateHolder<String?>, String?>(
      () => StateHolder<String?>(null),
    );

/// 跨源按检测时间倒序，统一分页，不让单个源占据整个首屏。
class ScoutFindingsState {
  final List<ScoutFindingItem> items;
  final bool hasMore;
  final bool loadingMore;
  final String? error;
  const ScoutFindingsState({
    required this.items,
    this.hasMore = false,
    this.loadingMore = false,
    this.error,
  });
  bool get isEmpty => items.isEmpty;
}

class ScoutFindingsNotifier extends AsyncNotifier<ScoutFindingsState> {
  static const pageSize = 40;
  int _generation = 0;
  @override
  Future<ScoutFindingsState> build() async {
    _generation++;
    ref.watch(scoutRevisionProvider);
    final source = ref.watch(scoutSourceFilterProvider);
    final kind = ref.watch(scoutKindFilterProvider);
    final unread = ref.watch(scoutUnreadOnlyProvider);
    final repo = ref.watch(scoutRepositoryProvider);
    final items = await repo.findings(
      source: source,
      kind: _kind(kind),
      unread: unread,
      limit: pageSize,
    );
    return ScoutFindingsState(items: items, hasMore: items.length == pageSize);
  }

  String? _kind(ScoutKindFilter f) => switch (f) {
    ScoutKindFilter.all => null,
    ScoutKindFilter.new_ => 'new',
    ScoutKindFilter.changed => 'changed',
    ScoutKindFilter.gone => 'gone',
  };
  Future<void> loadMore() async {
    final current = state.value;
    if (current == null || !current.hasMore || current.loadingMore) return;
    state = AsyncData(
      ScoutFindingsState(
        items: current.items,
        hasMore: true,
        loadingMore: true,
      ),
    );
    final pending = state.value;
    final generation = _generation;
    try {
      final items = await ref
          .read(scoutRepositoryProvider)
          .findings(
            source: ref.read(scoutSourceFilterProvider),
            kind: _kind(ref.read(scoutKindFilterProvider)),
            unread: ref.read(scoutUnreadOnlyProvider),
            offset: current.items.length,
            limit: pageSize,
          );
      if (!ref.mounted ||
          generation != _generation ||
          !identical(state.value, pending)) {
        return;
      }
      state = AsyncData(
        ScoutFindingsState(
          items: [...current.items, ...items],
          hasMore: items.length == pageSize,
        ),
      );
    } catch (e) {
      if (!ref.mounted ||
          generation != _generation ||
          !identical(state.value, pending)) {
        return;
      }
      state = AsyncData(
        ScoutFindingsState(
          items: current.items,
          hasMore: true,
          error: e.toString(),
        ),
      );
    }
  }

  Future<void> reload() async => ref.invalidateSelf();
}

final scoutFindingsProvider =
    AsyncNotifierProvider.autoDispose<
      ScoutFindingsNotifier,
      ScoutFindingsState
    >(ScoutFindingsNotifier.new);

/// 按源聚合摘要（总览卡 + 发现页分组）。Rust 侧一次 GROUP BY。
final scoutSourceSummariesProvider =
    FutureProvider.autoDispose<List<ScoutSourceSummary>>((ref) async {
      ref.watch(scoutRevisionProvider);
      final dtos = await api.scoutSourceSummaries();
      return dtos.map(ScoutSourceSummary.fromDto).toList();
    });

/// 一个源的镜像聚合（展示模型）。
class ScoutSourceSummary {
  final String sourceId;
  final int unread;
  final int newCount;
  final int changedCount;
  final int goneCount;
  final int total;
  final String? latestDetectedAt;

  const ScoutSourceSummary({
    required this.sourceId,
    required this.unread,
    required this.newCount,
    required this.changedCount,
    required this.goneCount,
    required this.total,
    required this.latestDetectedAt,
  });

  factory ScoutSourceSummary.fromDto(api.ScoutSourceSummaryDto dto) =>
      ScoutSourceSummary(
        sourceId: dto.sourceId,
        unread: dto.unread,
        newCount: dto.newCount,
        changedCount: dto.changedCount,
        goneCount: dto.goneCount,
        total: dto.total,
        latestDetectedAt: dto.latestDetectedAt,
      );

  /// 各 kind 的「非零」摘要片段（如「12 新 · 3 变 · 1 消失」）。
  /// 全零时不返回（该源只有已读、没有新的变化）。
  List<({String kind, int count})> get kindBreakdown => [
    if (newCount > 0) (kind: 'new', count: newCount),
    if (changedCount > 0) (kind: 'changed', count: changedCount),
    if (goneCount > 0) (kind: 'gone', count: goneCount),
  ];
}

/// 数据源页（服务端实时，只读）：合并进总览后仍保留给「源的运行状态」。
final scoutSourcesProvider =
    FutureProvider.autoDispose<List<api.ScoutSourceDto>>((ref) async {
      ref.watch(scoutRevisionProvider);
      return await api.listScoutSources();
    });
