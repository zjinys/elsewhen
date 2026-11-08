part of '../settings_screen.dart';

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，receiver 类型不变

//! 记忆设置、用量图表、推文服务与字体/字号设置。
extension _UsageFonts on _SettingsScreenState {
  Widget _buildMemorySettings(AppSettings settings) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: context.l10n.strategyType,
          value: settings.memory.strategyType,
          items: [
            DropdownMenuItem(
              value: 'simple',
              child: Text(context.l10n.simpleMemory),
            ),
            DropdownMenuItem(
              value: 'sliding-window',
              child: Text(context.l10n.slidingWindowMemory),
            ),
          ],
          onChanged: (value) {
            if (value != null) {
              ref
                  .read(settingsProvider.notifier)
                  .updateMemory(settings.memory.copyWith(strategyType: value));
            }
          },
        ),
        const SizedBox(height: 16),
        if (settings.memory.strategyType == 'simple')
          _buildTextField(
            label: context.l10n.maximumMessages,
            controller: _maxMessagesController,
            hint: '20',
            keyboardType: TextInputType.number,
          ),
        if (settings.memory.strategyType == 'sliding-window')
          _buildTextField(
            label: context.l10n.memoryBudget,
            controller: _maxTokensController,
            hint: context.l10n.memoryBudgetHint,
            keyboardType: TextInputType.number,
          ),
        const SizedBox(height: 16),
        // 保存按钮只负责本卡片的两个数字字段（maxMessages / maxTokens）；
        // 策略类型下拉是 onChanged 即时存。原 titlebar 的全局「保存」已删，
        // 按钮收进本卡片，语义与作用范围一致。
        Align(
          alignment: Alignment.centerRight,
          child: FilledButton.icon(
            icon: const Icon(Icons.check, size: 18),
            label: Text(context.l10n.save),
            onPressed: _saveSettings,
            style: FilledButton.styleFrom(
              backgroundColor: AppTheme.accentPrimary,
              foregroundColor: Colors.white,
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildTokenUsageContent() {
    if (_usageLoading) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 16),
        child: Center(
          child: SizedBox(
            width: 22,
            height: 22,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        ),
      );
    }
    // 读取失败时不能落进「暂无 AI 调用记录」——那会把故障说成从没调用过 AI。
    final usageError = _buildLoadErrorHint(_secTokenUsage);
    if (usageError != null) return usageError;
    if (_dailyUsage.isEmpty) {
      return Text(
        context.l10n.noAiUsage,
        style: TextStyle(
          color: AppTheme.textSecondary,
          fontSize: 13,
          height: 1.6,
        ),
      );
    }

    final today = _isoDate(DateTime.now());
    final totalTokens = _dailyUsage.fold<int>(0, (s, d) => s + d.totalTokens);
    final totalCalls = _dailyUsage.fold<int>(0, (s, d) => s + d.callCount);

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(
              child: _buildUsageMetric(
                label: context.l10n.last30DaysTokens,
                value: _fmtInt(totalTokens),
              ),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: _buildUsageMetric(
                label: context.l10n.totalCalls,
                value: context.l10n.callCount(totalCalls),
              ),
            ),
          ],
        ),
        const SizedBox(height: 20),
        _TokenUsageBarChart(
          // 图表按时间正序（左旧右新）；_dailyUsage 是倒序（最新在前）。
          days: _dailyUsage.take(30).toList().reversed.toList(),
          today: today,
        ),
      ],
    );
  }

  Widget _buildUsageMetric({required String label, required String value}) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(fontSize: 12, color: AppTheme.textSecondary),
        ),
        const SizedBox(height: 4),
        Text(
          value,
          style: TextStyle(
            fontSize: 22,
            fontWeight: FontWeight.w700,
            color: AppTheme.accentPrimary,
          ),
        ),
      ],
    );
  }

  String _fmtInt(int n) =>
      NumberFormat.decimalPattern(context.l10n.localeName).format(n);

  String _isoDate(DateTime dt) {
    final m = dt.month.toString().padLeft(2, '0');
    final d = dt.day.toString().padLeft(2, '0');
    return '${dt.year}-$m-$d';
  }

  Widget _buildTweetFetchSettings() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: context.l10n.l6FetchingServicebfb3a,
          value: _tweetService,
          items: const [
            DropdownMenuItem(value: 'fxtwitter', child: Text('fxTwitter')),
          ],
          onChanged: (value) async {
            if (value == null || value == _tweetService) return;
            setState(() => _tweetService = value);
            try {
              final repo =
                  ref.read(storageRepositoryProvider) as RustBridgeRepository;
              await repo.updateTweetFetchService(value);
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: Text(context.l10n.l6PostFetchingServiceUpdated1c8a1),
                  backgroundColor: AppTheme.accentPrimary,
                  behavior: SnackBarBehavior.floating,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.all(Radius.circular(999)),
                  ),
                ),
              );
            } catch (e) {
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: AppErrorMessage(
                    action: context.l10n.load,
                    error: _settingsFailure(
                      context.l10n.l6SavePostFetchingService23d23,
                      e,
                    ),
                  ),
                  backgroundColor: AppTheme.error,
                  behavior: SnackBarBehavior.floating,
                  shape: const RoundedRectangleBorder(
                    borderRadius: BorderRadius.all(Radius.circular(999)),
                  ),
                ),
              );
            }
          },
        ),
        // 这块没有 loading 态，字段值默认 'fxtwitter'：读取失败时若不提示，
        // 用户会以为库里就是这个值，而实际是读失败了。
        if (_buildLoadErrorHint(_secTweetService) case final tweetError?) ...[
          tweetError,
          const SizedBox(height: 12),
        ],
        const SizedBox(height: 12),
        Text(
          context.l10n.l6SupportedServiceFxtwitterConvertsXCom0989a,
          style: TextStyle(
            fontSize: 12,
            color: AppTheme.textSecondary.withValues(alpha: 0.7),
            height: 1.6,
          ),
        ),
      ],
    );
  }

  Widget _buildAppearanceSettings(AppSettings settings) {
    final notifier = ref.read(settingsProvider.notifier);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _buildDropdownField(
          label: context.l10n.language,
          value: settings.language ?? 'system',
          items: [
            DropdownMenuItem(
              value: 'zh-Hans',
              child: Text(context.l10n.chinese),
            ),
            DropdownMenuItem(value: 'en', child: Text(context.l10n.english)),
            DropdownMenuItem(
              value: 'system',
              child: Text(context.l10n.followSystem),
            ),
          ],
          onChanged: (value) async {
            try {
              await notifier.updateLanguage(value == 'system' ? null : value);
            } catch (e) {
              if (!mounted) return;
              ScaffoldMessenger.of(context).showSnackBar(
                SnackBar(
                  content: AppErrorMessage(
                    action: context.l10n.language,
                    error: e,
                  ),
                ),
              );
            }
          },
        ),
        const SizedBox(height: 16),
        _buildDropdownField(
          label: context.l10n.themeMode,
          value: settings.themeMode.name,
          items: AppThemeMode.values.map((mode) {
            return DropdownMenuItem(
              value: mode.name,
              child: Text(switch (mode) {
                AppThemeMode.light => context.l10n.lightTheme,
                AppThemeMode.dark => context.l10n.darkTheme,
                AppThemeMode.system => context.l10n.followSystem,
              }),
            );
          }).toList(),
          onChanged: (value) {
            if (value != null) {
              final mode = AppThemeMode.values.firstWhere(
                (m) => m.name == value,
              );
              notifier
                ..updateTheme(mode)
                ..saveTheme();
            }
          },
        ),
        const SizedBox(height: 16),
        _buildDropdownField(
          label: context.l10n.themeColors,
          value: settings.themePreset.name,
          items: AppThemePreset.values.map((preset) {
            return DropdownMenuItem(
              value: preset.name,
              child: Text(switch (preset) {
                AppThemePreset.amber => context.l10n.amberTheme,
                AppThemePreset.indigo => context.l10n.indigoTheme,
                AppThemePreset.aqua => context.l10n.aquaTheme,
                AppThemePreset.violet => context.l10n.violetTheme,
              }),
            );
          }).toList(),
          onChanged: (value) {
            if (value != null) {
              final preset = AppThemePreset.fromName(value);
              notifier
                ..updateThemePreset(preset)
                ..saveTheme();
            }
          },
        ),
        const SizedBox(height: 16),
        _buildFontField(settings, notifier),
        const SizedBox(height: 16),
        _buildFontSizeField(settings, notifier),
      ],
    );
  }

  /// 正文字号（仅知识库正文）：拖拽实时预览，松手才持久化
  Widget _buildFontSizeField(AppSettings settings, SettingsNotifier notifier) {
    final size = settings.fontSize;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text(
              context.l10n.bodyFontSize,
              style: TextStyle(
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: AppTheme.textSecondary,
              ),
            ),
            const Spacer(),
            Text(
              '${size.toStringAsFixed(size == size.roundToDouble() ? 0 : 1)} pt',
              style: TextStyle(
                fontSize: 13,
                color: AppTheme.textTertiary,
                fontFeatures: const [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
        const SizedBox(height: 4),
        Slider(
          value: size,
          min: AppFonts.minFontSize,
          max: AppFonts.maxFontSize,
          divisions: (AppFonts.maxFontSize - AppFonts.minFontSize).round(),
          label: size.toStringAsFixed(0),
          onChanged: (v) => notifier.updateFontSize(v),
          onChangeEnd: (v) => notifier
            ..updateFontSize(v)
            ..saveTheme(),
        ),
      ],
    );
  }

  /// 内容字体选择：主按钮打开自研选择框（系统默认 / 本地字体，
  /// 每行自身字体预览）；「系统默认」chip 单独提供。存值格式见 [parseStoredFont]。
  /// 界面文案字体固定，不在此设置（见 [AppFonts.uiFont]）。
  Widget _buildFontField(AppSettings settings, SettingsNotifier notifier) {
    final fontName = settings.fontName;
    final isSystem = fontName == AppFonts.system;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          context.l10n.contentFont,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          context.l10n.contentFontDescription,
          style: TextStyle(fontSize: 12, color: AppTheme.textTertiary),
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: OutlinedButton(
                onPressed: () => _showFontPicker(notifier, fontName),
                style: OutlinedButton.styleFrom(
                  alignment: Alignment.centerLeft,
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 14,
                  ),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: FontNameLabel(
                        stored: fontName,
                        fallbackStyle: null,
                      ),
                    ),
                    Icon(
                      Icons.font_download_outlined,
                      size: 18,
                      color: AppTheme.textSecondary,
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(width: 8),
            ChoiceChip(
              label: Text(context.l10n.systemDefault),
              selected: isSystem,
              onSelected: (selected) {
                if (selected) {
                  notifier
                    ..updateFontName(AppFonts.system)
                    ..saveTheme();
                }
              },
            ),
          ],
        ),
      ],
    );
  }

  /// 打开字体选择对话框：选中后先预热本地字体再落库，避免主题闪默认字体。
  Future<void> _showFontPicker(
    SettingsNotifier notifier,
    String current,
  ) async {
    final picked = await showFontPickerDialog(context, current: current);
    if (picked == null || !mounted) return;
    await notifier.setGlobalFont(picked);
    await notifier.saveTheme();
  }

  // static：只依赖 AppTheme 全局量，不碰任何 State 字段，
  // 因此 Provider 编辑对话框（独立 State）也能复用。
  static Widget _buildTextField({
    required String label,
    required TextEditingController controller,
    String? hint,
    bool obscure = false,
    ValueChanged<String>? onChanged,
    TextInputType? keyboardType,
    // 警告态（如「API Key 未配置」）：label 换警告色并挂提示徽标。
    bool warning = false,
    String? warningHint,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (warning)
          ProviderKeyWarningFieldLabel(label: label, warningHint: warningHint)
        else
          Text(
            label,
            style: TextStyle(
              fontSize: 14,
              fontWeight: FontWeight.w500,
              color: AppTheme.textSecondary,
            ),
          ),
        const SizedBox(height: 8),
        TextFormField(
          controller: controller,
          obscureText: obscure,
          onChanged: onChanged,
          keyboardType: keyboardType,
          style: TextStyle(color: AppTheme.textPrimary),
          decoration: InputDecoration(
            hintText: hint,
            hintStyle: TextStyle(
              color: AppTheme.textSecondary.withValues(alpha: 0.4),
            ),
            filled: true,
            fillColor: AppTheme.surface2.withValues(alpha: 0.5),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.2),
              ),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.2),
              ),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
            ),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 16,
              vertical: 14,
            ),
          ),
        ),
      ],
    );
  }

  static Widget _buildDropdownField<T>({
    required String label,
    required T value,
    required List<DropdownMenuItem<T>> items,
    required ValueChanged<T?> onChanged,
  }) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.textSecondary,
          ),
        ),
        const SizedBox(height: 8),
        DropdownButtonFormField<T>(
          initialValue: value,
          items: items,
          onChanged: onChanged,
          style: TextStyle(color: AppTheme.textPrimary),
          dropdownColor: AppTheme.surface2,
          decoration: InputDecoration(
            filled: true,
            fillColor: AppTheme.surface2.withValues(alpha: 0.5),
            border: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.2),
              ),
            ),
            enabledBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.2),
              ),
            ),
            focusedBorder: OutlineInputBorder(
              borderRadius: BorderRadius.circular(12),
              borderSide: BorderSide(color: AppTheme.accentPrimary, width: 2),
            ),
            contentPadding: const EdgeInsets.symmetric(
              horizontal: 16,
              vertical: 14,
            ),
          ),
        ),
      ],
    );
  }
}
