part of "../settings_screen.dart";

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，与 tabs_model.dart 同款纪律

/// 「安全」tab：本机密钥与可选的打开密码管理。设置密码后 App 启动需先解锁；
/// 不设置则完全无感。哈希记录由 AppLockService 落在系统安全存储。
extension _SecurityTab on _SettingsScreenState {
  Widget _buildSecurityTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: context.l10n.secretManagement,
            icon: Icons.key_outlined,
            child: const NamedSecretSettings(),
          ),
          const SizedBox(height: AppTheme.space6),
          _buildSection(
            title: context.l10n.appPassword,
            icon: Icons.lock_outline,
            child: _appLockLoading
                ? const Padding(
                    padding: EdgeInsets.symmetric(vertical: 16),
                    child: Center(
                      child: SizedBox(
                        width: 18,
                        height: 18,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                    ),
                  )
                : _buildAppLockContent(),
          ),
        ],
      ),
    );
  }

  Widget _buildAppLockContent() {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Icon(
              _appLockEnabled ? Icons.lock : Icons.lock_open,
              size: 18,
              color: _appLockEnabled
                  ? AppTheme.accentPrimary
                  : AppTheme.textTertiary,
            ),
            const SizedBox(width: AppTheme.space2),
            Expanded(
              child: Text(
                _appLockEnabled
                    ? context.l10n.l6EnabledAPasswordIsRequiredWhen1aee3
                    : context.l10n.l6DisabledTheAppOpensDirectlycbfe4,
                style: TextStyle(fontSize: 13, color: AppTheme.textSecondary),
              ),
            ),
          ],
        ),
        const SizedBox(height: AppTheme.space4),
        if (!_appLockEnabled)
          FilledButton.icon(
            onPressed: _showSetPasswordDialog,
            icon: const Icon(Icons.add, size: 18),
            label: Text(context.l10n.l6SetAnAppPassword72f45),
          )
        else
          Wrap(
            spacing: AppTheme.space2,
            runSpacing: AppTheme.space2,
            children: [
              OutlinedButton.icon(
                onPressed: _showChangePasswordDialog,
                icon: const Icon(Icons.edit_outlined, size: 17),
                label: Text(context.l10n.l6ChangePassword0ae20),
              ),
              OutlinedButton.icon(
                onPressed: _showDisablePasswordDialog,
                icon: Icon(Icons.lock_open, size: 17, color: AppTheme.error),
                label: Text(
                  context.l10n.l6TurnOffPassword7f7cb,
                  style: TextStyle(color: AppTheme.error),
                ),
              ),
            ],
          ),
        if (_appLockEnabled) ...[
          const SizedBox(height: AppTheme.space4),
          _buildBiometricTile(),
        ],
        const SizedBox(height: AppTheme.space3),
        Text(
          context.l10n.l6OnlyASaltedPasswordHashIsd5644,
          style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
        ),
      ],
    );
  }

  /// 指纹/面容解锁开关行。设备不支持或未录入生物特征时置灰并说明；
  /// 桌面端（biometricAvailable 恒 false）自然落到置灰分支，不展示可用假象。
  Widget _buildBiometricTile() {
    final usable = _biometricAvailable;
    return Row(
      children: [
        Icon(
          Icons.fingerprint,
          size: 20,
          color: usable ? AppTheme.textSecondary : AppTheme.textTertiary,
        ),
        const SizedBox(width: AppTheme.space2),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                context.l10n.l6FingerprintOrFaceUnlock1b96c,
                style: TextStyle(
                  fontSize: 13,
                  color: usable ? AppTheme.textPrimary : AppTheme.textTertiary,
                ),
              ),
              Text(
                usable
                    ? context.l10n.l6UseSystemBiometricsForQuickUnlocking22994
                    : context.l10n.l6ThisDeviceDoesNotSupportBiometrics40ff0,
                style: TextStyle(fontSize: 11, color: AppTheme.textTertiary),
              ),
            ],
          ),
        ),
        Switch(
          value: _biometricEnabled,
          onChanged: usable ? _toggleBiometric : null,
        ),
      ],
    );
  }

  /// 切换生物识别开关。开启前需先验证当前密码（防止他人拿到已解锁设备
  /// 悄悄录自己的指纹通道）；关闭无需再验（设备生物特征变更后系统侧也会
  /// 自动失效，不构成降权）。
  Future<void> _toggleBiometric(bool target) async {
    if (!target) {
      final ok = await _appLockService.setBiometricUnlock(false);
      if (ok) await _loadAppLockState();
      return;
    }
    final controller = TextEditingController();
    final verified = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(context.l10n.l6EnableFingerprintOrFaceUnlockb2a8f),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.l10n.l6TheLockScreenWillFirstOffer49b2b),
            const SizedBox(height: 12),
            TextField(
              controller: controller,
              obscureText: true,
              autofocus: true,
              decoration: InputDecoration(
                labelText: context.l10n.l6CurrentPassword33abf,
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () async {
              final ok = await _appLockService.verifyPassword(controller.text);
              if (ctx.mounted) Navigator.of(ctx).pop(ok);
            },
            child: Text(context.l10n.l6ConfirmEnable263d4),
          ),
        ],
      ),
    );
    if (verified != true) {
      if (verified == false && mounted && controller.text.isNotEmpty) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text(
              context.l10n.l6IncorrectCurrentPasswordNotEnabled691f1,
            ),
          ),
        );
      }
      return;
    }
    final ok = await _appLockService.setBiometricUnlock(true);
    await _loadAppLockState();
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            ok
                ? context.l10n.l6BiometricUnlockEnablede15eb
                : context
                      .l10n
                      .l6BiometricsAreCurrentlyUnavailableNotEnabled78c56,
          ),
        ),
      );
    }
  }

  Future<void> _loadAppLockState() async {
    try {
      final enabled = await _appLockService.isEnabled();
      final bioAvailable =
          enabled && await _appLockService.biometricAvailable();
      final bioEnabled =
          bioAvailable && await _appLockService.biometricUnlockEnabled();
      if (mounted) {
        setState(() {
          _appLockEnabled = enabled;
          _biometricAvailable = bioAvailable;
          _biometricEnabled = bioEnabled;
          _appLockLoading = false;
        });
      }
    } catch (_) {
      // 安全存储不可用时按未开启展示，保证设置页可用；
      // 真正设置/修改时再在操作处报错。
      if (mounted) setState(() => _appLockLoading = false);
    }
  }

  Future<void> _showSetPasswordDialog() async {
    final result = await showDialog<bool>(
      context: context,
      builder: (ctx) => _PasswordDialog(
        title: context.l10n.l6SetAnAppPassword72f45,
        requireCurrent: false,
        onSubmit: (current, next) => _appLockService.setPassword(next),
      ),
    );
    if (result == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.l6AppPasswordSeta5390)),
        );
      }
    }
  }

  Future<void> _showChangePasswordDialog() async {
    final result = await showDialog<bool>(
      context: context,
      builder: (ctx) => _PasswordDialog(
        title: context.l10n.l6ChangeAppPassword25e9a,
        requireCurrent: true,
        onSubmit: (current, next) =>
            _appLockService.setPassword(next, currentPassword: current),
      ),
    );
    if (result == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.l6AppPasswordChanged0493a)),
        );
      }
    }
  }

  Future<void> _showDisablePasswordDialog() async {
    final controller = TextEditingController();
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(context.l10n.l6TurnOffAppPassworda80d7),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.l10n.l6TheAppWillNoLongerRequire00cc0),
            const SizedBox(height: 12),
            TextField(
              controller: controller,
              obscureText: true,
              autofocus: true,
              decoration: InputDecoration(
                labelText: context.l10n.l6CurrentPassword33abf,
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: Text(context.l10n.cancel),
          ),
          FilledButton(
            onPressed: () async {
              final ok = await _appLockService.disable(controller.text);
              if (ctx.mounted) Navigator.of(ctx).pop(ok);
            },
            child: Text(context.l10n.l6ConfirmDisable059b4),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(context.l10n.l6AppPasswordTurnedOff9f4c1)),
        );
      }
    } else if (confirmed == false && mounted && controller.text.isNotEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            context.l10n.l6IncorrectCurrentPasswordNotDisabled0660c,
          ),
        ),
      );
    }
  }
}

/// 设置/修改密码共用的对话框：当前密码（修改时）+ 新密码两次确认。
class _PasswordDialog extends StatefulWidget {
  const _PasswordDialog({
    required this.title,
    required this.requireCurrent,
    required this.onSubmit,
  });

  final String title;
  final bool requireCurrent;
  final Future<bool> Function(String? current, String next) onSubmit;

  @override
  State<_PasswordDialog> createState() => _PasswordDialogState();
}

class _PasswordDialogState extends State<_PasswordDialog> {
  final _currentController = TextEditingController();
  final _nextController = TextEditingController();
  final _confirmController = TextEditingController();
  String? _error;
  bool _submitting = false;

  @override
  void dispose() {
    _currentController.dispose();
    _nextController.dispose();
    _confirmController.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final current = widget.requireCurrent ? _currentController.text : null;
    final next = _nextController.text;
    final confirm = _confirmController.text;
    if (widget.requireCurrent && (current == null || current.isEmpty)) {
      setState(() => _error = context.l10n.l6EnterTheCurrentPassword4b0b0);
      return;
    }
    if (next.isEmpty) {
      setState(() => _error = context.l10n.l6EnterANewPassword689e5);
      return;
    }
    if (next.length != 4) {
      setState(() => _error = context.l10n.l6TheAppPasswordMustHave41cb25);
      return;
    }
    if (next != confirm) {
      setState(() => _error = context.l10n.l6TheNewPasswordsDoNotMatchd0b9c);
      return;
    }
    setState(() {
      _error = null;
      _submitting = true;
    });
    final ok = await widget.onSubmit(current, next);
    if (!mounted) return;
    if (ok) {
      Navigator.of(context).pop(true);
    } else {
      setState(() {
        _submitting = false;
        _error = widget.requireCurrent
            ? context.l10n.l6TheCurrentPasswordIsIncorrectb0dab
            : context.l10n.l6CouldNotSaveTryAgainae0b2;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(widget.title),
      content: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (widget.requireCurrent) ...[
            TextField(
              controller: _currentController,
              obscureText: true,
              autofocus: true,
              decoration: InputDecoration(
                labelText: context.l10n.l6CurrentPassword33abf,
                border: OutlineInputBorder(),
              ),
            ),
            const SizedBox(height: 12),
          ],
          TextField(
            controller: _nextController,
            obscureText: true,
            autofocus: !widget.requireCurrent,
            keyboardType: TextInputType.number,
            inputFormatters: [
              FilteringTextInputFormatter.digitsOnly,
              LengthLimitingTextInputFormatter(4),
            ],
            decoration: InputDecoration(
              labelText: context.l10n.l6NewPassword4Digits51018,
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 12),
          TextField(
            controller: _confirmController,
            obscureText: true,
            keyboardType: TextInputType.number,
            inputFormatters: [
              FilteringTextInputFormatter.digitsOnly,
              LengthLimitingTextInputFormatter(4),
            ],
            decoration: InputDecoration(
              labelText: context.l10n.l6ConfirmNewPassworde0327,
              border: OutlineInputBorder(),
            ),
            onSubmitted: (_) => _submit(),
          ),
          if (_error != null) ...[
            const SizedBox(height: 10),
            AppErrorMessage(
              action: context.l10n.load,
              error: _error!,
              style: TextStyle(color: AppTheme.error, fontSize: 12),
            ),
          ],
        ],
      ),
      actions: [
        TextButton(
          onPressed: _submitting
              ? null
              : () => Navigator.of(context).pop(false),
          child: Text(context.l10n.cancel),
        ),
        FilledButton(
          onPressed: _submitting ? null : _submit,
          child: _submitting
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : Text(context.l10n.l6Confirmf526c),
        ),
      ],
    );
  }
}
