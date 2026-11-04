//! 面板弹层的统一入口：一律 bottom sheet。
//!
//! 背景：「今天」行右侧各按钮（目标/待办/创建主题/审阅提议）此前各自
//! 决定弹 dialog 还是 bottom sheet，交互方式不统一。现统一为一律
//! bottom sheet：单手可达、内容下滑可扩到大屏，且与移动端 Material
//! 惯例一致。此文件提供唯一的面板呈现入口与面板壳。

import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../theme/app_theme.dart';

/// 移动端（Android / iOS，非 Web）判定；消息气泡长按菜单等仍在用。
bool get isMobilePanel => !kIsWeb && (Platform.isAndroid || Platform.isIOS);

/// 面板呈现唯一入口：一律以 bottom sheet 弹出（全平台）。
///
/// [sheet] 返回的应是 [AdaptivePanelSheet]（自带把手/标题/关闭行）；
/// 内容 widget 由调用方复用同一份，不再区分桌面/移动两种骨架。
Future<T?> showPanelSheet<T>(
  BuildContext context, {
  required WidgetBuilder sheet,
}) {
  return showModalBottomSheet<T>(
    context: context,
    isScrollControlled: true,
    backgroundColor: AppTheme.surface0,
    shape: const RoundedRectangleBorder(
      borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
    ),
    builder: sheet,
  );
}

/// 通用底部面板壳：拖动把手 + 标题/关闭行 + 内容区（限高）。
/// 待办/目标/创建主题/审阅提议等 bottom sheet 复用。
class AdaptivePanelSheet extends StatelessWidget {
  final String title;
  final WidgetBuilder builder;

  /// 内容区最大高度占屏幕比例；长表单（如创建主题编辑器）可调大。
  final double maxHeightFactor;

  const AdaptivePanelSheet({
    super.key,
    required this.title,
    required this.builder,
    this.maxHeightFactor = 0.7,
  });

  @override
  Widget build(BuildContext context) {
    return SafeArea(
      top: false,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const _SheetHandle(),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 0, 8, 4),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    title,
                    style: TextStyle(
                      fontSize: 16,
                      fontWeight: FontWeight.w600,
                      color: AppTheme.textPrimary,
                    ),
                  ),
                ),
                IconButton(
                  tooltip: '关闭',
                  icon: const Icon(Icons.close, size: 20),
                  color: AppTheme.textSecondary,
                  onPressed: () => Navigator.of(context).pop(),
                ),
              ],
            ),
          ),
          Flexible(
            child: ConstrainedBox(
              constraints: BoxConstraints(
                maxHeight: MediaQuery.sizeOf(context).height * maxHeightFactor,
              ),
              child: builder(context),
            ),
          ),
        ],
      ),
    );
  }
}

class _SheetHandle extends StatelessWidget {
  const _SheetHandle();

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(top: 10, bottom: 10),
      child: Container(
        width: 36,
        height: 4,
        decoration: BoxDecoration(
          color: AppTheme.surface3,
          borderRadius: BorderRadius.circular(2),
        ),
      ),
    );
  }
}
