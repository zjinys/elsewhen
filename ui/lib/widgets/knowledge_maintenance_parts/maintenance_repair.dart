part of '../knowledge_maintenance_sections.dart';

//! 知识源修复 section

class KnowledgeSourceRepairSection extends ConsumerStatefulWidget {
  final String slug;
  final bool hasEvents;
  final VoidCallback onChanged;
  final Widget Function(api.KnowledgeProposal) previewBuilder;
  const KnowledgeSourceRepairSection({
    super.key,
    required this.slug,
    required this.hasEvents,
    required this.onChanged,
    required this.previewBuilder,
  });
  @override
  ConsumerState<KnowledgeSourceRepairSection> createState() =>
      _SourceRepairState();
}

class _SourceRepairState extends ConsumerState<KnowledgeSourceRepairSection> {
  List<api.KnowledgeRepairSource>? _sources;
  final Set<String> _selected = {};
  bool _busy = false;
  String? _error;
  String _search = '';
  api.KnowledgeProposal? _preview;

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      final items = await ref
          .read(knowledgeRepositoryProvider)
          .repairSources(widget.slug);
      if (!mounted) return;
      setState(() {
        _sources = items;
        _selected
          ..clear()
          ..addAll(
            items
                .where((s) => s.selected && s.eligible)
                .map((s) => s.snapshotId),
          );
      });
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取可用来源'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _prepare() async {
    setState(() {
      _busy = true;
      _error = null;
      _preview = null;
    });
    try {
      final repo = ref.read(knowledgeRepositoryProvider);
      final id = await repo.repair(widget.slug, _selected.toList());
      if (!mounted) return;
      widget.onChanged();
      final data = await repo.details(widget.slug);
      if (!mounted) return;
      setState(
        () => _preview = data.proposals.where((p) => p.id == id).firstOrNull,
      );
    } catch (e) {
      if (mounted) {
        setState(() => _error = _repairFailure(e));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: const Text('修复来源依据'),
    subtitle: const Text('保留有效来源，或选择替代原料；审阅后更新知识'),
    onExpansionChanged: (open) {
      if (open && _sources == null && !_busy) _load();
    },
    children: [
      const Text('不认可或已归档的来源不能继续支持新结论。下方默认保留有效来源，原知识在确认修订前保留。'),
      if (_sources != null) ...[
        TextField(
          decoration: const InputDecoration(
            labelText: '查找替代原料',
            prefixIcon: Icon(Icons.search),
          ),
          onChanged: (value) => setState(() => _search = value),
        ),
        if (_sources!.length > 50)
          const Text('列表显示前 50 项，可输入标题定位其他原料。已选来源仍保留。'),
        ConstrainedBox(
          constraints: const BoxConstraints(maxHeight: 300),
          child: ListView(
            shrinkWrap: true,
            children: [
              for (final source
                  in _sources!
                      .where(
                        (s) => s.title.toLowerCase().contains(
                          _search.toLowerCase(),
                        ),
                      )
                      .take(50))
                CheckboxListTile(
                  title: Text('${source.title} · v${source.version}'),
                  subtitle: Text(
                    !source.eligible
                        ? '不可用，将从新修订依据中排除'
                        : source.selected
                        ? '当前知识的来源'
                        : '可作为替代来源',
                  ),
                  value: _selected.contains(source.snapshotId),
                  onChanged: _busy || !source.eligible
                      ? null
                      : (value) => setState(() {
                          value == true
                              ? _selected.add(source.snapshotId)
                              : _selected.remove(source.snapshotId);
                        }),
                ),
            ],
          ),
        ),
        if (_selected.isEmpty && !widget.hasEvents)
          const Text('剩余依据不足。请选择或先导入有效原料；当前知识仍待补证据。'),
        if (_selected.length > 8) const Text('最多选择 8 份原料，请缩小范围。'),
        Wrap(
          spacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            FilledButton.icon(
              onPressed:
                  _busy ||
                      (_selected.isEmpty && !widget.hasEvents) ||
                      _selected.length > 8
                  ? null
                  : _prepare,
              icon: const Icon(Icons.fact_check_outlined),
              label: const Text('准备修复建议'),
            ),
            TextButton(
              onPressed: _busy ? null : _load,
              child: const Text('刷新来源'),
            ),
          ],
        ),
      ],
      if (_busy) const LinearProgressIndicator(),
      if (_error != null) Text(_error!),
      if (_error != null && _sources == null)
        TextButton(
          onPressed: _busy ? null : _load,
          child: const Text('重新读取来源'),
        ),
      if (_preview != null) widget.previewBuilder(_preview!),
    ],
  );
}
