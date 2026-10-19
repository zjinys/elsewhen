part of '../knowledge_workflows.dart';

//! 知识库列表（分页加载+条目卡+批量复审）

class KnowledgeLibrary extends ConsumerStatefulWidget {
  const KnowledgeLibrary({super.key});
  @override
  ConsumerState<KnowledgeLibrary> createState() => _LibraryState();
}

class _LibraryState extends ConsumerState<KnowledgeLibrary> {
  final _query = TextEditingController(), _tag = TextEditingController();
  String? _area, _kind, _state, _error;
  List<api.LibraryEntry> _items = [];
  bool _busy = false, _more = false;
  int _request = 0;
  Timer? _debounce;
  @override
  void initState() {
    super.initState();
    _load();
    // 输入变化时重建（suffixIcon 显隐依赖 _query.text）。
    _query.addListener(() => setState(() {}));
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _query.dispose();
    _tag.dispose();
    super.dispose();
  }

  Future<void> _load({bool more = false}) async {
    final request = ++_request;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final page = await ref
          .read(knowledgeRepositoryProvider)
          .browse(
            _query.text.trim(),
            _area,
            _kind,
            _tag.text.trim().isEmpty ? null : _tag.text.trim(),
            _state,
            more ? _items.length : 0,
          );
      if (mounted && request == _request) {
        setState(() {
          _items = more ? [..._items, ...page.items] : page.items;
          _more = page.hasMore;
        });
      }
    } catch (e) {
      if (mounted && request == _request) {
        setState(() => _error = '读取知识列表失败：$e');
      }
    } finally {
      if (mounted && request == _request) setState(() => _busy = false);
    }
  }

  void _search() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 300), () => _load());
  }

  Widget _filter(
    String label,
    String? value,
    Map<String, String> labels,
    void Function(String?) update,
  ) => DropdownButton<String>(
    value: value,
    hint: Text(label),
    items: [
      DropdownMenuItem<String>(value: null, child: Text('全部$label')),
      for (final e in labels.entries)
        DropdownMenuItem(value: e.key, child: Text(e.value)),
    ],
    onChanged: (v) {
      setState(() => update(v));
      _load();
    },
  );
  @override
  Widget build(BuildContext context) {
    ref.listen(knowledgeRevisionProvider, (_, _) => _load());
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.all(16),
          child: Column(
            children: [
              TextField(
                controller: _query,
                onChanged: (_) => _search(),
                decoration: InputDecoration(
                  hintText: '搜索标题或全文',
                  prefixIcon: Icon(
                    Icons.search,
                    color: AppTheme.textTertiary,
                    size: 20,
                  ),
                  filled: true,
                  fillColor: AppTheme.surface1,
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 12,
                  ),
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(10),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                  enabledBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(10),
                    borderSide: BorderSide(color: AppTheme.surface3),
                  ),
                  focusedBorder: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(10),
                    borderSide: BorderSide(
                      color: AppTheme.accentPrimary,
                      width: 1.5,
                    ),
                  ),
                  suffixIcon: _query.text.isNotEmpty
                      ? IconButton(
                          tooltip: '清除搜索',
                          icon: Icon(
                            Icons.clear,
                            color: AppTheme.textTertiary,
                            size: 18,
                          ),
                          onPressed: () {
                            _query.clear();
                            _debounce?.cancel();
                            _load();
                          },
                        )
                      : null,
                ),
              ),
              Wrap(
                spacing: 12,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  _filter('分区', _area, {
                    'imported': '素材库',
                    'insight': '知识沉淀',
                    'network': '联系人与项目',
                  }, (v) => _area = v),
                  _filter('类型', _kind, {
                    'source': '原料',
                    'note': '笔记',
                    'method': '方法',
                    'case': '案例',
                    'principle': '规律',
                    'topic': '主题',
                    'person': '联系人',
                    'project': '项目',
                  }, (v) => _kind = v),
                  _filter('阅读状态', _state, readingLabels, (v) => _state = v),
                  SizedBox(
                    width: 140,
                    child: TextField(
                      controller: _tag,
                      onChanged: (_) => _search(),
                      decoration: const InputDecoration(labelText: '标签'),
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
        if (_busy) const LinearProgressIndicator(),
        if (_error != null) Text(_error!),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
            itemCount: _items.length + 1,
            itemBuilder: (context, index) {
              if (index == _items.length) {
                return _more
                    ? Center(
                        child: TextButton(
                          onPressed: _busy ? null : () => _load(more: true),
                          child: const Text('加载更多'),
                        ),
                      )
                    : Padding(
                        padding: const EdgeInsets.all(16),
                        child: Center(
                          child: Text(
                            _items.isEmpty ? '没有匹配的知识页' : '已显示全部结果',
                            style: TextStyle(color: AppTheme.textTertiary),
                          ),
                        ),
                      );
              }
              final p = _items[index];
              return _LibraryCard(
                entry: p,
                onTap: () => openWikiTab(
                  ref,
                  PageTabEntry(slug: p.slug, title: p.title),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class KnowledgeBatchReview extends ConsumerStatefulWidget {
  final List<api.KnowledgeProposal> proposals;
  final Widget Function(api.KnowledgeProposal) preview;
  const KnowledgeBatchReview({
    super.key,
    required this.proposals,
    required this.preview,
  });
  @override
  ConsumerState<KnowledgeBatchReview> createState() => _BatchState();
}

class _BatchState extends ConsumerState<KnowledgeBatchReview> {
  final Set<String> _selected = {};
  bool _busy = false;
  String? _result;
  Future<void> _resolve(bool accept) async {
    setState(() => _busy = true);
    try {
      final results = await ref
          .read(knowledgeRepositoryProvider)
          .resolveBatch(_selected.toList(), accept);
      if (mounted) {
        setState(() {
          _selected.removeAll(results.where((r) => r.success).map((r) => r.id));
          _result = results.map((r) => r.detail).join('\n');
        });
        notifyKnowledgeChanged(ref);
      }
    } catch (e) {
      if (mounted) setState(() => _result = '批量处理失败：$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('批量审阅修订'),
    subtitle: const Text('先查看正文与适用条件，每批最多确认 20 项'),
    children: [
      for (final p in widget.proposals.where(
        (p) => p.pageId != null && p.baseHash != null && p.status == 'pending',
      ))
        Column(
          children: [
            CheckboxListTile(
              title: Text(p.title),
              value: _selected.contains(p.id),
              onChanged: _busy
                  ? null
                  : (v) => setState(() {
                      if (v == true && _selected.length < 20) {
                        _selected.add(p.id);
                      } else {
                        _selected.remove(p.id);
                      }
                    }),
            ),
            ExpansionTile(
              title: const Text('查看本项修订'),
              children: [widget.preview(p)],
            ),
          ],
        ),
      Wrap(
        spacing: 8,
        children: [
          FilledButton(
            onPressed: _busy || _selected.isEmpty ? null : () => _resolve(true),
            child: Text('确认采纳 ${_selected.length} 项'),
          ),
          TextButton(
            onPressed: _busy || _selected.isEmpty
                ? null
                : () => _resolve(false),
            child: const Text('拒绝所选'),
          ),
        ],
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_result != null) Text(_result!),
    ],
  );
}

/// 知识库首页列表的单个知识页卡片。
///
/// 替代原裸 ListTile：带圆角边框 / 表面底、左侧类型徽标（kind 首字 + 色）、
/// 标题加粗单行、摘要灰字两行、右上角阅读状态 chip，整卡可点。
class _LibraryCard extends StatelessWidget {
  final api.LibraryEntry entry;
  final VoidCallback onTap;

  const _LibraryCard({required this.entry, required this.onTap});

  /// 类型中文名（与其它界面 kindLabel 对齐，仅本列表用到的子集）。
  static const Map<String, String> _kindLabels = {
    'source': '原料',
    'note': '笔记',
    'method': '方法',
    'case': '案例',
    'principle': '规律',
    'topic': '主题',
    'person': '联系人',
    'project': '项目',
    'insight': '洞察',
    'asset': '资产',
  };

  /// 阅读状态对应颜色：待读=强调、已读=中性、值得使用/已采用=成功、归档=灰。
  Color _readingColor() {
    switch (entry.readingState) {
      case 'valuable':
      case 'adopted':
        return AppTheme.success;
      case 'read':
        return AppTheme.textSecondary;
      case 'archived':
        return AppTheme.textTertiary;
      case 'unread':
      default:
        return AppTheme.accentPrimary;
    }
  }

  @override
  Widget build(BuildContext context) {
    final kindLabel = _kindLabels[entry.kind] ?? entry.kind;
    final readingLabel =
        readingLabels[entry.readingState] ?? entry.readingState;
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Material(
        color: AppTheme.surface1,
        borderRadius: BorderRadius.circular(10),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(10),
          hoverColor: AppTheme.surface2,
          child: Container(
            padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(10),
              border: Border.all(color: AppTheme.surface3),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // 标题行：类型 chip + 标题 + 阅读状态 chip 同行，省一行纵向空间。
                Row(
                  children: [
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 7,
                        vertical: 3,
                      ),
                      decoration: BoxDecoration(
                        color: AppTheme.accentMuted.withValues(alpha: 0.16),
                        borderRadius: BorderRadius.circular(6),
                        border: Border.all(
                          color: AppTheme.accentMuted.withValues(alpha: 0.4),
                        ),
                      ),
                      child: Text(
                        kindLabel,
                        style: TextStyle(
                          fontSize: 10.5,
                          fontWeight: FontWeight.w600,
                          color: AppTheme.accentPrimary,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        entry.title,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 14.5,
                          fontWeight: FontWeight.w600,
                          height: 1.3,
                          color: AppTheme.textPrimary,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Container(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 7,
                        vertical: 3,
                      ),
                      decoration: BoxDecoration(
                        color: _readingColor().withValues(alpha: 0.14),
                        borderRadius: BorderRadius.circular(20),
                      ),
                      child: Text(
                        readingLabel,
                        style: TextStyle(
                          fontSize: 10.5,
                          fontWeight: FontWeight.w500,
                          color: _readingColor(),
                        ),
                      ),
                    ),
                  ],
                ),
                if (entry.summary.isNotEmpty) ...[
                  const SizedBox(height: 6),
                  Text(
                    entry.summary,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: TextStyle(
                      fontSize: 12.5,
                      height: 1.45,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }
}
