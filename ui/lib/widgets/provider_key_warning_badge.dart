import 'package:flutter/material.dart';

import '../theme/app_theme.dart';

/// API Key 未配置时卡片上的警告徽标（roadmap C12 后续：密文被清/密钥丢失
/// 后配置还在，但 AI 调用必失败 —— 卡片必须先于失败把这件事亮出来）。
///
/// 判定来自 Rust 侧 `hasApiKey`（明文或可解密封存任一存在即 true，
/// 锁定态只查存在性不解密）；本组件只负责视觉，不做业务判定。

/// 卡片边框色：未配置 key 用警告色，否则维持原有激活/常态配色。
/// 纯函数，供卡片与测试共用（测试注入 false 模拟密文丢失形态）。
Color providerCardBorderColor({
  required bool hasApiKey,
  required bool isActive,
}) {
  if (!hasApiKey) return AppTheme.warning.withValues(alpha: 0.55);
  return isActive
      ? AppTheme.accentPrimary.withValues(alpha: 0.5)
      : AppTheme.surface3;
}

/// 表单字段（如编辑对话框里的 API Key）的警告标签行：
/// label 换警告色 + 钥匙图标 + 提示文字（默认「未配置」）。
/// 与卡片徽标共用同一套警告语义，测试可公开驱动。
class ProviderKeyWarningFieldLabel extends StatelessWidget {
  const ProviderKeyWarningFieldLabel({
    super.key,
    required this.label,
    this.warningHint = '未配置',
  });

  final String label;
  final String warningHint;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Text(
          label,
          style: TextStyle(
            fontSize: 14,
            fontWeight: FontWeight.w500,
            color: AppTheme.warning,
          ),
        ),
        const SizedBox(width: 6),
        Icon(Icons.key_off_rounded, size: 14, color: AppTheme.warning),
        const SizedBox(width: 2),
        Text(
          warningHint,
          style: TextStyle(fontSize: 11, color: AppTheme.warning),
        ),
      ],
    );
  }
}

/// Provider 卡片右上区域的操作按钮组（编辑/复制/删除 + 激活区）。
/// 抽成独立组件让卡片「警告分支」可用注入的 DTO 在测试里整卡驱动，
/// 不必经私有的设置页 extension。
class ProviderCardActions extends StatelessWidget {
  const ProviderCardActions({
    super.key,
    required this.isActive,
    required this.onActivate,
    required this.onEdit,
    required this.onDuplicate,
    required this.onDelete,
  });

  final bool isActive;
  final VoidCallback onActivate;
  final VoidCallback onEdit;
  final VoidCallback onDuplicate;
  final VoidCallback onDelete;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: isActive
              // 激活项不再显示「设为激活」：按钮点了没意义，占位还让卡片左右不对称。
              ? Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    '对话使用此配置',
                    style: TextStyle(color: AppTheme.textTertiary, fontSize: 11),
                  ),
                )
              : Align(
                  alignment: Alignment.centerLeft,
                  child: TextButton(
                    onPressed: onActivate,
                    style: TextButton.styleFrom(
                      padding: const EdgeInsets.symmetric(horizontal: 8),
                      minimumSize: const Size(0, 28),
                      tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                    ),
                    child: const Text('设为激活'),
                  ),
                ),
        ),
        IconButton(
          tooltip: '编辑',
          icon: const Icon(Icons.edit_outlined, size: 18),
          color: AppTheme.textSecondary,
          visualDensity: VisualDensity.compact,
          onPressed: onEdit,
        ),
        IconButton(
          tooltip: '复制配置（API Key 需重新输入）',
          icon: const Icon(Icons.copy_rounded, size: 18),
          color: AppTheme.textSecondary,
          visualDensity: VisualDensity.compact,
          onPressed: onDuplicate,
        ),
        IconButton(
          tooltip: '删除',
          icon: const Icon(Icons.delete_outline, size: 18),
          color: AppTheme.error,
          visualDensity: VisualDensity.compact,
          onPressed: onDelete,
        ),
      ],
    );
  }
}
class ProviderKeyWarningBadge extends StatelessWidget {
  const ProviderKeyWarningBadge({super.key});

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: AppTheme.space2,
        vertical: 2,
      ),
      decoration: BoxDecoration(
        color: AppTheme.warning.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.key_off_rounded, size: 11, color: AppTheme.warning),
          const SizedBox(width: 3),
          Text(
            '未配置 Key',
            style: TextStyle(
              color: AppTheme.warning,
              fontSize: 11,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}
