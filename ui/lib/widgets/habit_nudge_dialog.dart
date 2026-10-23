import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../bridge/api.dart' as api;
import '../bridge/rust_bridge_repository.dart';
import '../models/wiki_page.dart';
import '../theme/app_theme.dart';

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
  String? _error;
  String? _existingId;
  String _basisExcerpt = '';

  @override
  void initState() {
    super.initState();
    _load();
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
      setState(() {
        _loading = false;
        _error = e.toString().replaceFirst('Exception: ', '');
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
      setState(() => _error = '问法不能为空');
      return;
    }
    if (!_validTime(time)) {
      setState(() => _error = '时间需在 07:00–22:00 之间，格式 HH:MM');
      return;
    }
    if (_days.isEmpty) {
      setState(() => _error = '至少选择一天');
      return;
    }
    if (doneLabel.isEmpty ||
        doneText.isEmpty ||
        missedLabel.isEmpty ||
        missedText.isEmpty) {
      setState(() => _error = '按钮和记录句都不能为空');
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
      setState(() {
        _saving = false;
        _error = e.toString().replaceFirst('Exception: ', '');
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
        title: const Text('关闭定期问候？'),
        content: const Text('已产生的历史问候和回答会保留，之后不再主动问你。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: const Text('取消'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: AppTheme.error),
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: const Text('关闭'),
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
      setState(() => _error = e.toString().replaceFirst('Exception: ', ''));
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
                              _existingId == null ? '定期问我' : '调整定期问候',
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
                        '到点后我会在主对话里发一条消息。所有内容先由你确认，'
                        '不会自动修改习惯页。',
                        style: TextStyle(
                          fontSize: 12,
                          color: AppTheme.textTertiary,
                          height: 1.5,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        '多设备同步时只有主设备会在到点后写入问候；'
                        '其他设备同步显示，不会重复发问。',
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
                                '推断依据',
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
                      _label('问法'),
                      TextField(
                        controller: _question,
                        maxLines: 2,
                        minLines: 1,
                        style: TextStyle(color: AppTheme.textPrimary),
                        decoration: _inputDecoration('例如：今天午饭后散步了吗？'),
                      ),
                      const SizedBox(height: 14),
                      Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                _label('每天什么时候问'),
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
                                _label('重复'),
                                Text(
                                  _days.length == 7
                                      ? '每天'
                                      : '每周 ${_days.length} 天',
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
                              label: Text(_dayName(day)),
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
                      _label('完成按钮与记录'),
                      Row(
                        children: [
                          SizedBox(
                            width: 120,
                            child: TextField(
                              controller: _doneLabel,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration('走了'),
                            ),
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: TextField(
                              controller: _doneText,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration('今天午饭后散步了'),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      _label('未完成按钮与记录'),
                      Row(
                        children: [
                          SizedBox(
                            width: 120,
                            child: TextField(
                              controller: _missedLabel,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration('没走'),
                            ),
                          ),
                          const SizedBox(width: 10),
                          Expanded(
                            child: TextField(
                              controller: _missedText,
                              style: TextStyle(color: AppTheme.textPrimary),
                              decoration: _inputDecoration('今天还没走'),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 10),
                      Text(
                        '按钮文字尽量短；记录句会作为一条真实记录写入，'
                        '「没完成」默认只留在对话里，不生成事件。',
                        style: TextStyle(
                          fontSize: 11,
                          color: AppTheme.textTertiary,
                          height: 1.5,
                        ),
                      ),
                      if (_error != null) ...[
                        const SizedBox(height: 12),
                        Text(
                          _error!,
                          style: TextStyle(color: AppTheme.error, fontSize: 12),
                        ),
                      ],
                      const SizedBox(height: 18),
                      Row(
                        children: [
                          if (_existingId != null)
                            TextButton.icon(
                              onPressed: _saving ? null : _delete,
                              icon: const Icon(Icons.delete_outline, size: 16),
                              label: const Text('关闭定期问候'),
                              style: TextButton.styleFrom(
                                foregroundColor: AppTheme.error,
                              ),
                            ),
                          const Spacer(),
                          TextButton(
                            onPressed: _saving
                                ? null
                                : () => Navigator.of(context).pop(),
                            child: Text(
                              '取消',
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
                                : const Text('保存'),
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

  static String _dayName(int day) =>
      const ['一', '二', '三', '四', '五', '六', '日'][day - 1];
}
