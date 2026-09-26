import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../providers/app_provider.dart';
import '../screens/settings_screen.dart';
import '../theme/app_theme.dart';

/// 首次运行引导横幅：未配置任何 AI Provider 时提示并引导去设置页。
///
/// 行为约定：
/// - 探测进行中或查询失败时不显示（fail-safe，配置页里另有静默空态兜底）；
/// - 已配置（探测为 true）或用户手动关闭（[IconButton]）后隐藏；
/// - 「去配置」从设置页返回后 invalidate 探测源，配置完立即消失。
class AiProviderSetupHint extends ConsumerStatefulWidget {
  const AiProviderSetupHint({super.key});

  @override
  ConsumerState<AiProviderSetupHint> createState() =>
      _AiProviderSetupHintState();
}

class _AiProviderSetupHintState extends ConsumerState<AiProviderSetupHint> {
  bool _dismissed = false;

  @override
  Widget build(BuildContext context) {
    final configured = ref.watch(aiProviderConfiguredProvider);
    // 未配置才提示：探测中 / 失败 / 已配置 / 已手动关闭 → 不占空间
    if (_dismissed || !configured.hasValue || configured.value == true) {
      return const SizedBox.shrink();
    }

    Future<void> openSettings() async {
      await Navigator.of(context).push(
        MaterialPageRoute(builder: (_) => const SettingsScreen()),
      );
      // 从设置页返回后重新探测：配好则横幅消失，没配则继续提示
      if (!mounted) return;
      ref.invalidate(aiProviderConfiguredProvider);
    }

    return Container(
      width: double.infinity,
      margin: const EdgeInsets.fromLTRB(
        AppTheme.space4,
        AppTheme.space3,
        AppTheme.space4,
        0,
      ),
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space4,
        vertical: AppTheme.space2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusMedium),
        border: Border.all(color: AppTheme.surface3),
      ),
      child: Row(
        children: [
          Icon(
            Icons.rocket_launch_outlined,
            size: 18,
            color: AppTheme.accentPrimary,
          ),
          const SizedBox(width: AppTheme.space3),
          Expanded(
            child: Text(
              '首次使用提示：尚未配置 AI Provider，AI 对话与自动分析暂不可用。',
              style: TextStyle(
                color: AppTheme.textSecondary,
                fontSize: 12.5,
              ),
            ),
          ),
          TextButton(
            onPressed: openSettings,
            style: TextButton.styleFrom(
              foregroundColor: AppTheme.accentPrimary,
              padding: const EdgeInsets.symmetric(
                horizontal: AppTheme.space3,
              ),
              minimumSize: const Size(0, 32),
            ),
            child: const Text('去配置'),
          ),
          IconButton(
            icon: const Icon(Icons.close, size: 16),
            color: AppTheme.textTertiary,
            tooltip: '关闭',
            padding: EdgeInsets.zero,
            constraints: const BoxConstraints(minWidth: 28, minHeight: 28),
            onPressed: () => setState(() => _dismissed = true),
          ),
        ],
      ),
    );
  }
}