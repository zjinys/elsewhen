import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../providers/wiki_provider.dart';
import '../utils/error_report.dart';

T _repo<T>(Ref ref) => ref.read(storageRepositoryProvider) as T;

extension on WidgetRef {
  T _readRepo<T>() => read(storageRepositoryProvider) as T;
}

/// 记事本 = **单一知识页**（kind=memo，固定 slug）。
/// 「记事」入口点击即打开这一页：不存在时静默建档，存在则原样返回——
/// 永不新建第二张。旧「多页列表 + 新建」已按需求移除；快记仍走
/// appendWikiPageContent（单事务追加，连击不丢写）。
///
/// ⚠️ ensure 必须先查后建：create_wiki_page 对已存在页是 upsert(Always)
/// **覆盖正文**（见 storage tests `memo_same_title_recreate_reuses_slug_and_overwrites_content`），
/// 已有快记内容时绝不能再调 create。

/// 记事本唯一页的固定 slug（标题「记事本」slugify 后落在此处）。
const kNotepadSlug = 'memo/记事本';

/// 记事本唯一页：读取；首次使用（不存在）时静默建档。
final notepadDetailProvider = FutureProvider<WikiPage?>((ref) async {
  // 订阅知识修订号：快记追加会让 revision 前进，详情随之刷新。
  ref.watch(knowledgeRevisionProvider);
  final bridge = _repo<RustBridgeRepository>(ref);
  final page = await bridge.getWikiPage(kNotepadSlug);
  if (page != null) return page;
  // 首次建档（同 slug 同标题 → unique_slug 复用，不会产生第二页）。
  await bridge.createWikiPage(title: '记事本');
  return bridge.getWikiPage(kNotepadSlug);
});

/// 记事本面板：唯一知识页详情 + 快记框。
class NotepadView extends ConsumerStatefulWidget {
  const NotepadView({super.key});

  @override
  ConsumerState<NotepadView> createState() => _NotepadViewState();
}

class _NotepadViewState extends ConsumerState<NotepadView> {
  final _quickController = TextEditingController();
  bool _appending = false;

  @override
  void dispose() {
    _quickController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final pageAsync = ref.watch(notepadDetailProvider);
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
            error: (e, _) => Center(
              child: Text(reportUiError('读取记事本', e, showDetail: true)),
            ),
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
      await bridge.appendWikiPageContent(slug: kNotepadSlug, appendMd: text);
      _quickController.clear();
      ref.invalidate(knowledgeRevisionProvider);
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(reportUiError('追加内容', e, showDetail: true))),
        );
      }
    } finally {
      if (mounted) setState(() => _appending = false);
    }
  }
}
