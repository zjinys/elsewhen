part of "../settings_screen.dart";

// ignore_for_file: invalid_use_of_protected_member — setState 从 State 子类方法平移至 extension，与 tabs_model.dart 同款纪律

/// 「安全」tab：可选的打开密码管理。设置后 App 启动需先解锁；
/// 不设置则完全无感。哈希记录由 AppLockService 落在系统安全存储。
extension _SecurityTab on _SettingsScreenState {
  Widget _buildSecurityTab() {
    return SingleChildScrollView(
      padding: const EdgeInsets.all(AppTheme.space6),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _buildSection(
            title: "打开密码",
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
                _appLockEnabled ? "已开启：启动 App 时需要输入密码" : "未开启：启动 App 直接进入主界面",
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
            label: const Text("设置打开密码"),
          )
        else
          Wrap(
            spacing: AppTheme.space2,
            runSpacing: AppTheme.space2,
            children: [
              OutlinedButton.icon(
                onPressed: _showChangePasswordDialog,
                icon: const Icon(Icons.edit_outlined, size: 17),
                label: const Text("修改密码"),
              ),
              OutlinedButton.icon(
                onPressed: _showDisablePasswordDialog,
                icon: Icon(Icons.lock_open, size: 17, color: AppTheme.error),
                label: Text("关闭密码", style: TextStyle(color: AppTheme.error)),
              ),
            ],
          ),
        if (_appLockEnabled) ...[
          const SizedBox(height: AppTheme.space4),
          _buildBiometricTile(),
        ],
        const SizedBox(height: AppTheme.space3),
        Text(
          "密码只以加盐哈希形式保存在本机系统安全存储，不会上传、不会随同步带走。"
          "遗忘密码后无法找回，只能清除应用数据重置。",
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
                "指纹/面容解锁",
                style: TextStyle(
                  fontSize: 13,
                  color: usable ? AppTheme.textPrimary : AppTheme.textTertiary,
                ),
              ),
              Text(
                usable ? "开启后可用系统生物验证快速解锁，密码始终保留为兜底" : "当前设备不支持或未录入指纹/面容",
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
        title: const Text("开启指纹/面容解锁"),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text("开启后锁屏页会优先调起系统指纹/面容验证。请输入当前打开密码确认。"),
            const SizedBox(height: 12),
            TextField(
              controller: controller,
              obscureText: true,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: "当前密码",
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text("取消"),
          ),
          FilledButton(
            onPressed: () async {
              final ok = await _appLockService.verifyPassword(controller.text);
              if (ctx.mounted) Navigator.of(ctx).pop(ok);
            },
            child: const Text("确认开启"),
          ),
        ],
      ),
    );
    if (verified != true) {
      if (verified == false && mounted && controller.text.isNotEmpty) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text("当前密码不正确，未开启")));
      }
      return;
    }
    final ok = await _appLockService.setBiometricUnlock(true);
    await _loadAppLockState();
    if (mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text(ok ? "指纹/面容解锁已开启" : "设备当前不可用生物验证，未开启")),
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
        title: "设置打开密码",
        requireCurrent: false,
        onSubmit: (current, next) => _appLockService.setPassword(next),
      ),
    );
    if (result == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text("打开密码已设置")));
      }
    }
  }

  Future<void> _showChangePasswordDialog() async {
    final result = await showDialog<bool>(
      context: context,
      builder: (ctx) => _PasswordDialog(
        title: "修改打开密码",
        requireCurrent: true,
        onSubmit: (current, next) =>
            _appLockService.setPassword(next, currentPassword: current),
      ),
    );
    if (result == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text("打开密码已修改")));
      }
    }
  }

  Future<void> _showDisablePasswordDialog() async {
    final controller = TextEditingController();
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text("关闭打开密码"),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text("关闭后启动 App 将不再要求输入密码。请输入当前密码确认。"),
            const SizedBox(height: 12),
            TextField(
              controller: controller,
              obscureText: true,
              autofocus: true,
              decoration: const InputDecoration(
                labelText: "当前密码",
                border: OutlineInputBorder(),
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(ctx).pop(false),
            child: const Text("取消"),
          ),
          FilledButton(
            onPressed: () async {
              final ok = await _appLockService.disable(controller.text);
              if (ctx.mounted) Navigator.of(ctx).pop(ok);
            },
            child: const Text("确认关闭"),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await _loadAppLockState();
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(const SnackBar(content: Text("打开密码已关闭")));
      }
    } else if (confirmed == false && mounted && controller.text.isNotEmpty) {
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text("当前密码不正确，未关闭")));
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
      setState(() => _error = "请输入当前密码");
      return;
    }
    if (next.isEmpty) {
      setState(() => _error = "新密码不能为空");
      return;
    }
    if (next.length != 4) {
      setState(() => _error = "打开密码固定为 4 位");
      return;
    }
    if (next != confirm) {
      setState(() => _error = "两次输入的新密码不一致");
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
        _error = widget.requireCurrent ? "当前密码不正确" : "设置失败，请重试";
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
              decoration: const InputDecoration(
                labelText: "当前密码",
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
            decoration: const InputDecoration(
              labelText: "新密码（4 位）",
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
            decoration: const InputDecoration(
              labelText: "再次输入新密码",
              border: OutlineInputBorder(),
            ),
            onSubmitted: (_) => _submit(),
          ),
          if (_error != null) ...[
            const SizedBox(height: 10),
            Text(
              _error!,
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
          child: const Text("取消"),
        ),
        FilledButton(
          onPressed: _submitting ? null : _submit,
          child: _submitting
              ? const SizedBox(
                  width: 16,
                  height: 16,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Text("确定"),
        ),
      ],
    );
  }
}
