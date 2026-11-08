import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../theme/app_theme.dart';
import '../l10n/localization.dart';

/// 打开「定期问我」设置。习惯页头部入口与空状态共用。
Future<bool?> showHabitNudgeDialog(
  BuildContext context, {
  required WikiPageRef page,
}) {
  return showDialog<bool>(
    context: context,
    builder: (_) => HabitNudgeDialog(page: page),
  );
}

class HabitNudgeDialog extends ConsumerStatefulWidget {
  final WikiPageRef page;

  const HabitNudgeDialog({super.key, required this.page});

  @override
  ConsumerState<HabitNudgeDialog> createState() => _HabitNudgeDialogState();
}

enum _HabitDialogError { question, time, days, responses, load, save, close }

class _HabitNudgeDialogState extends ConsumerState<HabitNudgeDialog> {
  final _question = TextEditingController();
  final _time = TextEditingController();
  final _doneLabel = TextEditingController();
  final _doneText = TextEditingController();
  final _missedLabel = TextEditingController();
  final _missedText = TextEditingController();

  Set<int> _days = {1, 2, 3, 4, 5, 6, 7};
  bool _enabled = true;
  bool _loading = true;
  bool _saving = false;
  _HabitDialogError? _error;
  String? _existingId;
  String _basisExcerpt = '';

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _load();
    });
  }

  @override
  void dispose() {
    _question.dispose();
    _time.dispose();
    _doneLabel.dispose();
    _doneText.dispose();
    _missedLabel.dispose();
    _missedText.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      final suggestion = await repo.getHabitNudgeSuggestion(widget.page.slug);
      final existing = suggestion.existing;
      _existingId = existing?.id;
      _basisExcerpt = existing == null ? suggestion.basisExcerpt : '';
      _question.text = existing?.question ?? suggestion.question;
      _time.text = existing?.localTime ?? suggestion.localTime;
      _doneLabel.text = existing?.doneLabel ?? suggestion.doneLabel;
      _doneText.text = existing?.doneText ?? suggestion.doneText;
      _missedLabel.text = existing?.missedLabel ?? suggestion.missedLabel;
      _missedText.text = existing?.missedText ?? suggestion.missedText;
      final mask = existing?.weekdays ?? suggestion.weekdays;
      _days = {
        for (var i = 1; i <= 7; i++)
          if ((mask & (1 << (i - 1))) != 0) i,
      };
      if (_days.isEmpty) _days = {1, 2, 3, 4, 5, 6, 7};
      _enabled = existing?.enabled ?? true;
      if (!mounted) return;
      setState(() => _loading = false);
    } catch (e) {
      if (!mounted) return;
      context.errorMessage(context.l10n.edit, e);
      setState(() {
        _loading = false;
        _error = _HabitDialogError.load;
      });
    }
  }

  int get _weekdayMask => _days.fold(0, (mask, day) => mask | (1 << (day - 1)));

  bool _validTime(String value) {
    final parts = value.trim().split(':');
    if (parts.length != 2) return false;
    final hour = int.tryParse(parts[0]);
    final minute = int.tryParse(parts[1]);
    if (hour == null || minute == null) return false;
    if (minute < 0 || minute > 59) return false;
    return hour >= 7 && (hour < 22 || (hour == 22 && minute == 0));
  }

  Future<void> _save() async {
    if (_saving) return;
    final question = _question.text.trim();
    final time = _time.text.trim();
    final doneLabel = _doneLabel.text.trim();
    final doneText = _doneText.text.trim();
    final missedLabel = _missedLabel.text.trim();
    final missedText = _missedText.text.trim();
    if (question.isEmpty) {
      setState(() => _error = _HabitDialogError.question);
      return;
    }
    if (!_validTime(time)) {
      setState(() => _error = _HabitDialogError.time);
      return;
    }
    if (_days.isEmpty) {
      setState(() => _error = _HabitDialogError.days);
      return;
    }
    if (doneLabel.isEmpty ||
        doneText.isEmpty ||
        missedLabel.isEmpty ||
        missedText.isEmpty) {
      setState(() => _error = _HabitDialogError.responses);
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.saveHabitNudge(
        api.HabitNudgeInputDto(
          pageSlug: widget.page.slug,
          question: question,
          doneLabel: doneLabel,
          doneText: doneText,
          missedLabel: missedLabel,
          missedText: missedText,
          weekdays: _weekdayMask,
          localTime: time,
          enabled: _enabled,
        ),
      );
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (e) {
      if (!mounted) return;
      context.errorMessage(context.l10n.edit, e);
      setState(() {
        _saving = false;
        _error = _HabitDialogError.save;
      });
    }
  }

  Future<void> _delete() async {
    final id = _existingId;
    if (id == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        backgroundColor: AppTheme.surface1,
        title: Text(context.l10n.closeCheckinsQuestion),
        content: Text(context.l10n.closeCheckinsHelp),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: AppTheme.error),
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text(context.l10n.close),
          ),
        ],
      ),
    );
    if (confirmed != true) return;
    try {
      final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
      await repo.deleteHabitNudge(id);
      if (!mounted) return;
      Navigator.of(context).pop(true);
    } catch (e) {
      if (!mounted) return;
      context.errorMessage(context.l10n.edit, e);
      setState(() => _error = _HabitDialogError.close);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: AppTheme.surface0,
      insetPadding: const EdgeInsets.symmetric(horizontal: 24, vertical: 24),
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(AppTheme.radiusLarge),
      ),
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 640, maxHeight: 760),
        child: Padding(
          padding: const EdgeInsets.fromLTRB(24, 20, 24, 16),
          child: _loading
              ? const SizedBox(
                  height: 220,
                  child: Center(child: CircularProgressIndicator()),
                )
              : SingleChildScrollView(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(
                            Icons.notifications_active_outlined,
                            color: AppTheme.accentPrimary,
                            size: 20,
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              _existingId == null
                                  ? context.l10n.askRegularly
                                  : context.l10n.adjustCheckins,
                              style: TextStyle(
                                color: AppTheme.textPrimary,
                                fontSize: 17,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 6),
                      Text(
                        context.l10n.checkinHelp,
                        style: TextStyle(
                          fontSize: 12,
                          color: AppTheme.textTertiary,
                          height: 1.5,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        context.l10n.checkinSyncHelp,
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textTertiary,
                          height: 1.5,
                        ),
                      ),
                      if (_basisExcerpt.isNotEmpty) ...[
                        const SizedBox(height: 12),
                        Container(
                          width: double.infinity,
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                          decoration: BoxDecoration(
                            color: AppTheme.surface1,
                            borderRadius: BorderRadius.circular(
                              AppTheme.radiusSmall,
                            ),
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                context.l10n.inferenceBasis,
                                style: TextStyle(
                                  fontSize: 11,
                                  fontWeight: FontWeight.w600,
                                  color: AppTheme.textSecondary,
                                ),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                _basisExcerpt,
                                style: TextStyle(
                                  fontSize: 12,
                                  color: AppTheme.textTertiary,
                                  height: 1.5,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ],
                      const SizedBox(height: 16),
                      _label(context.l10n.checkinQuestion),
                      TextField(
                        controller: _question,
                        maxLines: 2,
                        minLines: 1,
                        style: TextStyle(color: AppTheme.textPrimary),
                        decoration: _inputDecoration(
                          context.l10n.checkinQuestionHint,
                        ),
                      ),
                      const SizedBox(height: 14),
                      Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                _label(context.l10n.checkinTime),
                                TextField(
                                  controller: _time,
                                  style: TextStyle(color: AppTheme.textPrimary),
                                  decoration: _inputDecoration('12:30'),
                                ),
                              ],
                            ),
                          ),
                          const SizedBox(width: 16),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                _label(context.l10n.repeat),
                                Text(
                                  _days.length == 7
                                      ? context.l10n.everyDay
                                      : context.l10n.checkinDaysPerWeek(
                                          _days.length,
                                        ),
                                  style: TextStyle(
                                    color: AppTheme.textSecondary,
                                    fontSize: 13,
                                  ),
                                ),
                                const SizedBox(height: 6),
                                Switch.adaptive(
                                  value: _enabled,
                                  activeTrackColor: AppTheme.accentPrimary,
                                  onChanged: (value) =>
                                      setState(() => _enabled = value),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Wrap(
                        spacing: 6,
                        runSpacing: 6,
                        children: [
                          for (var day = 1; day <= 7; day++)
                            FilterChip(
                              label: Text(_dayName(context, day)),
                              selected: _days.contains(day),
                              onSelected: (selected) {
                                setState(() {
                                  if (selected) {
                                    _days.add(day);
                                  } else if (_days.length > 1) {
                                    _days.remove(day);
                                  }
                                });
                              },
                            ),
                        ],
                      ),
                      const SizedBox(height: 16),
                      _label(context.l10n.checkinDoneFields),
                      Row(
                        children: [
                          SizedBox(
                            width: 120,
                            child: TextField(
                              controller: _doneLabel,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration(
                                context.l10n.walkDoneHint,
                              ),
                            ),
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: TextField(
                              controller: _doneText,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration(
                                context.l10n.walkDoneTextHint,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      _label(context.l10n.checkinMissedFields),
                      Row(
                        children: [
                          SizedBox(
                            width: 120,
                            child: TextField(
                              controller: _missedLabel,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration(
                                context.l10n.walkMissedHint,
                              ),
                            ),
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: TextField(
                              controller: _missedText,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration(
                                context.l10n.walkMissedTextHint,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        context.l10n.checkinRecordHelp,
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textTertiary,
                          height: 1.5,
                        ),
                      ),
                      if (_error != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          _errorText(context),
                          style: TextStyle(color: AppTheme.error, fontSize: 12),
                        ),
                      ],
                      const SizedBox(height: 18),
                      Wrap(
                        spacing: 8,
                        runSpacing: 6,
                        alignment: WrapAlignment.end,
                        children: [
                          if (_existingId != null)
                            TextButton.icon(
                              onPressed: _saving ? null : _delete,
                              icon: const Icon(Icons.delete_outline, size: 16),
                              label: Text(context.l10n.closeCheckins),
                              style: TextButton.styleFrom(
                                foregroundColor: AppTheme.error,
                              ),
                            ),
                          TextButton(
                            onPressed: _saving
                                ? null
                                : () => Navigator.of(context).pop(),
                            child: Text(
                              context.l10n.cancel,
                              style: TextStyle(color: AppTheme.textSecondary),
                            ),
                          ),
                          const SizedBox(width: 8),
                          FilledButton(
                            onPressed: _saving ? null : _save,
                            style: FilledButton.styleFrom(
                              backgroundColor: AppTheme.accentPrimary,
                            ),
                            child: _saving
                                ? const SizedBox(
                                    width: 16,
                                    height: 16,
                                    child: CircularProgressIndicator(
                                      strokeWidth: 2,
                                    ),
                                  )
                                : Text(context.l10n.save),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
        ),
      ),
    );
  }

  String _errorText(BuildContext context) => switch (_error!) {
    _HabitDialogError.question => context.l10n.checkinQuestionRequired,
    _HabitDialogError.time => context.l10n.checkinTimeInvalid,
    _HabitDialogError.days => context.l10n.checkinDayRequired,
    _HabitDialogError.responses => context.l10n.checkinResponsesRequired,
    _HabitDialogError.load => context.l10n.operationFailed(context.l10n.load),
    _HabitDialogError.save => context.l10n.operationFailed(context.l10n.save),
    _HabitDialogError.close => context.l10n.operationFailed(context.l10n.close),
  };

  Widget _label(String text) => Padding(
    padding: const EdgeInsets.only(bottom: 6),
    child: Text(
      text,
      style: TextStyle(
        fontSize: 12,
        fontWeight: FontWeight.w600,
        color: AppTheme.textSecondary,
      ),
    ),
  );

  InputDecoration _inputDecoration(String hint) => InputDecoration(
    hintText: hint,
    hintStyle: TextStyle(color: AppTheme.textTertiary, fontSize: 12),
    isDense: true,
    contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 10),
    enabledBorder: OutlineInputBorder(
      borderSide: BorderSide(color: AppTheme.surface3),
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
    ),
    focusedBorder: OutlineInputBorder(
      borderSide: BorderSide(color: AppTheme.accentPrimary),
      borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
    ),
  );

  String _dayName(BuildContext context, int day) =>
      context.l10n.weekdayShort(day.toString());
}
