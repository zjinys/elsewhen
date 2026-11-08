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
      if (mounted) {
        setState(
          () => _error = _failure(
            context,
            e,
            context.l10n.l6LoadLinkableKnowledgea71cb,
          ),
        );
      }
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
      if (mounted) {
        setState(
          () => _error = _failure(
            context,
            e,
            context.l10n.l6LoadResolutionRevision2bf89,
          ),
        );
      }
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
        setState(() => _error = appFailure(e));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => _done
      ? Text(context.l10n.l6ResolutionSavedf80e5)
      : ExpansionTile(
          title: Text(context.l10n.l6LinkedResolutionsb26bd),
          subtitle: Text(context.l10n.l6LinkTheIssueToAKnowledge08e6c),
          onExpansionChanged: (open) {
            if (open && _targets == null && !_busy) _load();
          },
          children: [
            if (_targets?.isEmpty ?? false)
              Text(context.l10n.l6NoKnowledgePageCoversTheseSources10eed),
            if (_targets?.isNotEmpty ?? false)
              DropdownButtonFormField<String>(
                initialValue: _slug,
                isExpanded: true,
                decoration: InputDecoration(
                  labelText:
                      context.l10n.l6KnowledgePageThatResolvesTheIssueb3d20,
                ),
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
              Text(
                context.l10n.l6CurrentRevisionArg0beb34(
                  (_revision!.createdAt).toString(),
                ),
              ),
              ExpansionTile(
                title: Text(context.l10n.l6ReviewRevisionContent73b70),
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
                decoration: InputDecoration(
                  labelText: context
                      .l10n
                      .l6HowThisRevisionResolvesTheDisagreemented5c3,
                ),
                onChanged: (_) => setState(() {}),
              ),
              FilledButton(
                onPressed: _busy || _note.text.trim().isEmpty ? null : _resolve,
                child: Text(context.l10n.l6ConfirmLinkedResolution5427f),
              ),
            ],
            if (_busy) const LinearProgressIndicator(),
            if (_error != null)
              AppErrorMessage(action: context.l10n.load, error: _error!),
            if (_error != null)
              TextButton(
                onPressed: _busy ? null : _load,
                child: Text(context.l10n.l6RefreshLinkableKnowledge5a5b4),
              ),
          ],
        );
}
