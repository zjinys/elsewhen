part of '../knowledge_maintenance_sections.dart';

//! 问题处置 section

class KnowledgeIssueResolutionSection extends ConsumerStatefulWidget {
  final api.KnowledgeIssue issue;
  final VoidCallback onChanged;
  const KnowledgeIssueResolutionSection({
    super.key,
    required this.issue,
    required this.onChanged,
  });
  @override
  ConsumerState<KnowledgeIssueResolutionSection> createState() =>
      _IssueResolutionState();
}

class _IssueResolutionState
    extends ConsumerState<KnowledgeIssueResolutionSection> {
  List<api.WikiPageDto>? _targets;
  String? _slug, _error;
  api.KnowledgeRevisionDto? _revision;
  final _note = TextEditingController();
  bool _busy = false, _done = false;
  @override
  void dispose() {
    _note.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _busy = true;
      _error = null;
      _slug = null;
      _revision = null;
    });
    try {
      final targets = await ref
          .read(knowledgeRepositoryProvider)
          .resolutionTargets(widget.issue.fingerprint);
      if (mounted) setState(() => _targets = targets);
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取可关联知识'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _select(String? slug) async {
    setState(() {
      _slug = slug;
      _revision = null;
      _busy = true;
      _error = null;
    });
    try {
      final revisions = await ref
          .read(knowledgeRepositoryProvider)
          .revisions(slug!);
      if (mounted) setState(() => _revision = revisions.firstOrNull);
    } catch (e) {
      if (mounted) setState(() => _error = _failure(e, '读取解决修订'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _resolve() async {
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await ref
          .read(knowledgeRepositoryProvider)
          .resolveIssue(
            widget.issue.fingerprint,
            _slug!,
            _revision!.id,
            _note.text,
          );
      if (!mounted) return;
      setState(() => _done = true);
      widget.onChanged();
    } catch (e) {
      if (mounted) {
        setState(() => _error = '${_failure(e, '关联解决记录')} 来源或修订可能已变化，请重新选择。');
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => _done
      ? const Text('已保存解决记录')
      : ExpansionTile(
          title: const Text('关联解决记录'),
          subtitle: const Text('将问题关联到覆盖全部依据的知识修订'),
          onExpansionChanged: (open) {
            if (open && _targets == null && !_busy) _load();
          },
          children: [
            if (_targets?.isEmpty ?? false)
              const Text('暂无覆盖这些来源的知识页。请先在相关主题中整理分歧及适用条件，再回到此处关联。'),
            if (_targets?.isNotEmpty ?? false)
              DropdownButtonFormField<String>(
                initialValue: _slug,
                isExpanded: true,
                decoration: const InputDecoration(labelText: '解决问题的知识页'),
                items: [
                  for (final p in _targets!)
                    DropdownMenuItem(
                      value: p.slug,
                      child: Text(p.title, overflow: TextOverflow.ellipsis),
                    ),
                ],
                onChanged: _busy ? null : _select,
              ),
            if (_revision != null) ...[
              Text('当前修订：${_revision!.createdAt}'),
              ExpansionTile(
                title: const Text('核对修订正文'),
                children: [
                  SelectionArea(
                    child: MarkdownView(markdown: _revision!.contentMd),
                  ),
                ],
              ),
              TextField(
                controller: _note,
                maxLength: 1000,
                minLines: 2,
                maxLines: 4,
                decoration: const InputDecoration(labelText: '这次修订如何处理分歧'),
                onChanged: (_) => setState(() {}),
              ),
              FilledButton(
                onPressed: _busy || _note.text.trim().isEmpty ? null : _resolve,
                child: const Text('确认关联解决'),
              ),
            ],
            if (_busy) const LinearProgressIndicator(),
            if (_error != null) Text(_error!),
            if (_error != null)
              TextButton(
                onPressed: _busy ? null : _load,
                child: const Text('刷新可关联知识'),
              ),
          ],
        );
}
