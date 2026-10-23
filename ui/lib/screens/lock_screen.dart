import "package:flutter/material.dart";
import "package:flutter/services.dart";

import "../theme/app_theme.dart";
import "../utils/app_lock.dart";

/// 启动锁屏：已设置打开密码时，App 初始化完成后先停在本页，
/// 输入正确密码回调 [onUnlocked] 才进入主界面。无返回键、无跳过入口；
/// 忘记密码只能清除应用数据（哈希记录随安全存储一并清掉）。
///
/// 密码固定 4 位：4 个格子输满即自动校验，没有「解锁」按钮。旧版本设置过
/// 非 4 位密码时通过页面底部的小入口退回自由输入（按回车解锁）。
class LockScreen extends StatefulWidget {
  const LockScreen({super.key, required this.onUnlocked, this.service});

  final VoidCallback onUnlocked;

  /// 测试注入口：绕开真实安全存储（默认用系统安全存储）。
  @visibleForTesting
  final AppLockService? service;

  @override
  State<LockScreen> createState() => _LockScreenState();
}

class _LockScreenState extends State<LockScreen> {
  static const _pinLength = 4;

  final _controller = TextEditingController();
  late final AppLockService _service;
  final _pinFocus = FocusNode();
  final _fallbackFocus = FocusNode();
  String? _error;
  bool _verifying = false;
  bool _biometricReady = false;
  bool _biometricBusy = false;

  /// 非 4 位旧密码的自由输入兜底（正常流程不可见）。
  bool _fallbackEntry = false;

  @override
  void initState() {
    super.initState();
    _service = widget.service ?? AppLockService();
    _initBiometric();
    _controller.addListener(_onPinChanged);
  }

  /// 进入锁屏时：若用户已开启指纹/面容且设备可用，自动弹一次系统验证。
  Future<void> _initBiometric() async {
    try {
      final enabled = await _service.biometricUnlockEnabled();
      final available = enabled && await _service.biometricAvailable();
      if (!mounted) return;
      setState(() => _biometricReady = available);
      if (available) _tryBiometric();
    } catch (_) {
      // 检测失败按无生物识别处理，密码输入不受影响。
    }
  }

  Future<void> _tryBiometric() async {
    if (_biometricBusy || _verifying) return;
    setState(() => _biometricBusy = true);
    try {
      final ok = await _service.authenticateWithBiometrics();
      if (!mounted) return;
      if (ok) {
        widget.onUnlocked();
        return;
      }
    } finally {
      if (mounted) setState(() => _biometricBusy = false);
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    _pinFocus.dispose();
    _fallbackFocus.dispose();
    super.dispose();
  }

  /// 输满 4 位立即校验：这是唯一的「提交」方式，没有按钮。
  void _onPinChanged() {
    if (_verifying || _controller.text.length < _pinLength) return;
    _submit();
  }

  Future<void> _submit() async {
    if (_verifying) return;
    final input = _controller.text;
    if (input.isEmpty) return;
    setState(() {
      _verifying = true;
      _error = null;
    });
    try {
      final ok = await _service.verifyPassword(input);
      if (!mounted) return;
      if (ok) {
        widget.onUnlocked();
      } else {
        setState(() {
          _verifying = false;
          _error = "密码不正确";
        });
        _controller.clear();
        if (_fallbackEntry && mounted) {
          _fallbackFocus.requestFocus();
        } else {
          _pinFocus.requestFocus();
        }
      }
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _verifying = false;
        _error = "安全存储暂不可用，请稍后重试";
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.surface0,
      // LayoutBuilder + SingleChildScrollView：与启动页同款纪律，无边框/
      // 透明窗口首帧可能给出极小高度约束，Center+Column 会溢出。
      body: LayoutBuilder(
        builder: (context, constraints) => SingleChildScrollView(
          child: ConstrainedBox(
            constraints: BoxConstraints(
              minHeight: constraints.maxHeight,
              minWidth: constraints.maxWidth,
            ),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(
                  Icons.lock_outline,
                  size: 40,
                  color: AppTheme.textTertiary,
                ),
                const SizedBox(height: 16),
                Text(
                  "Elsewhen 已锁定",
                  style: TextStyle(
                    color: AppTheme.textPrimary,
                    fontSize: 18,
                    letterSpacing: 1.2,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                const SizedBox(height: 24),
                if (!_fallbackEntry) ...[
                  _PinBoxes(
                    length: _pinLength,
                    controller: _controller,
                    focusNode: _pinFocus,
                    verifying: _verifying,
                    hasError: _error != null,
                  ),
                  const SizedBox(height: 10),
                  // 校验中给一个安静的进度点；其余反馈只有错误文案。
                  SizedBox(
                    height: 16,
                    child: _verifying
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : null,
                  ),
                  if (_error != null)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 6),
                      child: Text(
                        _error!,
                        style: TextStyle(color: AppTheme.error, fontSize: 12),
                      ),
                    ),
                ] else ...[
                  SizedBox(
                    width: 260,
                    child: TextField(
                      controller: _controller,
                      obscureText: true,
                      autofocus: true,
                      focusNode: _fallbackFocus,
                      enabled: !_verifying,
                      textAlign: TextAlign.center,
                      onSubmitted: (_) => _submit(),
                      decoration: InputDecoration(
                        hintText: "输入打开密码，输完按回车",
                        border: const OutlineInputBorder(),
                        errorText: _verifying ? null : _error,
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  if (_verifying)
                    const SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    ),
                ],
                if (_biometricReady) ...[
                  const SizedBox(height: 12),
                  SizedBox(
                    width: 260,
                    child: OutlinedButton.icon(
                      onPressed: _biometricBusy ? null : _tryBiometric,
                      icon: const Icon(Icons.fingerprint, size: 20),
                      label: Text(_biometricBusy ? "验证中…" : "使用指纹/面容解锁"),
                    ),
                  ),
                ],
                // 兜底入口放最小视觉权重：4 位密码是主流程。
                TextButton(
                  onPressed: _verifying
                      ? null
                      : () {
                          _controller.clear();
                          _error = null;
                          setState(() {
                            _fallbackEntry = !_fallbackEntry;
                          });
                        },
                  style: TextButton.styleFrom(
                    foregroundColor: AppTheme.textTertiary,
                    textStyle: const TextStyle(fontSize: 12),
                  ),
                  child: Text(_fallbackEntry ? "改用 4 格输入" : "旧密码不是 4 位？换输入方式"),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 4 个密码格：真实输入发生在一个透明 TextField 上（保证键盘/自动聚焦/
/// 长按粘贴等系统能力齐全），格子里只画圆点与高亮态。
/// 本身是监听者：controller 与焦点挂在 build 状态上，任一变化即重画。
class _PinBoxes extends StatefulWidget {
  const _PinBoxes({
    required this.length,
    required this.controller,
    required this.focusNode,
    required this.verifying,
    required this.hasError,
  });

  final int length;
  final TextEditingController controller;
  final FocusNode focusNode;
  final bool verifying;
  final bool hasError;

  @override
  State<_PinBoxes> createState() => _PinBoxesState();
}

class _PinBoxesState extends State<_PinBoxes> {
  void _refresh() {
    if (mounted) setState(() {});
  }

  @override
  void initState() {
    super.initState();
    widget.controller.addListener(_refresh);
    widget.focusNode.addListener(_refresh);
  }

  @override
  void didUpdateWidget(covariant _PinBoxes old) {
    super.didUpdateWidget(old);
    if (old.controller != widget.controller) {
      old.controller.removeListener(_refresh);
      widget.controller.addListener(_refresh);
    }
    if (old.focusNode != widget.focusNode) {
      old.focusNode.removeListener(_refresh);
      widget.focusNode.addListener(_refresh);
    }
  }

  @override
  void dispose() {
    widget.controller.removeListener(_refresh);
    widget.focusNode.removeListener(_refresh);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: 232,
      height: 56,
      child: Stack(
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              for (var i = 0; i < widget.length; i++)
                _PinBox(
                  filled: i < widget.controller.text.length,
                  active:
                      !widget.verifying &&
                      !widget.hasError &&
                      i == widget.controller.text.length &&
                      widget.focusNode.hasFocus,
                  hasError: widget.hasError,
                ),
            ],
          ),
          // 透明输入层盖在格子上：点击任何格子都会把焦点给它。必须关掉
          // 全局主题的 filled 填充（否则会画一块不透明矩形盖住格子），
          // 文字也透明——可见状态只由下方的格子层表达。
          Positioned.fill(
            child: TextField(
              controller: widget.controller,
              focusNode: widget.focusNode,
              autofocus: true,
              enabled: !widget.verifying,
              obscureText: true,
              showCursor: false,
              // 长度限定用 formatter，不用 maxLength：后者会带出「0/4」计数器。
              keyboardType: TextInputType.number,
              inputFormatters: [
                FilteringTextInputFormatter.digitsOnly,
                LengthLimitingTextInputFormatter(widget.length),
              ],
              style: const TextStyle(
                color: Colors.transparent,
                fontSize: 16,
                height: 1.0,
              ),
              decoration: const InputDecoration(
                filled: false,
                border: InputBorder.none,
                enabledBorder: InputBorder.none,
                focusedBorder: InputBorder.none,
                contentPadding: EdgeInsets.zero,
                isDense: true,
                isCollapsed: true,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _PinBox extends StatelessWidget {
  const _PinBox({
    required this.filled,
    required this.active,
    required this.hasError,
  });

  final bool filled;
  final bool active;
  final bool hasError;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final border = Border.all(
      width: active || hasError ? 1.6 : 1,
      color: hasError
          ? AppTheme.error
          : active
          ? AppTheme.accentPrimary
          : theme.colorScheme.outlineVariant,
    );
    return AnimatedContainer(
      duration: const Duration(milliseconds: 120),
      width: 52,
      height: 56,
      decoration: BoxDecoration(
        color: filled ? AppTheme.surface2 : Colors.transparent,
        border: border,
        borderRadius: BorderRadius.circular(12),
      ),
      alignment: Alignment.center,
      child: filled
          ? Text(
              "●",
              style: TextStyle(
                color: AppTheme.textPrimary,
                fontSize: 14,
                height: 1.0,
              ),
            )
          : null,
    );
  }
}
