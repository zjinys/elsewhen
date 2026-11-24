part of '../scout_view.dart';

String _preferenceLabel(BuildContext context, String key) => switch (key) {
  'audience' => context.l10n.scoutPreferenceAudience,
  'interests' => context.l10n.scoutPreferenceInterests,
  'capabilities' => context.l10n.scoutPreferenceCapabilities,
  'constraints' => context.l10n.scoutPreferenceConstraints,
  _ => context.l10n.scoutPreferenceBuyers,
};

class _ScoutPreferencesButton extends ConsumerWidget {
  const _ScoutPreferencesButton();
  @override
  Widget build(BuildContext context, WidgetRef ref) => IconButton(
    tooltip: context.l10n.scoutPreferences,
    icon: const Icon(Icons.tune),
    onPressed: () => showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => const _ScoutPreferencesEditor(),
    ),
  );
}

class _ScoutPreferencesEditor extends ConsumerStatefulWidget {
  const _ScoutPreferencesEditor();
  @override
  ConsumerState<_ScoutPreferencesEditor> createState() =>
      _ScoutPreferencesEditorState();
}

class _ScoutPreferencesEditorState
    extends ConsumerState<_ScoutPreferencesEditor> {
  final _fields = <String, TextEditingController>{};
  bool _saving = false;
  @override
  void dispose() {
    for (final field in _fields.values) {
      field.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ref
      .watch(scoutPreferencesProvider)
      .when(
        loading: () => const AlertDialog(content: LinearProgressIndicator()),
        error: (e, _) => AlertDialog(
          content: Text(context.errorMessage(context.l10n.scoutPreferences, e)),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text(context.l10n.close),
            ),
            TextButton(
              onPressed: () => ref.invalidate(scoutPreferencesProvider),
              child: Text(context.l10n.scoutRetry),
            ),
          ],
        ),
        data: (data) {
          for (final key in [
            'audience',
            'interests',
            'capabilities',
            'constraints',
            'reachable_buyers',
          ]) {
            _fields.putIfAbsent(
              key,
              () => TextEditingController(text: '${data[key] ?? ''}'),
            );
          }
          return PopScope(
            canPop: !_saving,
            child: AlertDialog(
              title: Text(context.l10n.scoutPreferences),
              content: SizedBox(
                width: 560,
                child: SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(context.l10n.scoutPreferencesHelp),
                      for (final entry in _fields.entries)
                        Padding(
                          padding: const EdgeInsets.only(top: 16),
                          child: TextField(
                            controller: entry.value,
                            minLines: 2,
                            maxLines: 4,
                            maxLength: 2000,
                            decoration: InputDecoration(
                              labelText: _preferenceLabel(context, entry.key),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
              actions: [
                TextButton(
                  onPressed: _saving
                      ? null
                      : () {
                          for (final c in _fields.values) {
                            c.clear();
                          }
                        },
                  child: Text(context.l10n.scoutWithdrawPreferences),
                ),
                TextButton(
                  onPressed: _saving ? null : () => Navigator.pop(context),
                  child: Text(context.l10n.cancel),
                ),
                FilledButton(
                  onPressed: _saving
                      ? null
                      : () async {
                          setState(() => _saving = true);
                          try {
                            await ref
                                .read(scoutResearchRepositoryProvider)
                                .savePreferences({
                                  for (final e in _fields.entries)
                                    e.key: e.value.text.trim(),
                                }, data['token'] as String?);
                            ref.invalidate(scoutPreferencesProvider);
                            ref.invalidate(wikiPagesProvider);
                            if (context.mounted) Navigator.pop(context);
                          } catch (e) {
                            if (context.mounted) {
                              FeedbackToast.of(context).show(
                                SnackBar(
                                  content: Text(
                                    context.errorMessage(
                                      context.l10n.scoutPreferences,
                                      e,
                                    ),
                                  ),
                                ),
                              );
                            }
                          } finally {
                            if (mounted) setState(() => _saving = false);
                          }
                        },
                  child: Text(context.l10n.scoutSaveResearch),
                ),
              ],
            ),
          );
        },
      );
}

class _PreferenceContext extends ConsumerWidget {
  final void Function(Map<String, dynamic>) onRecord;
  const _PreferenceContext({required this.onRecord});
  @override
  Widget build(BuildContext context, WidgetRef ref) => ref
      .watch(scoutPreferencesProvider)
      .when(
        loading: () => const SizedBox.shrink(),
        error: (_, _) => const _ScoutPreferencesButton(),
        data: (data) {
          final entries = data.entries
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
              )
              .toList();
          return Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (entries.isEmpty)
                Text(context.l10n.scoutPreferencesNeutral)
              else ...[
                for (final e in entries)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Text(
                      '${_preferenceLabel(context, e.key)}: ${e.value}',
                    ),
                  ),
                TextButton(
                  onPressed: () => onRecord(
                    {...data}
                      ..remove('token')
                      ..remove('slug'),
                  ),
                  child: Text(context.l10n.scoutRecordPreferenceBasis),
                ),
              ],
              const Align(
                alignment: Alignment.centerLeft,
                child: _ScoutPreferencesButton(),
              ),
            ],
          );
        },
      );
}
