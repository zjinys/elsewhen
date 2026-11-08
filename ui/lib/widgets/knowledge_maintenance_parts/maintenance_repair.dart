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
      if (mounted) {
        setState(
          () => _error = _failure(
            context,
            e,
            context.l10n.l6LoadAvailableSourcesefee2,
          ),
        );
      }
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
        setState(() => _error = _repairFailure(context, e));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ExpansionTile(
    title: Text(context.l10n.l6RepairSourceEvidence7c494),
    subtitle: Text(context.l10n.l6KeepValidSourcesOrChooseReplacementse7069),
    onExpansionChanged: (open) {
      if (open && _sources == null && !_busy) _load();
    },
    children: [
      Text(context.l10n.l6RejectedOrArchivedSourcesCannotSupport758ff),
      if (_sources != null) ...[
        TextField(
          decoration: InputDecoration(
            labelText: context.l10n.l6FindReplacementSources05a6a,
            prefixIcon: Icon(Icons.search),
          ),
          onChanged: (value) => setState(() => _search = value),
        ),
        if (_sources!.length > 50)
          Text(context.l10n.l6TheFirst50SourcesAreShown7e891),
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
                        ? context
                              .l10n
                              .l6UnavailableExcludedFromEvidenceForThe84882
                        : source.selected
                        ? context.l10n.l6SourcesForThisKnowledgec088b
                        : context.l10n.l6AvailableAsAReplacementSourceda692,
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
          Text(context.l10n.l6ThereIsNotEnoughEvidenceSelect1b56c),
        if (_selected.length > 8)
          Text(context.l10n.l6ChooseUpTo8SourcesNarrow5655d),
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
              label: Text(context.l10n.l6PrepareRepairProposale29a1),
            ),
            TextButton(
              onPressed: _busy ? null : _load,
              child: Text(context.l10n.l6RefreshSources59684),
            ),
          ],
        ),
      ],
      if (_busy) const LinearProgressIndicator(),
      if (_error != null)
        AppErrorMessage(action: context.l10n.load, error: _error!),
      if (_error != null && _sources == null)
        TextButton(
          onPressed: _busy ? null : _load,
          child: Text(context.l10n.l6ReloadSources59500),
        ),
      if (_preview != null) widget.previewBuilder(_preview!),
    ],
  );
}
