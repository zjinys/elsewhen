import 'app_error_notice.dart';
import '../l10n/notice_messages.dart';

import 'package:flutter/material.dart';

import '../bridge/api.dart' as api;
import '../l10n/localization.dart';

class NamedSecretSettings extends StatefulWidget {
  const NamedSecretSettings({super.key});
  @override
  State<NamedSecretSettings> createState() => _NamedSecretSettingsState();
}

class _NamedSecretSettingsState extends State<NamedSecretSettings> {
  late Future<List<api.NamedSecretDto>> items;
  @override
  void initState() {
    super.initState();
    reload();
  }

  void reload() {
    items = api.listNamedSecrets();
  }

  void error(Object e) {
    if (mounted) {
      showAppError(context, context.l10n.save, e);
    }
  }

  Future<void> edit([api.NamedSecretDto? item]) async {
    final name = TextEditingController(text: item?.name);
    final title = TextEditingController(text: item?.title);
    final value = TextEditingController();
    bool hidden = true;
    bool extract = false;
    bool saving = false;
    String? formError;
    final route = DialogRoute<void>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, update) => AlertDialog(
          title: Text(
            item == null ? context.l10n.addSecret : context.l10n.editSecret,
          ),
          content: SizedBox(
            width: 420,
            child: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  TextField(
                    controller: name,
                    enabled: item == null,
                    decoration: InputDecoration(
                      labelText: context.l10n.secretCodeRequired,
                      helperText: item == null
                          ? context.l10n.secretCodeCreateHint
                          : context.l10n.secretCodeEditHint,
                    ),
                  ),
                  const SizedBox(height: 16),
                  TextField(
                    controller: title,
                    maxLength: 128,
                    decoration: InputDecoration(
                      labelText: context.l10n.nameRequired,
                      hintText: context.l10n.secretNameHint,
                    ),
                  ),
                  const SizedBox(height: 8),
                  TextField(
                    controller: value,
                    obscureText: hidden,
                    enableSuggestions: false,
                    autocorrect: false,
                    decoration: InputDecoration(
                      labelText: item == null
                          ? context.l10n.secretValueRequired
                          : context.l10n.newSecretValueOptional,
                      hintText: item == null
                          ? null
                          : context.l10n.keepSecretValueHint,
                      suffixIcon: IconButton(
                        onPressed: () => update(() => hidden = !hidden),
                        icon: Icon(
                          hidden ? Icons.visibility : Icons.visibility_off,
                        ),
                      ),
                    ),
                  ),
                  if (item == null)
                    CheckboxListTile(
                      contentPadding: EdgeInsets.zero,
                      title: Text(context.l10n.replaceMatchingCredentials),
                      subtitle: Text(context.l10n.matchingCredentialsHint),
                      value: extract,
                      onChanged: (v) => update(() => extract = v ?? false),
                    ),
                  if (item != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 16),
                      child: Text(
                        item.references.isEmpty
                            ? context.l10n.notReferenced
                            : context.l10n.secretUsedBy(
                                item.references.join(
                                  context.l10n.listSeparator,
                                ),
                              ),
                      ),
                    ),
                  if (formError != null)
                    Padding(
                      padding: const EdgeInsets.only(top: 12),
                      child: AppErrorMessage(
                        action: context.l10n.save,
                        error: formError!,
                        style: TextStyle(
                          color: Theme.of(context).colorScheme.error,
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogContext),
              child: Text(context.l10n.cancel),
            ),
            FilledButton(
              onPressed: saving
                  ? null
                  : () async {
                      if (title.text.trim().isEmpty) {
                        update(
                          () => formError = appNotice('nameRequiredError'),
                        );
                        return;
                      }
                      if (item == null &&
                          (name.text.trim().isEmpty || value.text.isEmpty)) {
                        update(
                          () => formError = appNotice('secretFieldsRequired'),
                        );
                        return;
                      }
                      update(() {
                        saving = true;
                        formError = null;
                      });
                      try {
                        if (item != null) {
                          await api.editNamedSecret(
                            name: item.name,
                            title: title.text.trim(),
                            value: value.text.isEmpty ? null : value.text,
                          );
                        } else if (extract) {
                          await api.extractNamedSecret(
                            name: name.text.trim(),
                            title: title.text,
                            value: value.text,
                          );
                        } else {
                          await api.saveNamedSecret(
                            name: name.text.trim(),
                            title: title.text,
                            value: value.text,
                          );
                        }
                        if (dialogContext.mounted) Navigator.pop(dialogContext);
                        if (mounted) setState(reload);
                      } catch (e) {
                        if (dialogContext.mounted) {
                          update(() {
                            saving = false;
                            formError = appFailure(e);
                          });
                        }
                      }
                    },
              child: Text(saving ? context.l10n.saving : context.l10n.save),
            ),
          ],
        ),
      ),
    );
    await Navigator.of(context).push(route);
    await route.completed;
    name.dispose();
    title.dispose();
    value.dispose();
  }

  Future<void> remove(api.NamedSecretDto item) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l10n.deleteSecretTitle),
        content: Text(
          item.references.isEmpty
              ? context.l10n.deleteSecretConfirmation(item.name)
              : context.l10n.secretStillUsedBy(
                  item.references.join(context.l10n.listSeparator),
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.l10n.cancel),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l10n.delete),
          ),
        ],
      ),
    );
    if (ok == true) {
      try {
        await api.deleteNamedSecret(name: item.name, force: true);
        if (mounted) setState(reload);
      } catch (e) {
        error(e);
      }
    }
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(context.l10n.secretVaultDescription),
      TextButton.icon(
        onPressed: edit,
        icon: const Icon(Icons.add),
        label: Text(context.l10n.addSecret),
      ),
      FutureBuilder<List<api.NamedSecretDto>>(
        future: items,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return AppErrorMessage(
              action: context.l10n.load,
              error: snapshot.error!,
            );
          }
          if (!snapshot.hasData) return const LinearProgressIndicator();
          return Column(
            children: snapshot.data!
                .map(
                  (s) => ListTile(
                    title: Text(s.title.isEmpty ? s.name : s.title),
                    subtitle: Text(
                      '${s.name} · ${s.references.isEmpty ? context.l10n.notReferenced : s.references.join(context.l10n.listSeparator)}',
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        IconButton(
                          tooltip: context.l10n.editSecret,
                          onPressed: () => edit(s),
                          icon: const Icon(Icons.edit_outlined),
                        ),
                        IconButton(
                          tooltip: context.l10n.delete,
                          onPressed: () => remove(s),
                          icon: const Icon(Icons.delete_outline),
                        ),
                      ],
                    ),
                  ),
                )
                .toList(),
          );
        },
      ),
    ],
  );
}
