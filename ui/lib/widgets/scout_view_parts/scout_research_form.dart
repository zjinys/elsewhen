part of '../scout_view.dart';

String _researchKey(Map<String, dynamic> value) =>
    jsonEncode([value['instance_id'], value['product_id']]);
final _researchDraftsProvider =
    NotifierProvider<
      StateHolder<Map<String, Map<String, dynamic>>>,
      Map<String, Map<String, dynamic>>
    >(() => StateHolder({}));

Future<void> _editResearch(
  BuildContext context,
  WidgetRef ref,
  ScoutProduct product,
  String instance, {
  String? initialState,
}) async {
  try {
    final studies = await ref.read(scoutResearchRepositoryProvider).studies();
    if (!context.mounted) return;
    final existing = studies
        .where(
          (s) =>
              s['instance_id'] == instance &&
              (s['product_id'] == product.id ||
                  (product.data['aliases'] as List? ?? []).contains(
                    s['product_id'],
                  )),
        )
        .firstOrNull;
    final initial =
        existing ??
        {
          'instance_id': instance,
          'product_id': product.id,
          'title': product.title,
          'state': 'research',
          'evidence': product.data,
          'created_at': DateTime.now().toUtc().toIso8601String(),
        };
    await _openResearchEditor(
      context,
      ref,
      initial,
      initialState: initialState,
    );
    ref.invalidate(scoutStudiesProvider);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.errorMessage(context.l10n.scoutStudies, e)),
        ),
      );
    }
  }
}

Future<void> _openResearchEditor(
  BuildContext context,
  WidgetRef ref,
  Map<String, dynamic> initial, {
  String? initialState,
}) async {
  try {
    final draft =
        ref.read(_researchDraftsProvider)[_researchKey(initial)] ??
        await ref
            .read(scoutResearchRepositoryProvider)
            .draft('${initial['instance_id']}', '${initial['product_id']}');
    if (!context.mounted) return;
    await showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => _ResearchEditor(
        initial: {
          ...draft ?? initial,
          ...?(initialState == null ? null : {'state': initialState}),
        },
      ),
    );
    ref.invalidate(scoutStudiesProvider);
    ref.invalidate(scoutDraftsProvider);
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(context.errorMessage(context.l10n.scoutStudies, e)),
        ),
      );
    }
  }
}

String _stateLabel(BuildContext context, String state) => switch (state) {
  'validating' => context.l10n.scoutStateValidating,
  'watching' => context.l10n.scoutStateWatching,
  'paused' => context.l10n.scoutStatePaused,
  'rejected' => context.l10n.scoutStateRejected,
  _ => context.l10n.scoutStateResearch,
};

final _studySearchProvider = NotifierProvider<StateHolder<String>, String>(
  () => StateHolder(''),
);

class ScoutResearchList extends ConsumerWidget {
  const ScoutResearchList({super.key});
  @override
  Widget build(BuildContext context, WidgetRef ref) => Column(
    children: [
      Padding(
        padding: const EdgeInsets.all(16),
        child: Row(
          children: [
            Expanded(
              child: Text(
                context.l10n.scoutStudies,
                style: TextStyle(
                  fontSize: 20,
                  fontWeight: FontWeight.w600,
                  color: AppTheme.textPrimary,
                ),
              ),
            ),
            const _ScoutPreferencesButton(),
          ],
        ),
      ),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16),
        child: Text(
          context.l10n.scoutResearchHelp,
          style: TextStyle(color: AppTheme.textSecondary, fontSize: 12),
        ),
      ),
      Padding(
        padding: const EdgeInsets.all(16),
        child: TextField(
          decoration: InputDecoration(
            hintText: context.l10n.scoutResearchSearch,
            prefixIcon: const Icon(Icons.search),
          ),
          onChanged: (v) =>
              ref.read(_studySearchProvider.notifier).set(v.toLowerCase()),
        ),
      ),
      ref
          .watch(scoutDraftsProvider)
          .when(
            loading: () => const SizedBox.shrink(),
            error: (e, _) =>
                Text(context.errorMessage(context.l10n.scoutDrafts, e)),
            data: (drafts) => drafts.isEmpty
                ? const SizedBox.shrink()
                : ExpansionTile(
                    title: Text(context.l10n.scoutDraftCount(drafts.length)),
                    children: [
                      SizedBox(
                        height: 220,
                        child: ListView(
                          children: [
                            for (final draft in drafts)
                              ListTile(
                                title: Text(
                                  '${draft['title'] ?? ''}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                ),
                                subtitle: Text(context.l10n.scoutLocalDraft),
                                onTap: () =>
                                    _openResearchEditor(context, ref, draft),
                                trailing: IconButton(
                                  tooltip: context.l10n.scoutDiscardDraft,
                                  icon: const Icon(Icons.delete_outline),
                                  onPressed: () async {
                                    try {
                                      await ref
                                          .read(scoutResearchRepositoryProvider)
                                          .deleteDraft(
                                            '${draft['instance_id']}',
                                            '${draft['product_id']}',
                                          );
                                      ref
                                          .read(
                                            _researchDraftsProvider.notifier,
                                          )
                                          .set(
                                            {
                                              ...ref.read(
                                                _researchDraftsProvider,
                                              ),
                                            }..remove(_researchKey(draft)),
                                          );
                                      ref.invalidate(scoutDraftsProvider);
                                    } catch (e) {
                                      if (context.mounted) {
                                        ScaffoldMessenger.of(context)
                                            .showSnackBar(
                                              SnackBar(
                                                content: Text(
                                                  context.errorMessage(
                                                    context.l10n.scoutDrafts,
                                                    e,
                                                  ),
                                                ),
                                              ),
                                            );
                                      }
                                    }
                                  },
                                ),
                              ),
                          ],
                        ),
                      ),
                    ],
                  ),
          ),
      Expanded(
        child: ref
            .watch(scoutStudiesProvider)
            .when(
              loading: () => const Center(child: CircularProgressIndicator()),
              error: (e, _) => Center(
                child: Text(context.errorMessage(context.l10n.scoutStudies, e)),
              ),
              data: (all) {
                final query = ref.watch(_studySearchProvider);
                final studies = all
                    .where(
                      (s) =>
                          '${s['title']} ${scoutStudyThemes(s).join(' ')} ${s['state']} ${s['question']}'
                              .toLowerCase()
                              .contains(query),
                    )
                    .toList();
                return studies.isEmpty
                    ? Center(child: Text(context.l10n.scoutStudyEmpty))
                    : ListView.builder(
                        padding: const EdgeInsets.all(16),
                        itemCount: studies.length,
                        itemBuilder: (context, i) {
                          final study = studies[i];
                          return Card(
                            child: InkWell(
                              onTap: () async {
                                await _openResearchEditor(context, ref, study);
                                ref.invalidate(scoutStudiesProvider);
                              },
                              child: Padding(
                                padding: const EdgeInsets.all(16),
                                child: Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(
                                      '${study['title']}',
                                      style: TextStyle(
                                        fontSize: 17,
                                        fontWeight: FontWeight.w600,
                                        color: AppTheme.textPrimary,
                                      ),
                                    ),
                                    const SizedBox(height: 8),
                                    Text(
                                      _stateLabel(context, '${study['state']}'),
                                      style: TextStyle(
                                        color: AppTheme.accentPrimary,
                                      ),
                                    ),
                                    Wrap(
                                      children: [
                                        for (final theme in scoutStudyThemes(
                                          study,
                                        ))
                                          TextButton.icon(
                                            icon: const Icon(
                                              Icons.compare_arrows,
                                              size: 16,
                                            ),
                                            label: Text(theme),
                                            onPressed: () => showDialog<void>(
                                              context: context,
                                              builder: (_) => _StudyComparison(
                                                theme: theme,
                                                studies: all
                                                    .where(
                                                      (s) =>
                                                          scoutStudyThemes(s)
                                                              .contains(theme),
                                                    )
                                                    .take(5)
                                                    .toList(),
                                              ),
                                            ),
                                          ),
                                      ],
                                    ),
                                    if (study['question'] != null)
                                      Text(
                                        '${study['question']}',
                                        maxLines: 2,
                                        overflow: TextOverflow.ellipsis,
                                      ),
                                  ],
                                ),
                              ),
                            ),
                          );
                        },
                      );
              },
            ),
      ),
    ],
  );
}

class _ResearchEditor extends ConsumerStatefulWidget {
  final Map<String, dynamic> initial;
  const _ResearchEditor({required this.initial});
  @override
  ConsumerState<_ResearchEditor> createState() => _ResearchEditorState();
}

class _ResearchEditorState extends ConsumerState<_ResearchEditor> {
  late String _state = '${widget.initial['state']}';
  late final _fields = {
    for (final key in [
      'reason',
      'buyer',
      'offer',
      'alternatives',
      'question',
      'experiment',
      'deadline',
      'criteria',
      'feedback',
      'sources',
      'theme',
      'fit',
      'misfit',
    ])
      key: TextEditingController(
        text: key == 'theme'
            ? scoutStudyThemes(widget.initial).join('\n')
            : '${widget.initial[key] ?? ''}',
      ),
  };
  late final List<Map<String, dynamic>> _materials =
      (widget.initial['materials'] as List? ?? [])
          .whereType<Map>()
          .map((e) => Map<String, dynamic>.from(e))
          .toList();
  late bool _analysis = widget.initial['analysis_enabled'] == true;
  Map<String, dynamic>? _preferencesBasis;
  late String? _analysisDecision =
      widget.initial['analysis_decision'] as String?;
  late String? _analysisBasis =
      widget.initial['analysis_selected_basis'] as String?;
  bool _saving = false;
  Timer? _draftTimer;
  Future<void> _draftWrite = Future.value();
  Object? _draftError;
  @override
  void initState() {
    super.initState();
    for (final field in _fields.values) {
      field.addListener(_scheduleDraft);
    }
  }

  void _scheduleDraft() {
    if (_saving) return;
    _draftTimer?.cancel();
    _draftTimer = Timer(const Duration(milliseconds: 600), _queueDraft);
  }

  Future<void> _queueDraft() {
    final document = _document();
    final repository = ref.read(scoutResearchRepositoryProvider);
    ref.read(_researchDraftsProvider.notifier).set({
      ...ref.read(_researchDraftsProvider),
      _researchKey(widget.initial): document,
    });
    _draftWrite = _draftWrite.then((_) async {
      try {
        await repository.saveDraft(document);
        _draftError = null;
      } catch (e) {
        _draftError = e;
      }
      if (mounted) setState(() {});
    });
    return _draftWrite;
  }

  @override
  void dispose() {
    _draftTimer?.cancel();
    for (final c in _fields.values) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => CallbackShortcuts(
    bindings: {
      const SingleActivator(LogicalKeyboardKey.escape): () {
        if (!_saving) _close();
      },
    },
    child: Focus(
      autofocus: true,
      child: PopScope(
        canPop: false,
        onPopInvokedWithResult: (didPop, _) {
          if (!didPop && !_saving) _close();
        },
        child: AlertDialog(
          title: Text('${widget.initial['title']}'),
          content: SizedBox(
            width: 640,
            child: SingleChildScrollView(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(context.l10n.scoutMyHypothesis),
                  if (_draftError != null)
                    Text(
                      context.l10n.scoutDraftWriteFailed,
                      style: TextStyle(color: AppTheme.warning),
                    ),
                  const SizedBox(height: 12),
                  DropdownButtonFormField<String>(
                    initialValue: _state,
                    decoration: InputDecoration(
                      labelText: context.l10n.scoutState,
                    ),
                    items: [
                      for (final s in [
                        'research',
                        'validating',
                        'watching',
                        'paused',
                        'rejected',
                      ])
                        DropdownMenuItem(
                          value: s,
                          child: Text(_stateLabel(context, s)),
                        ),
                    ],
                    onChanged: _saving
                        ? null
                        : (s) {
                            setState(() => _state = s!);
                            _scheduleDraft();
                          },
                  ),
                  for (final key in ['reason', 'buyer', 'offer', 'question'])
                    _field(key),
                  ExpansionTile(
                    title: Text(context.l10n.scoutValidationPlan),
                    children: [
                      for (final key in [
                        'alternatives',
                        'experiment',
                        'deadline',
                        'criteria',
                        'feedback',
                      ])
                        _field(key),
                    ],
                  ),
                  ExpansionTile(
                    title: Text(context.l10n.scoutResearchContext),
                    children: [
                      _field('theme'),
                      _PreferenceContext(
                        onRecord: (value) => setState(() {
                          _preferencesBasis = value;
                          _scheduleDraft();
                        }),
                      ),
                      if (_preferencesBasis != null ||
                          widget.initial['preferences_basis'] != null)
                        ExpansionTile(
                          title: Text(
                            context.l10n.scoutPreferenceBasisRecorded,
                          ),
                          children: [
                            for (final e
                                in ((_preferencesBasis ??
                                            widget.initial['preferences_basis'])
                                        as Map)
                                    .entries
                                    .where(
                                      (e) =>
                                          [
                                            'audience',
                                            'interests',
                                            'capabilities',
                                            'constraints',
                                            'reachable_buyers',
                                          ].contains(e.key) &&
                                          '${e.value}'.trim().isNotEmpty,
                                    ))
                              Padding(
                                padding: const EdgeInsets.symmetric(
                                  vertical: 4,
                                ),
                                child: SelectableText(
                                  '${_preferenceLabel(context, '${e.key}')}: ${e.value}',
                                ),
                              ),
                          ],
                        ),
                      _field('fit'),
                      _field('misfit'),
                      _field('sources'),
                    ],
                  ),
                  const SizedBox(height: 12),
                  for (final m in _materials)
                    ExpansionTile(
                      title: Text(
                        '${_evidenceType(context, '${m['type']}')} · ${m['url']}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      trailing: IconButton(
                        tooltip: context.l10n.scoutRemoveEvidence,
                        icon: const Icon(Icons.remove_circle_outline),
                        onPressed: _saving
                            ? null
                            : () {
                                setState(() => _materials.remove(m));
                                _scheduleDraft();
                              },
                      ),
                      children: [
                        SelectableText('${m['body']}'),
                        Text('${m['observed_at']}'),
                        if ('${m['author'] ?? ''}'.isNotEmpty)
                          Text('${m['author']}'),
                        if ('${m['published_at'] ?? ''}'.isNotEmpty)
                          Text('${m['published_at']}'),
                        if ('${m['language'] ?? ''}'.isNotEmpty)
                          Text('${m['language']}'),
                      ],
                    ),
                  TextButton.icon(
                    onPressed: _saving || _materials.length >= 20
                        ? null
                        : () async {
                            final evidence =
                                await showDialog<Map<String, dynamic>>(
                                  context: context,
                                  builder: (_) => const _EvidenceEditor(),
                                );
                            if (evidence != null &&
                                mounted &&
                                !_materials.any(
                                  (m) => m['id'] == evidence['id'],
                                )) {
                              setState(() => _materials.add(evidence));
                              _scheduleDraft();
                            }
                          },
                    icon: const Icon(Icons.add_link),
                    label: Text(context.l10n.scoutAddEvidence),
                  ),
                  CheckboxListTile(
                    value: _analysis,
                    onChanged: _saving
                        ? null
                        : (v) {
                            setState(() => _analysis = v ?? false);
                            _scheduleDraft();
                          },
                    title: Text(context.l10n.scoutAnalysisEnabled),
                    subtitle: Text(context.l10n.scoutAnalysisHelp),
                  ),
                  if (widget.initial['slug'] is String)
                    _AnalysisCandidate(
                      slug: widget.initial['slug'] as String,
                      onAccept: (text, basis) {
                        setState(() {
                          _fields['reason']!.text = text;
                          _analysisDecision = 'accepted';
                          _analysisBasis = basis;
                          _scheduleDraft();
                        });
                      },
                      onReject: () {
                        setState(() => _analysisDecision = 'rejected');
                        _scheduleDraft();
                      },
                    ),
                  Text(
                    context.l10n.scoutSourceMaterialHelp,
                    style: TextStyle(
                      fontSize: 12,
                      color: AppTheme.textSecondary,
                    ),
                  ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: _saving ? null : _close,
              child: Text(context.l10n.scoutKeepDraft),
            ),
            FilledButton(
              onPressed: _saving ? null : _save,
              child: Text(context.l10n.scoutSaveResearch),
            ),
          ],
        ),
      ),
    ),
  );
  Widget _field(String key) => Padding(
    padding: const EdgeInsets.only(top: 14, bottom: 4),
    child: TextField(
      controller: _fields[key],
      enabled: !_saving,
      minLines: 2,
      maxLines: 5,
      decoration: InputDecoration(
        labelText: switch (key) {
          'reason' => context.l10n.scoutStudyReason,
          'buyer' => context.l10n.scoutBuyer,
          'offer' => context.l10n.scoutOffer,
          'alternatives' => context.l10n.scoutAlternatives,
          'question' => context.l10n.scoutQuestion,
          'experiment' => context.l10n.scoutExperiment,
          'deadline' => context.l10n.scoutDeadline,
          'criteria' => context.l10n.scoutCriteria,
          'feedback' => context.l10n.scoutFeedback,
          'theme' => context.l10n.scoutTheme,
          'fit' => context.l10n.scoutFit,
          'misfit' => context.l10n.scoutMisfit,
          _ => context.l10n.scoutEvidence,
        },
      ),
    ),
  );
  Map<String, dynamic> _document() => {
    ...widget.initial,
    'state': _state,
    'themes': _fields['theme']!.text
        .split('\n')
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toSet()
        .toList(),
    if (_preferencesBasis != null) 'preferences_basis': _preferencesBasis,
    'materials': _materials,
    'analysis_enabled': _analysis,
    if (_analysisBasis != null) 'analysis_selected_basis': _analysisBasis,
    if (_analysisDecision != null) 'analysis_decision': _analysisDecision,
    for (final e in _fields.entries) e.key: e.value.text.trim(),
    'updated_at': DateTime.now().toUtc().toIso8601String(),
  };
  Future<void> _close() async {
    _draftTimer?.cancel();
    setState(() => _saving = true);
    await _queueDraft();
    if (!mounted) return;
    if (_draftError != null) {
      setState(() => _saving = false);
      return;
    }
    ref.invalidate(scoutDraftsProvider);
    Navigator.pop(context);
  }

  Future<void> _save() async {
    _draftTimer?.cancel();
    setState(() => _saving = true);
    try {
      await _draftWrite;
      final document = _document();
      await ref
          .read(scoutResearchRepositoryProvider)
          .save(document, widget.initial['token'] as String?);
      if (mounted) {
        final drafts = {...ref.read(_researchDraftsProvider)}
          ..remove(_researchKey(widget.initial));
        ref.read(_researchDraftsProvider.notifier).set(drafts);
        ref.invalidate(scoutStudiesProvider);
        ref.invalidate(scoutDraftsProvider);
        ref.invalidate(wikiPagesProvider);
        Navigator.pop(context);
      }
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              context.errorMessage(context.l10n.scoutSaveResearch, e),
            ),
          ),
        );
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }
}

String _evidenceType(BuildContext context, String value) => switch (value) {
  'review' => context.l10n.scoutEvidenceReview,
  'pricing' => context.l10n.scoutEvidencePricing,
  _ => context.l10n.scoutEvidenceOther,
};

class _EvidenceEditor extends ConsumerStatefulWidget {
  const _EvidenceEditor();
  @override
  ConsumerState<_EvidenceEditor> createState() => _EvidenceEditorState();
}

class _EvidenceEditorState extends ConsumerState<_EvidenceEditor> {
  final _url = TextEditingController(),
      _body = TextEditingController(),
      _author = TextEditingController(),
      _language = TextEditingController(),
      _published = TextEditingController();
  String? _urlError;
  String _type = 'review';
  bool _fetching = false;
  bool _truncated = false;
  @override
  void dispose() {
    _url.dispose();
    _body.dispose();
    _author.dispose();
    _language.dispose();
    _published.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(context.l10n.scoutAddEvidence),
    content: SizedBox(
      width: 600,
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _url,
              decoration: InputDecoration(
                labelText: context.l10n.scoutEvidenceUrl,
                errorText: _urlError,
              ),
            ),
            TextButton(
              onPressed: _fetching
                  ? null
                  : () async {
                      setState(() => _fetching = true);
                      try {
                        final text = await ref
                            .read(scoutResearchRepositoryProvider)
                            .fetchEvidence(_url.text.trim());
                        if (mounted) {
                          setState(() {
                            _truncated = text.runes.length > 8000;
                            _body.text = String.fromCharCodes(
                              text.runes.take(8000),
                            );
                          });
                        }
                      } catch (e) {
                        if (context.mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                            SnackBar(
                              content: Text(
                                context.errorMessage(
                                  context.l10n.scoutFetchEvidence,
                                  e,
                                ),
                              ),
                            ),
                          );
                        }
                      } finally {
                        if (mounted) setState(() => _fetching = false);
                      }
                    },
              child: Text(context.l10n.scoutFetchEvidence),
            ),
            DropdownButtonFormField<String>(
              initialValue: _type,
              items: [
                for (final s in ['review', 'pricing', 'other'])
                  DropdownMenuItem(
                    value: s,
                    child: Text(
                      s == 'review'
                          ? context.l10n.scoutEvidenceReview
                          : s == 'pricing'
                          ? context.l10n.scoutEvidencePricing
                          : context.l10n.scoutEvidenceOther,
                    ),
                  ),
              ],
              onChanged: (s) => setState(() => _type = s!),
            ),
            const SizedBox(height: 12),
            if (_truncated) Text(context.l10n.scoutEvidenceTruncated),
            TextField(
              controller: _body,
              minLines: 5,
              maxLines: 12,
              maxLength: 8000,
              decoration: InputDecoration(
                labelText: context.l10n.scoutEvidenceBody,
              ),
              onChanged: (_) => setState(() {}),
            ),
            ExpansionTile(
              title: Text(context.l10n.scoutEvidenceMetadata),
              children: [
                TextField(
                  controller: _author,
                  maxLength: 300,
                  decoration: InputDecoration(
                    labelText: context.l10n.scoutEvidenceAuthor,
                  ),
                ),
                TextField(
                  controller: _language,
                  maxLength: 80,
                  decoration: InputDecoration(
                    labelText: context.l10n.scoutEvidenceLanguage,
                  ),
                ),
                TextField(
                  controller: _published,
                  maxLength: 80,
                  decoration: InputDecoration(
                    labelText: context.l10n.scoutEvidencePublished,
                  ),
                ),
              ],
            ),
            Text(context.l10n.scoutEvidenceProvenance),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(context.l10n.cancel),
      ),
      FilledButton(
        onPressed: _body.text.trim().length < 8
            ? null
            : () {
                final url = Uri.tryParse(_url.text.trim());
                if (url == null ||
                    !['http', 'https'].contains(url.scheme) ||
                    url.host.isEmpty ||
                    url.userInfo.isNotEmpty) {
                  setState(
                    () => _urlError = context.l10n.scoutEvidenceInvalidUrl,
                  );
                  return;
                }
                final body = _body.text.trim();
                Navigator.pop(context, {
                  'id': sha256
                      .convert(utf8.encode('${url.toString()}\n$body'))
                      .toString(),
                  'url': url.toString(),
                  'body': body,
                  'type': _type,
                  'author': _author.text.trim(),
                  'language': _language.text.trim(),
                  'published_at': _published.text.trim(),
                  'observed_at': DateTime.now().toUtc().toIso8601String(),
                  'origin': 'user-confirmed-excerpt',
                  'truncated_import': _truncated,
                });
              },
        child: Text(context.l10n.confirm),
      ),
    ],
  );
}

class _AnalysisCandidate extends ConsumerWidget {
  final String slug;
  final void Function(String, String) onAccept;
  final VoidCallback onReject;
  const _AnalysisCandidate({
    required this.slug,
    required this.onAccept,
    required this.onReject,
  });
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(scoutResearchAnalysisProvider(slug))
      .when(
        loading: () => const LinearProgressIndicator(),
        error: (e, _) => Text(context.l10n.scoutAnalysisWaiting),
        data: (data) {
          final result = data['result'] as Map?;
          if (result == null) return Text(context.l10n.scoutAnalysisWaiting);
          return ExpansionTile(
            title: Text(context.l10n.scoutAnalysisCandidate),
            children: [
              if (data['stale'] == true) Text(context.l10n.scoutAnalysisStale),
              for (final claim in (result['claims'] as List? ?? []))
                Padding(
                  padding: const EdgeInsets.all(8),
                  child: SelectableText(
                    '${claim['text']}\n“${claim['quote']}” · ${claim['evidence_id']}',
                  ),
                ),
              Text('${result['coverage']}'),
              for (final h in (result['hypotheses'] as List? ?? []))
                Padding(
                  padding: const EdgeInsets.all(8),
                  child: Column(
                    children: [
                      Text(
                        context.l10n.scoutSupportingEvidence(
                          (h['support'] as List? ?? []).join(', '),
                        ),
                      ),
                      Text(
                        context.l10n.scoutCounterEvidence(
                          (h['counter'] as List? ?? []).join(', '),
                        ),
                      ),
                      SelectableText(
                        '${h['text']}\n${h['unknowns']}\n${h['experiment']}',
                      ),
                      TextButton(
                        onPressed: data['stale'] == true
                            ? null
                            : () => onAccept(
                                '${h['text']}\n${h['unknowns']}\n${h['experiment']}',
                                '${data['basis']}',
                              ),
                        child: Text(context.l10n.scoutAcceptCandidate),
                      ),
                    ],
                  ),
                ),
              TextButton(
                onPressed: onReject,
                child: Text(context.l10n.scoutRejectCandidate),
              ),
            ],
          );
        },
      );
}

class _StudyComparison extends StatelessWidget {
  final List<Map<String, dynamic>> studies;
  final String theme;
  const _StudyComparison({required this.studies, required this.theme});
  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(theme),
    content: SizedBox(
      width: 900,
      child: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              context.l10n.scoutComparisonScope(
                studies.length,
                studies
                    .expand(
                      (s) =>
                          (s['evidence'] as Map? ?? {})['sources'] as List? ??
                          [],
                    )
                    .whereType<Map>()
                    .map((s) => s['source_id'])
                    .where((s) => s != null)
                    .toSet()
                    .length,
              ),
            ),
            const SizedBox(height: 12),
            SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: DataTable(
                columns: [
                  DataColumn(label: Text(context.l10n.scoutAllProducts)),
                  DataColumn(label: Text(context.l10n.scoutState)),
                  DataColumn(label: Text(context.l10n.scoutBuyer)),
                  DataColumn(label: Text(context.l10n.scoutKnownPrice)),
                  DataColumn(label: Text(context.l10n.scoutQuestion)),
                  DataColumn(label: Text(context.l10n.scoutMisfit)),
                ],
                rows: [
                  for (final s in studies)
                    DataRow(
                      cells: [
                        DataCell(
                          SizedBox(width: 140, child: Text('${s['title']}')),
                        ),
                        DataCell(Text(_stateLabel(context, '${s['state']}'))),
                        DataCell(
                          SizedBox(
                            width: 150,
                            child: Text('${s['buyer'] ?? '—'}'),
                          ),
                        ),
                        DataCell(
                          Text(
                            _price(
                              context,
                              ScoutProduct(
                                Map<String, dynamic>.from(
                                  s['evidence'] as Map? ?? {},
                                ),
                              ),
                            ),
                          ),
                        ),
                        DataCell(
                          SizedBox(
                            width: 180,
                            child: Text('${s['question'] ?? '—'}'),
                          ),
                        ),
                        DataCell(
                          SizedBox(
                            width: 180,
                            child: Text('${s['misfit'] ?? '—'}'),
                          ),
                        ),
                      ],
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: Text(context.l10n.close),
      ),
    ],
  );
}
