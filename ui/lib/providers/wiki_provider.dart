import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../models/wiki_page.dart';
import '../models/relation.dart';
import '../models/tweet_fetch.dart';
import '../models/import_fetch.dart';
import '../bridge/rust_bridge_repository.dart';
import '../bridge/generated.dart/api.dart'
    show EntityFactDto, EntityMergeStatusDto;

/// 一级导航内容域。待办使用主工作区承载，避免在弹窗里维护持续讨论。
enum SidebarTab { conversation, wiki, todos }

final sidebarTabProvider = StateProvider<SidebarTab>(
  (ref) => SidebarTab.conversation,
);

/// wiki 页面列表（全部，UI 按 kind 分组展示）
final wikiPagesProvider = FutureProvider<List<WikiPage>>((ref) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listWikiPages();
});

/// 与某页相关的人物关系（双向；页面详情头部的人物关系区块用）
final pageRelationsProvider = FutureProvider.family<List<Relation>, String>((
  ref,
  slug,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listRelationsForPage(slug);
});

/// 单独查询某页详情（tab 内容用，不依赖“单页选中”）
final wikiPageProvider = FutureProvider.family<WikiPage?, String>((
  ref,
  slug,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.getWikiPage(slug);
});

/// 某页的派生产物列表（AI 加工成果，挂在详情页「派生产物」区块）
final wikiDerivativesProvider = FutureProvider.family<List<WikiPage>, String>((
  ref,
  slug,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.listWikiPageDerivatives(slug);
});

final entityFactsProvider = FutureProvider.family<List<EntityFactDto>, String>((
  ref,
  slug,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  final kind = slug.split('/').first;
  if (!['person', 'project', 'topic'].contains(kind)) return const [];
  return bridge.listEntityFacts(kind, slug);
});

final entityAliasesProvider = FutureProvider.family<List<String>, String>((
  ref,
  slug,
) async {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  final kind = slug.split('/').first;
  if (!['person', 'project', 'topic'].contains(kind)) return const [];
  return bridge.listEntityAliases(kind, slug);
});

final entityMergeStatusProvider =
    FutureProvider.family<EntityMergeStatusDto?, String>((ref, slug) async {
      final bridge =
          ref.read(storageRepositoryProvider) as RustBridgeRepository;
      return bridge.getEntityMergeStatus(slug);
    });

/// 当前选中查看的 wiki 页 slug（保留：左侧列表高亮 + 兼容引用）
final selectedWikiSlugProvider = StateProvider<String?>((ref) => null);

/// 选中页详情（保留兼容，已不再驱动右侧面板）
final wikiPageDetailProvider = FutureProvider<WikiPage?>((ref) async {
  final slug = ref.watch(selectedWikiSlugProvider);
  if (slug == null) return null;
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return await bridge.getWikiPage(slug);
});

// ─────────────────────────────────────────────
// 右侧知识库面板：多 tab
// ─────────────────────────────────────────────

/// 右侧面板的一个 tab
sealed class WikiTabEntry {
  const WikiTabEntry();

  String get id;
  String get title;
  bool get closable => true;
}

/// Tab 1（固定）：导入（粘贴链接抓取或直接文本保存）。缺省只有这一个，不可关闭。
class ImportTabEntry extends WikiTabEntry {
  const ImportTabEntry();

  @override
  String get id => 'import';

  @override
  String get title => '导入';

  @override
  bool get closable => false;
}

/// 知识库页面详情 tab
class PageTabEntry extends WikiTabEntry {
  final String slug;

  /// 页面标题（同时作为 tab 标题）
  @override
  final String title;

  const PageTabEntry({required this.slug, required this.title});

  @override
  String get id => 'page-$slug';
}

/// 已抓取推文的预览 tab（内含内容对话 + 保存按钮，点保存才入库）
class TweetTabEntry extends WikiTabEntry {
  final TweetFetch fetch;

  const TweetTabEntry({required this.fetch});

  @override
  String get id => 'tweet-${fetch.tweetId}';

  @override
  String get title => '推文 ${fetch.tweetId}';
}

/// 已抓取任意网址的预览 tab（网页/推文通用；内含保存按钮，点保存才入库）
class ImportFetchTabEntry extends WikiTabEntry {
  final ImportFetch fetch;

  const ImportFetchTabEntry({required this.fetch});

  @override
  String get id => 'import-${fetch.sourceUrl.hashCode}';

  @override
  String get title {
    final t = fetch.displayTitle;
    return t.length > 12 ? '${t.substring(0, 12)}…' : t;
  }
}

/// 已打开的 tab 列表（第一个固定为导入 tab）
final wikiOpenTabsProvider = StateProvider<List<WikiTabEntry>>(
  (ref) => const [ImportTabEntry()],
);

/// 当前激活的 tab id
final wikiActiveTabIdProvider = StateProvider<String>((ref) => 'import');

const _maxWikiTabs = 8;

/// 打开一个 tab；已存在则用新条目替换（拿到最新标题）并激活。
void openWikiTab(WidgetRef ref, WikiTabEntry entry) {
  final tabs = [...ref.read(wikiOpenTabsProvider)];
  final existingIndex = tabs.indexWhere((t) => t.id == entry.id);
  if (existingIndex >= 0) {
    // 已打开：原位替换为最新条目（如页面标题已更新），再激活
    tabs[existingIndex] = entry;
  } else {
    tabs.add(entry);
    // 上限控制：超出时优先关闭最老的（固定导入 tab 不可关）
    while (tabs.length > _maxWikiTabs) {
      final removable = tabs.indexWhere((t) => t.closable);
      if (removable < 0) break;
      tabs.removeAt(removable);
    }
  }
  ref.read(wikiOpenTabsProvider.notifier).state = tabs;
  ref.read(wikiActiveTabIdProvider.notifier).state = entry.id;
}

/// 从左侧列表打开知识库页面 tab（同时更新左侧高亮）。
/// 每次都使 family provider 失效，确保重新从数据库读取最新内容
/// （否则同一 slug 一旦被解析过一次就缓存旧内容，保存再打开会看到旧页）。
void openWikiPageTab(WidgetRef ref, WikiPage page) {
  ref.read(selectedWikiSlugProvider.notifier).state = page.slug;
  ref.invalidate(wikiPageProvider(page.slug));
  ref.invalidate(pageRelationsProvider(page.slug));
  openWikiTab(ref, PageTabEntry(slug: page.slug, title: page.title));
}

/// 打开一个已抓取推文的预览 tab
void openWikiTweetTab(WidgetRef ref, TweetFetch fetch) {
  openWikiTab(ref, TweetTabEntry(fetch: fetch));
}

/// 打开一个已抓取任意网址内容的预览 tab（网页/推文通用）
void openWikiImportFetchTab(WidgetRef ref, ImportFetch fetch) {
  openWikiTab(ref, ImportFetchTabEntry(fetch: fetch));
}

/// 关闭一个 tab（导入 tab 不可关闭）
void closeWikiTab(WidgetRef ref, String id) {
  final tabs = [...ref.read(wikiOpenTabsProvider)];
  final index = tabs.indexWhere((t) => t.id == id);
  if (index < 0 || !tabs[index].closable) return;
  tabs.removeAt(index);
  ref.read(wikiOpenTabsProvider.notifier).state = tabs;
  if (ref.read(wikiActiveTabIdProvider) == id) {
    final fallback = index.clamp(0, tabs.length - 1);
    ref.read(wikiActiveTabIdProvider.notifier).state = tabs.isEmpty
        ? 'import'
        : tabs[fallback].id;
  }
}
