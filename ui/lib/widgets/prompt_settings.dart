import 'dart:convert';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../l10n/localization.dart';
import 'app_error_notice.dart';

class PromptSettings extends StatefulWidget {
  const PromptSettings({super.key, this.showDiagnostics = false});
  final bool showDiagnostics;
  @override
  State<PromptSettings> createState() => _PromptSettingsState();
}

class _PromptSettingsState extends State<PromptSettings> {
  Future<List<api.PromptDefinitionDto>>? items;
  String templateLocale = 'zh-Hans';
  Future<(api.PromptLanguagePreferencesDto, api.KnowledgeLanguageDto)>?
  languagePreferences;
  @override
  void initState() {
    super.initState();
    // 按需加载，避免设置页的只读预览在桥接尚未初始化的测试/启动阶段触发 FFI。
  }

  void reload() {
    setState(() {
      items = api.listPromptDefinitions(locale: templateLocale);
      languagePreferences = loadLanguagePreferences();
    });
  }

  Future<(api.PromptLanguagePreferencesDto, api.KnowledgeLanguageDto)>
  loadLanguagePreferences() async => (
    await api.getPromptLanguagePreferences(),
    await api.getKnowledgeLanguage(),
  );

  String sourceLabel(BuildContext context, String source) => switch (source) {
    'explicit_request' => context.l10n.languageSourceExplicit,
    'fixed_preference' => context.l10n.languageSourcePreference,
    'current_message' => context.l10n.languageSourceCurrent,
    'conversation' => context.l10n.languageSourceConversation,
    'fallback' => context.l10n.languageSourceFallback,
    'dataset_policy' => context.l10n.languageSourceDataset,
    _ => context.l10n.languageSourceLegacy,
  };

  void error(Object e) {
    if (mounted) {
      showAppError(context, context.l10n.edit, e);
    }
  }

  String titleOf(api.PromptDefinitionDto item) => switch (item.key) {
    'chat.personal.policy' => context.l10n.promptPersonalPolicy,
    'chat.fixed_contract' => context.l10n.promptChatContract,
    'chat.context.fixed_contract' => context.l10n.promptContextContract,
    'chat.execution.fixed_contract' => context.l10n.promptExecutionContract,
    'task.processing.fixed_contract' => context.l10n.promptProcessingContract,
    'chat.reply.fixed_contract' => context.l10n.promptReplyContract,
    'chat.lookup.fixed_contract' => context.l10n.promptLookupContract,
    'chat.supplement.fixed_contract' => context.l10n.promptSupplementContract,
    'knowledge.processing.fixed_contract' =>
      context.l10n.promptKnowledgeProcessingContract,
    'tool.descriptions.fixed_contract' =>
      context.l10n.promptToolDescriptionsContract,
    'tool.metadata.fixed_contract' => context.l10n.promptToolMetadataContract,
    'summary.fixed_contract' => context.l10n.promptSummaryContract,
    'reading.fixed_contract' => context.l10n.promptReadingContract,
    'digest.fixed_contract' => context.l10n.promptDigestContract,
    'summary.policy' => context.l10n.promptSummaryPolicy,
    'reading.policy' => context.l10n.promptReadingPolicy,
    'digest.policy' => context.l10n.promptDigestPolicy,
    'analysis.policy' => context.l10n.promptAnalysisPolicy,
    _ => item.title,
  };

  String fallbackOf(api.PromptDefinitionDto item) {
    final parts = item.fallback.split(':');
    if (parts.first == 'incompatible_revision') {
      return context.l10n.promptIncompatibleFallback;
    }
    if (parts.length == 3 && parts.first == 'parent_locale') {
      return context.l10n.promptParentFallback(parts[1], parts[2]);
    }
    if (parts.length == 3 && parts.first == 'missing_translation') {
      return context.l10n.promptMissingTranslation(parts[1], parts[2]);
    }
    return item.fallback;
  }

  Future<void> edit(api.PromptDefinitionDto item) async {
    final activated = await showDialog<bool>(
      context: context,
      builder: (_) => _PromptEditorDialog(
        item: item,
        title: titleOf(item),
        fallback: fallbackOf(item),
      ),
    );
    if (mounted && activated == true) reload();
  }

  Future<void> history(api.PromptDefinitionDto item) async {
    try {
      final revisions = await api.listPromptRevisions(
        key: item.key,
        locale: item.locale,
        offset: 0,
      );
      if (!mounted) {
        return;
      }
      await showDialog<void>(
        context: context,
        builder: (dialogContext) => AlertDialog(
          title: Text(context.l10n.promptHistoryTitle(titleOf(item))),
          content: SizedBox(
            width: 700,
            height: 500,
            child: ListView(
              children: revisions
                  .map(
                    (r) => ExpansionTile(
                      title: Text(
                        '${r.origin == 'builtin' ? context.l10n.builtin : context.l10n.custom} · ${r.createdAt}',
                      ),
                      subtitle: Text(
                        r.id == item.currentRevisionId
                            ? context.l10n.currentlyActive
                            : r.note,
                      ),
                      children: [
                        SelectableText(r.body),
                        if (item.editable)
                          TextButton(
                            onPressed: () async {
                              try {
                                await api.activatePromptRevision(
                                  key: item.key,
                                  locale: item.locale,
                                  revisionId: r.id,
                                  expectedGeneration: item.generation,
                                );
                                if (dialogContext.mounted) {
                                  Navigator.pop(dialogContext);
                                }
                                if (mounted) {
                                  reload();
                                }
                              } catch (e) {
                                error(e);
                              }
                            },
                            child: Text(context.l10n.activateRevision),
                          ),
                      ],
                    ),
                  )
                  .toList(),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(context.l10n.close),
            ),
          ],
        ),
      );
    } catch (e) {
      error(e);
    }
  }

  Future<void> reset(api.PromptDefinitionDto item) async {
    try {
      await api.activatePromptRevision(
        key: item.key,
        locale: item.locale,
        revisionId: null,
        expectedGeneration: item.generation,
      );
      if (mounted) {
        reload();
      }
    } catch (e) {
      error(e);
    }
  }

  Future<void> runs() async {
    try {
      final bundles = await api.listPromptBundles(offset: 0);
      if (!mounted) {
        return;
      }
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(context.l10n.recentRunVersions),
          content: SizedBox(
            width: 700,
            height: 500,
            child: ListView(
              children: bundles
                  .map(
                    (b) => ExpansionTile(
                      title: Text('${b.ownerKind} · ${b.createdAt}'),
                      subtitle: Text(b.ownerId),
                      children: [
                        Text(context.l10n.runLanguageUi(b.uiLocale)),
                        Text(
                          context.l10n.runLanguageInstructions(
                            b.instructionLocale,
                          ),
                        ),
                        Text(
                          context.l10n.runLanguageKnowledge(
                            b.knowledgeOutputLocale,
                          ),
                        ),
                        Builder(
                          builder: (context) {
                            final policy = jsonDecode(
                              b.responsePolicyJson,
                            ) as Map<String, dynamic>;
                            return Column(
                              children: [
                                Text(
                                  policy['mode'] == 'preserve_source'
                                      ? context.l10n.runLanguagePreserveSource
                                      : context.l10n.runLanguageResponse(
                                          policy['locale'] as String? ??
                                              'zh-Hans',
                                          sourceLabel(
                                            context,
                                            policy['source'] as String? ??
                                                'legacy',
                                          ),
                                        ),
                                ),
                                if (policy['locale_resolution'] != null)
                                  Text(
                                    context.l10n.runLanguageFallback(
                                      policy['locale_resolution'] as String,
                                    ),
                                  ),
                              ],
                            );
                          },
                        ),
                        for (var i = 0; i < b.keys.length; i++)
                          ExpansionTile(
                            title: Text(b.keys[i]),
                            subtitle: Text(
                              '${b.requestedLocales[i]} → ${b.resolvedLocales[i]} · ${b.revisionIds[i]}',
                            ),
                            children: [
                              if (b.fallbackReasons[i].isNotEmpty)
                                Text(
                                  context.l10n.runLanguageFallback(
                                    b.fallbackReasons[i],
                                  ),
                                ),
                              SelectableText(b.bodies[i]),
                            ],
                          ),
                      ],
                    ),
                  )
                  .toList(),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: Text(context.l10n.close),
            ),
          ],
        ),
      );
    } catch (e) {
      error(e);
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(context.l10n.promptLocalDescription),
      const SizedBox(height: 12),
      SizedBox(
        width: 280,
        child: DropdownButtonFormField<String>(
          key: const ValueKey('prompt-template-language'),
          initialValue: templateLocale,
          decoration: InputDecoration(
            labelText: context.l10n.promptTemplateLanguage,
          ),
          items: [
            DropdownMenuItem(
              value: 'zh-Hans',
              child: Text(context.l10n.chinese),
            ),
            DropdownMenuItem(value: 'en', child: Text(context.l10n.english)),
          ],
          onChanged: (value) {
            if (value == null || value == templateLocale) return;
            setState(() {
              templateLocale = value;
              // Browsing another variant does not activate it or call a model.
              if (items != null) {
                items = api.listPromptDefinitions(locale: value);
              }
            });
          },
        ),
      ),
      if (widget.showDiagnostics) ...[
        const SizedBox(height: 8),
        Wrap(
          spacing: 8,
          children: [
            FilledButton.tonalIcon(
              onPressed: reload,
              icon: const Icon(Icons.refresh),
              label: Text(context.l10n.loadPrompts),
            ),
            TextButton(
              onPressed: runs,
              child: Text(context.l10n.viewRunVersions),
            ),
          ],
        ),
      ],
      if (languagePreferences != null)
        FutureBuilder<
          (api.PromptLanguagePreferencesDto, api.KnowledgeLanguageDto)
        >(
          future: languagePreferences,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Text(context.l10n.operationFailed(context.l10n.load));
            }
            if (!snapshot.hasData) return const SizedBox.shrink();
            return _LanguagePolicySettings(
              key: ValueKey(
                'language-policy-${snapshot.data!.$1.generation}-${snapshot.data!.$2.generation}',
              ),
              preferences: snapshot.data!.$1,
              knowledge: snapshot.data!.$2,
            );
          },
        ),
      if (items != null)
        FutureBuilder<List<api.PromptDefinitionDto>>(
          future: items!,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Text(context.l10n.operationFailed(context.l10n.load));
            }
            if (!snapshot.hasData) {
              return const LinearProgressIndicator();
            }
            return Column(
              children: snapshot.data!
                  .map(
                    (item) => ListTile(
                      title: Text(titleOf(item)),
                      subtitle: Text(
                        '${item.editable ? context.l10n.editable : context.l10n.readOnlyContract} · ${context.l10n.approximateTokens(item.tokens)}',
                      ),
                      onTap: () => edit(item),
                      trailing: item.editable
                          ? PopupMenuButton<String>(
                              onSelected: (v) {
                                if (v == 'history') {
                                  history(item);
                                } else {
                                  reset(item);
                                }
                              },
                              itemBuilder: (_) => [
                                PopupMenuItem(
                                  value: 'history',
                                  child: Text(context.l10n.revisionHistory),
                                ),
                                PopupMenuItem(
                                  value: 'reset',
                                  child: Text(context.l10n.restoreDefault),
                                ),
                              ],
                            )
                          : const Icon(Icons.lock_outline),
                    ),
                  )
                  .toList(),
            );
          },
        ),
    ],
  );
}

// The controller belongs to the route, including its dismissal animation.
// showDialog resolves before the route's widgets have all been disposed.
class _PromptEditorDialog extends StatefulWidget {
  final api.PromptDefinitionDto item;
  final String title, fallback;
  const _PromptEditorDialog({
    required this.item,
    required this.title,
    required this.fallback,
  });
  @override
  State<_PromptEditorDialog> createState() => _PromptEditorDialogState();
}

class _PromptEditorDialogState extends State<_PromptEditorDialog> {
  late final body = TextEditingController(text: widget.item.currentBody);
  String? saved;
  bool busy = false;
  @override
  void dispose() {
    body.dispose();
    super.dispose();
  }

  Future<void> save() async {
    setState(() => busy = true);
    final text = body.text;
    try {
      final item = widget.item;
      final id = await api.savePromptRevision(
        key: item.key,
        locale: item.locale,
        body: text,
        parentRevisionId: item.resolvedLocale == item.locale
            ? item.currentRevisionId
            : null,
        note: context.l10n.settingsEditNote,
      );
      if (mounted) setState(() => saved = body.text == text ? id : null);
    } catch (e) {
      if (mounted) showError(e);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> activateRevision() async {
    final id = saved;
    if (id == null) return;
    setState(() => busy = true);
    try {
      await api.activatePromptRevision(
        key: widget.item.key,
        locale: widget.item.locale,
        revisionId: id,
        expectedGeneration: widget.item.generation,
      );
      if (mounted) Navigator.of(context).pop(true);
    } catch (e) {
      if (mounted) showError(e);
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  void showError(Object error) =>
      showAppError(context, context.l10n.save, error);

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    return AlertDialog(
      title: Text(widget.title),
      content: SizedBox(
        width: 700,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                '${item.resolvedLocale} · ${context.l10n.approximateTokens(item.tokens)}${widget.fallback.isEmpty ? '' : ' · ${widget.fallback}'}',
              ),
              const SizedBox(height: 12),
              TextField(
                controller: body,
                onChanged: (_) => setState(() => saved = null),
                readOnly: !item.editable || busy,
                minLines: 8,
                maxLines: 16,
                decoration: InputDecoration(
                  border: const OutlineInputBorder(),
                  labelText: context.l10n.promptBody,
                ),
              ),
              const SizedBox(height: 8),
              Text(
                item.editable
                    ? context.l10n.promptEditHint
                    : context.l10n.promptReadOnlyHint,
              ),
              if (saved != null) Text(context.l10n.promptDraftSaved),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(context.l10n.close),
        ),
        TextButton(
          onPressed: busy
              ? null
              : () => setState(() {
                  body.text = item.builtinBody;
                  saved = null;
                }),
          child: Text(context.l10n.viewDefault),
        ),
        if (item.editable) ...[
          TextButton(
            onPressed: busy ? null : save,
            child: Text(context.l10n.saveDraft),
          ),
          FilledButton(
            onPressed: saved == null || busy ? null : activateRevision,
            child: Text(context.l10n.activateDraft),
          ),
        ],
      ],
    );
  }
}

class _LanguagePolicySettings extends StatefulWidget {
  final api.PromptLanguagePreferencesDto preferences;
  final api.KnowledgeLanguageDto knowledge;
  const _LanguagePolicySettings({
    super.key,
    required this.preferences,
    required this.knowledge,
  });
  @override
  State<_LanguagePolicySettings> createState() =>
      _LanguagePolicySettingsState();
}

class _LanguagePolicySettingsState extends State<_LanguagePolicySettings> {
  late api.PromptLanguagePreferencesDto preferences = widget.preferences;
  late api.KnowledgeLanguageDto knowledge = widget.knowledge;
  bool busy = false;
  bool failed = false;

  Future<void> update(String? instruction, String response) async {
    setState(() {
      busy = true;
      failed = false;
    });
    try {
      final saved = await api.updatePromptLanguagePreferences(
        instructionLocale: instruction,
        responseMode: response == 'auto' ? 'auto' : 'fixed',
        responseLocale: response == 'auto' ? null : response,
        expectedGeneration: preferences.generation,
      );
      if (mounted) setState(() => preferences = saved);
    } catch (e) {
      if (mounted) {
        showAppError(context, context.l10n.save, e);
        setState(() => failed = true);
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> updateKnowledge(String locale) async {
    setState(() {
      busy = true;
      failed = false;
    });
    try {
      final saved = await api.updateKnowledgeLanguage(
        outputLocale: locale,
        expectedGeneration: knowledge.generation,
      );
      if (mounted) setState(() => knowledge = saved);
    } catch (error) {
      if (mounted) {
        showAppError(context, context.l10n.save, error);
        setState(() => failed = true);
      }
      try {
        final latest = await api.getKnowledgeLanguage();
        if (mounted) setState(() => knowledge = latest);
      } catch (_) {
        // Keep the last displayed version after a failed refresh. A later save
        // still carries its generation and cannot overwrite concurrent edits.
      }
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final response = preferences.responseMode == 'auto'
        ? 'auto'
        : preferences.responseLocale!;
    final instruction = preferences.instructionLocale ?? 'default';
    List<DropdownMenuItem<String>> choices(
      String defaultValue,
      String label,
      String selected,
    ) => [
      DropdownMenuItem(value: defaultValue, child: Text(label)),
      DropdownMenuItem(value: 'zh-Hans', child: Text(context.l10n.chinese)),
      DropdownMenuItem(value: 'en', child: Text(context.l10n.english)),
      if (![defaultValue, 'zh-Hans', 'en'].contains(selected))
        DropdownMenuItem(value: selected, child: Text(selected)),
    ];
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Wrap(
            spacing: 16,
            runSpacing: 12,
            children: [
              SizedBox(
                width: 280,
                child: DropdownButtonFormField<String>(
                  key: ValueKey(
                    'ai-response-language-$response-${preferences.generation}',
                  ),
                  initialValue: response,
                  isExpanded: true,
                  decoration: InputDecoration(
                    labelText: context.l10n.aiResponseLanguage,
                  ),
                  items: choices(
                    'auto',
                    context.l10n.languageAutomatic,
                    response,
                  ),
                  onChanged: busy
                      ? null
                      : (value) {
                          if (value != null) {
                            update(preferences.instructionLocale, value);
                          }
                        },
                ),
              ),
              SizedBox(
                width: 280,
                child: DropdownButtonFormField<String>(
                  key: ValueKey(
                    'ai-instruction-language-$instruction-${preferences.generation}',
                  ),
                  initialValue: instruction,
                  isExpanded: true,
                  decoration: InputDecoration(
                    labelText: context.l10n.aiInstructionLanguage,
                  ),
                  items: choices(
                    'default',
                    context.l10n.instructionFollowContext,
                    instruction,
                  ),
                  onChanged: busy
                      ? null
                      : (value) {
                          if (value != null) {
                            update(value == 'default' ? null : value, response);
                          }
                        },
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text(context.l10n.languagePolicyHelp),
          const SizedBox(height: 12),
          SizedBox(
            width: 280,
            child: DropdownButtonFormField<String>(
              key: ValueKey('knowledge-language-${knowledge.generation}'),
              initialValue: knowledge.outputLocale,
              isExpanded: true,
              decoration: InputDecoration(
                labelText: context.l10n.knowledgeLanguage,
              ),
              items: [
                DropdownMenuItem(
                  value: 'zh-Hans',
                  child: Text(context.l10n.chinese),
                ),
                DropdownMenuItem(
                  value: 'en',
                  child: Text(context.l10n.english),
                ),
                if (!['zh-Hans', 'en'].contains(knowledge.outputLocale))
                  DropdownMenuItem(
                    value: knowledge.outputLocale,
                    child: Text(knowledge.outputLocale),
                  ),
              ],
              onChanged: busy || !knowledge.canEdit
                  ? null
                  : (value) {
                      if (value != null && value != knowledge.outputLocale) {
                        updateKnowledge(value);
                      }
                    },
            ),
          ),
          const SizedBox(height: 8),
          Text(context.l10n.knowledgeLanguageHelp),
          if (!knowledge.supported)
            Text(context.l10n.knowledgeLanguageUnsupported),
          if (!knowledge.canEdit)
            Text(
              knowledge.editBlockReason == 'owner_required'
                  ? context.l10n.knowledgeLanguageOwnerOnly
                  : context.l10n.knowledgeLanguageCapabilityRequired,
            ),
          if (busy) const LinearProgressIndicator(),
          if (failed) Text(context.l10n.operationFailed(context.l10n.save)),
        ],
      ),
    );
  }
}
