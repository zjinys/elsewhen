import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';

T _repo<T>(Ref ref) => ref.read(storageRepositoryProvider) as T;

extension on WidgetRef {
  T _readRepo<T>() => read(storageRepositoryProvider) as T;
}


/// 记事本（kind=memo 的人类专属页面）列表 + 详情 + 快记框。
/// 存储与写纪律全部由 Rust 侧保证（create_wiki_page / append_wiki_page_content，
/// 单事务追加）。本组件只负责呈现与调用。

/// 记事本列表（memo 页，按更新时间倒序）
final notepadPagesProvider = FutureProvider<List<WikiPageMeta>>((ref) async {
  // 订阅知识修订号：memo 的建/改会让 revision 前进，列表随之刷新。
  ref.watch(knowledgeRevisionProvider);
  final bridge = _repo<RustBridgeRepository>(ref);
  final metas = await bridge.listWikiPageMetas();
  return metas.where((m) => m.kind == 'memo').toList()
    ..sort((a, b) => b.updatedAt.compareTo(a.updatedAt));
});

/// 当前打开的记事本详情
final notepadDetailProvider =
    FutureProvider.family<WikiPage?, String>((ref, slug) async {
  ref.watch(knowledgeRevisionProvider);
  return _repo<RustBridgeRepository>(ref).getWikiPage(slug);
});

class NotepadView extends ConsumerStatefulWidget {
  const NotepadView({super.key});

  @override
  ConsumerState<NotepadView> createState() => _NotepadViewState();
}

class _NotepadViewState extends ConsumerState<NotepadView> {
  String? _openSlug;

  @override
  Widget build(BuildContext context) {
    if (_openSlug != null) {
      return _NotepadDetail(
        slug: _openSlug!,
        onBack: () => setState(() => _openSlug = null),
      );
    }
    final pages = ref.watch(notepadPagesProvider);
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 8, 8),
          child: Row(
            children: [
              Text('记事本',
                  style: Theme.of(context)
                      .textTheme
                      .titleMedium
                      ?.copyWith(fontWeight: FontWeight.w600)),
              const Spacer(),
              IconButton(
                tooltip: '新建记事',
                icon: const Icon(Icons.add),
                onPressed: () => _createPage(context),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: pages.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => Center(child: Text('加载失败：$e')),
            data: (list) => list.isEmpty
                ? const Center(child: Text('还没有记事，点右上角 + 新建'))
                : ListView.builder(
                    itemCount: list.length,
                    itemBuilder: (context, i) {
                      final p = list[i];
                      return Material(
                        type: MaterialType.transparency,
                        child: ListTile(
                          title: Text(p.title,
                              maxLines: 1, overflow: TextOverflow.ellipsis),
                          subtitle: Text(p.summary.isEmpty ? '（无摘要）' : p.summary,
                              maxLines: 1, overflow: TextOverflow.ellipsis),
                          trailing: Text(
                            _fmtTime(p.updatedAt),
                            style: Theme.of(context).textTheme.bodySmall,
                          ),
                          onTap: () => setState(() => _openSlug = p.slug),
                        ),
                      );
                    },
                  ),
          ),
        ),
      ],
    );
  }

  Future<void> _createPage(BuildContext context) async {
    final controller = TextEditingController();
    final title = await showDialog<String>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('新建记事'),
        content: TextField(
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(hintText: '标题（如「想法收集」）'),
          onSubmitted: (v) => Navigator.pop(ctx, v),
        ),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(
              onPressed: () => Navigator.pop(ctx, controller.text),
              child: const Text('创建')),
        ],
      ),
    );
    if (title == null || title.trim().isEmpty) return;
    final bridge = ref._readRepo<RustBridgeRepository>();
    final page = await bridge.createWikiPage(title: title.trim());
    ref.invalidate(knowledgeRevisionProvider);
    setState(() => _openSlug = page.slug);
  }
}

class _NotepadDetail extends ConsumerStatefulWidget {
  const _NotepadDetail({required this.slug, required this.onBack});
  final String slug;
  final VoidCallback onBack;

  @override
  ConsumerState<_NotepadDetail> createState() => _NotepadDetailState();
}

class _NotepadDetailState extends ConsumerState<_NotepadDetail> {
  final _quickController = TextEditingController();
  bool _appending = false;

  @override
  void dispose() {
    _quickController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final pageAsync = ref.watch(notepadDetailProvider(widget.slug));
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 8, 8, 4),
          child: Row(
            children: [
              IconButton(
                  icon: const Icon(Icons.arrow_back), onPressed: widget.onBack),
              Expanded(
                child: pageAsync.when(
                  data: (p) => Text(p?.title ?? '',
                      style: Theme.of(context)
                          .textTheme
                          .titleMedium
                          ?.copyWith(fontWeight: FontWeight.w600),
                      overflow: TextOverflow.ellipsis),
                  loading: () => const SizedBox.shrink(),
                  error: (_, _) => const SizedBox.shrink(),
                ),
              ),
              // 打开完整详情（进入知识页 tab 的编辑/修订/AI 聊天全功能视图）
              pageAsync.when(
                data: (p) => p == null
                    ? const SizedBox.shrink()
                    : IconButton(
                        tooltip: '打开完整详情',
                        icon: const Icon(Icons.open_in_new, size: 18),
                        onPressed: () => openWikiPageTab(ref, p),
                      ),
                loading: () => const SizedBox.shrink(),
                error: (_, _) => const SizedBox.shrink(),
              ),
            ],
          ),
        ),
        const Divider(height: 1),
        Expanded(
          child: pageAsync.when(
            loading: () => const Center(child: CircularProgressIndicator()),
            error: (e, _) => Center(child: Text('加载失败：$e')),
            data: (page) => page == null
                ? const Center(child: Text('页面不存在'))
                : SingleChildScrollView(
                    padding: const EdgeInsets.all(16),
                    child: Align(
                      alignment: Alignment.topLeft,
                      child: SelectableText(
                        page.contentMd.isEmpty ? '（空，下面快记第一条）' : page.contentMd,
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                    ),
                  ),
          ),
        ),
        const Divider(height: 1),
        // 快记框：回车/按钮即追加（单事务，连击不丢写）
        Padding(
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: TextField(
                  controller: _quickController,
                  minLines: 1,
                  maxLines: 4,
                  decoration: const InputDecoration(
                    hintText: '快记一条，回车追加…',
                    isDense: true,
                    border: OutlineInputBorder(),
                  ),
                  onSubmitted: (_) => _append(),
                ),
              ),
              const SizedBox(width: 8),
              IconButton.filled(
                tooltip: '追加',
                onPressed: _appending ? null : _append,
                icon: _appending
                    ? const SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2))
                    : const Icon(Icons.send),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Future<void> _append() async {
    final text = _quickController.text.trim();
    if (text.isEmpty || _appending) return;
    setState(() => _appending = true);
    try {
      final bridge = ref._readRepo<RustBridgeRepository>();
      await bridge.appendWikiPageContent(slug: widget.slug, appendMd: text);
      _quickController.clear();
      ref.invalidate(knowledgeRevisionProvider);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('追加失败：$e')));
      }
    } finally {
      if (mounted) setState(() => _appending = false);
    }
  }
}

String _fmtTime(DateTime t) {
  final local = t.toLocal();
  final now = DateTime.now();
  if (local.year == now.year &&
      local.month == now.month &&
      local.day == now.day) {
    return '${local.hour.toString().padLeft(2, '0')}:${local.minute.toString().padLeft(2, '0')}';
  }
  return '${local.month}/${local.day}';
}
