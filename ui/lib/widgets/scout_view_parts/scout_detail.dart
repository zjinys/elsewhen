part of '../scout_view.dart';

class ScoutFindingDetail extends ConsumerStatefulWidget {
  final ScoutFindingItem item;
  final VoidCallback onClose;
  const ScoutFindingDetail({
    super.key,
    required this.item,
    required this.onClose,
  });
  @override
  ConsumerState<ScoutFindingDetail> createState() => _ScoutFindingDetailState();
}

class _ScoutFindingDetailState extends ConsumerState<ScoutFindingDetail> {
  bool _busy = false;
  final Set<int> _saved = {}, _recorded = {};
  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final scale = chatTypeScaleOf(ref.watch(settingsProvider));
    final detail = ref.watch(scoutDetailProvider(item.id));
    Widget heading(String text) => Padding(
      padding: const EdgeInsets.only(top: 28, bottom: 10),
      child: Text(
        text,
        style: TextStyle(
          fontSize: scale.panel,
          fontWeight: FontWeight.w600,
          color: AppTheme.textPrimary,
        ),
      ),
    );
    return ContentFontScope(
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(12, 8, 8, 0),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    context.l10n.scoutFindings,
                    style: TextStyle(
                      fontSize: scale.meta,
                      color: AppTheme.textTertiary,
                    ),
                  ),
                ),
                IconButton(
                  tooltip: context.l10n.close,
                  onPressed: widget.onClose,
                  icon: const Icon(Icons.close, size: 20),
                ),
              ],
            ),
          ),
          Expanded(
            child: ListView(
              padding: const EdgeInsets.fromLTRB(24, 12, 24, 32),
              children: [
                ScoutSourceBadge(sourceId: item.sourceId),
                const SizedBox(height: 18),
                Text(
                  _kindLabel(context, item.kind),
                  style: TextStyle(
                    fontSize: scale.chip,
                    fontWeight: FontWeight.w600,
                    color: _kindColor(item.kind),
                  ),
                ),
                const SizedBox(height: 10),
                SelectableText(
                  item.title,
                  style: TextStyle(
                    fontSize: scale.panel + 6,
                    fontWeight: FontWeight.w600,
                    height: 1.3,
                    color: AppTheme.textPrimary,
                  ),
                ),
                const SizedBox(height: 10),
                Text(
                  '${_sourceName(ref, item.sourceId)} · ${context.l10n.scoutDetected} ${_exactTime(item.detectedAt)}',
                  style: TextStyle(
                    fontSize: scale.meta,
                    color: AppTheme.textTertiary,
                  ),
                ),
                const SizedBox(height: 16),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    FilledButton.tonalIcon(
                      onPressed: _busy || _saved.contains(item.id)
                          ? null
                          : () => _save(item),
                      icon: const Icon(Icons.bookmark_add_outlined, size: 16),
                      label: Text(
                        _saved.contains(item.id)
                            ? context.l10n.scoutSavedAsMaterial
                            : context.l10n.scoutSaveAsMaterial,
                      ),
                    ),
                    OutlinedButton.icon(
                      onPressed: _busy || _recorded.contains(item.id)
                          ? null
                          : () => _record(item),
                      icon: const Icon(Icons.edit_note, size: 18),
                      label: Text(
                        _recorded.contains(item.id)
                            ? context.l10n.scoutRecordedAsEvent
                            : context.l10n.scoutRecordAsEvent,
                      ),
                    ),
                    if (item.link != null)
                      TextButton.icon(
                        onPressed: () => _openLink(item.link!),
                        icon: const Icon(Icons.open_in_new, size: 16),
                        label: Text(context.l10n.scoutOpenSource),
                      ),
                  ],
                ),
                heading(context.l10n.scoutChanges),
                Text(
                  switch (item.kind) {
                    'new' => context.l10n.scoutNewHelp,
                    'gone' => context.l10n.scoutGoneHelp,
                    _ => context.l10n.scoutChangedHelp,
                  },
                  style: TextStyle(
                    fontSize: scale.bubbleSecondary,
                    color: AppTheme.textSecondary,
                    height: 1.5,
                  ),
                ),
                if (item.kind == 'changed') ...[
                  const SizedBox(height: 12),
                  if (item.delta.isEmpty)
                    Text(context.l10n.scoutNoDelta)
                  else
                    for (final e in item.delta.entries)
                      if (e.value is Map)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 14),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                _fieldLabel(context, e.key),
                                style: TextStyle(
                                  fontSize: scale.meta,
                                  color: AppTheme.textSecondary,
                                ),
                              ),
                              const SizedBox(height: 4),
                              SelectableText(
                                '${_value(e.value['from'])} → ${_value(e.value['to'])}',
                                style: TextStyle(
                                  fontSize: scale.bubbleSecondary,
                                  color: AppTheme.accentPrimary,
                                  height: 1.5,
                                ),
                              ),
                            ],
                          ),
                        ),
                ],
                heading(context.l10n.scoutFullData),
                if (item.subtitle != null)
                  SelectableText(
                    item.subtitle!,
                    style: TextStyle(
                      fontSize: scale.bubbleSecondary,
                      color: AppTheme.textSecondary,
                      height: 1.6,
                    ),
                  ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 16,
                  runSpacing: 8,
                  children: [
                    for (final s in item.signals)
                      Text(
                        '${s.label} ${s.value}',
                        style: TextStyle(
                          fontSize: scale.meta,
                          color: AppTheme.textSecondary,
                        ),
                      ),
                  ],
                ),
                if (item.topics.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: 12),
                    child: Text(
                      item.topics.map((t) => '#$t').join('  '),
                      style: TextStyle(
                        fontSize: scale.meta,
                        color: AppTheme.textTertiary,
                      ),
                    ),
                  ),
                ExpansionTile(
                  tilePadding: EdgeInsets.zero,
                  title: Text(
                    context.l10n.scoutRawData,
                    style: TextStyle(fontSize: scale.meta),
                  ),
                  children: [
                    SelectableText(
                      const JsonEncoder.withIndent('  ').convert(item.payload),
                      style: TextStyle(
                        fontSize: scale.meta,
                        color: AppTheme.textSecondary,
                      ),
                    ),
                  ],
                ),
                heading(context.l10n.scoutHistory),
                Text(
                  context.l10n.scoutHistoryHelp,
                  style: TextStyle(
                    fontSize: scale.meta,
                    color: AppTheme.textTertiary,
                  ),
                ),
                const SizedBox(height: 12),
                detail.when(
                  loading: () => const LinearProgressIndicator(),
                  error: (e, _) => Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        context.l10n.scoutHistoryOffline,
                        style: TextStyle(
                          color: AppTheme.textSecondary,
                          fontSize: scale.meta,
                        ),
                      ),
                      TextButton(
                        onPressed: () =>
                            ref.invalidate(scoutDetailProvider(item.id)),
                        child: Text(context.l10n.scoutRetry),
                      ),
                    ],
                  ),
                  data: (data) {
                    final history = data['history'];
                    final rows = history is List
                        ? history.whereType<Map>().toList().reversed.toList()
                        : <Map>[];
                    if (rows.isEmpty) return Text(context.l10n.scoutNoHistory);
                    return Column(
                      children: [
                        for (final row in rows)
                          ExpansionTile(
                            tilePadding: EdgeInsets.zero,
                            title: Text(
                              _exactTime('${row['taken_at']}'),
                              style: TextStyle(fontSize: scale.meta),
                            ),
                            subtitle: row['item'] is Map
                                ? Text(
                                    (row['item'] as Map).entries
                                        .where((e) => e.value is num)
                                        .take(4)
                                        .map(
                                          (e) =>
                                              '${_fieldLabel(context, '${e.key}')} ${e.value}',
                                        )
                                        .join(' · '),
                                    style: TextStyle(
                                      fontSize: scale.meta,
                                      color: AppTheme.textSecondary,
                                    ),
                                  )
                                : null,
                            children: [
                              for (final e
                                  in (row['item'] is Map
                                          ? row['item'] as Map
                                          : {})
                                      .entries)
                                Padding(
                                  padding: const EdgeInsets.symmetric(
                                    vertical: 6,
                                  ),
                                  child: Align(
                                    alignment: Alignment.centerLeft,
                                    child: SelectableText(
                                      '${_fieldLabel(context, '${e.key}')}: ${_value(e.value)}',
                                      style: TextStyle(
                                        fontSize: scale.meta,
                                        color: AppTheme.textSecondary,
                                      ),
                                    ),
                                  ),
                                ),
                            ],
                          ),
                      ],
                    );
                  },
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Future<void> _openLink(String link) async {
    try {
      if (!await launchUrl(
        Uri.parse(link),
        mode: LaunchMode.externalApplication,
      )) {
        throw StateError('Cannot open link');
      }
    } catch (e) {
      _error(e);
    }
  }

  String _content(ScoutFindingItem item) =>
      '# ${item.title}\n\n${_changeSummary(context, item)}\n\n${context.l10n.scoutDetected}: ${_exactTime(item.detectedAt)}\n\n${item.link ?? item.sourceId}\n\n${const JsonEncoder.withIndent('  ').convert(item.payload)}';
  Future<void> _save(ScoutFindingItem item) async {
    setState(() => _busy = true);
    try {
      final base = ref.read(scoutSettingsProvider).value?.baseUrl ?? '';
      final page = await saveKnowledgeImport(
        context,
        ref,
        ImportFetch(
          sourceUrl: '$base/findings/${item.id}',
          sourceKind: 'webpage',
          title: item.title,
          contentMd: _content(item),
        ),
        ['scout'],
      );
      if (page != null && mounted) {
        ref.invalidate(wikiPagesProvider);
        setState(() => _saved.add(item.id));
      }
    } catch (e) {
      _error(e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _record(ScoutFindingItem item) async {
    final text = await showDialog<String>(
      context: context,
      builder: (_) => _ScoutRecordDialog(
        initial:
            '${item.title}\n${_changeSummary(context, item)}\n${item.link ?? item.sourceId}\n${_exactTime(item.detectedAt)}',
      ),
    );
    if (text == null || !mounted) return;
    setState(() => _busy = true);
    try {
      await ref.read(storageRepositoryProvider).recordEvent(text);
      if (mounted) {
        ref.invalidate(eventsProvider);
        setState(() => _recorded.add(item.id));
      }
    } catch (e) {
      _error(e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _error(Object e) {
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            context.errorMessage(context.l10n.scoutActionFailed, e),
          ),
        ),
      );
    }
  }
}

String _exactTime(String iso) {
  final t = DateTime.tryParse(iso)?.toLocal();
  return t == null ? iso : DateFormat('yyyy-MM-dd HH:mm').format(t);
}

class _ScoutRecordDialog extends StatefulWidget {
  final String initial;
  const _ScoutRecordDialog({required this.initial});
  @override
  State<_ScoutRecordDialog> createState() => _ScoutRecordDialogState();
}

class _ScoutRecordDialogState extends State<_ScoutRecordDialog> {
  late final _controller = TextEditingController(text: widget.initial);
  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(context.l10n.scoutRecordNote),
    content: SizedBox(
      width: 600,
      child: TextField(
        controller: _controller,
        minLines: 5,
        maxLines: 12,
        onChanged: (_) => setState(() {}),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(context.l10n.cancel),
      ),
      FilledButton(
        onPressed: _controller.text.trim().isEmpty
            ? null
            : () => Navigator.pop(context, _controller.text.trim()),
        child: Text(context.l10n.scoutRecordAsEvent),
      ),
    ],
  );
}
