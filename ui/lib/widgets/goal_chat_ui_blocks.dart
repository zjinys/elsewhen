import 'dart:convert';

import 'package:ai_ui_render/ai_ui_render.dart';
import 'package:flutter/material.dart';

import '../theme/app_theme.dart';

//! 目标域 AI 生成 UI（elsewhen-ui 围栏块 → 原生组件渲染）
//!
//! 纪律与 elsewhen-choices 同源：AI 只能在回复里输出受目录约束的
//! JSON UI 树（guardrail），端上白名单渲染；解析失败一律回退纯文本，
//! 绝不把原始 JSON 摆给用户。写操作不在这里——组件动作只有
//! send_message（把文本作为用户消息发出）与 fill_draft（预填输入框），
//! 目标变更仍走 WriteConfirm 提议卡。

/// 目标域组件目录 + 注册表（单例：目录固定，构建一次复用）。
class GoalUiCatalog {
  GoalUiCatalog._();

  static final ComponentRegistry registry = _buildRegistry();

  /// 供 Rust 侧幕僚提示词 / 调试用：目录中声明的动作名。
  static const actions = ['send_message', 'fill_draft'];

  static ComponentRegistry _buildRegistry() {
    final r = ComponentRegistry(
      fallback: (ctx) => _GoalText(
        ctx.element.props['text']?.toString() ?? '',
        tone: 'secondary',
      ),
    );
    r.register('GoalCard', _goalCard);
    r.register('GoalText', (ctx) {
      return _GoalText(
        ctx.propOr<String>('text', ''),
        size: (ctx.prop<num>('size') ?? 12.5).toDouble(),
        bold: ctx.prop<bool>('bold') ?? false,
        tone: ctx.propOr<String>('tone', 'primary'),
      );
    });
    r.register('GoalHeading', (ctx) {
      return Padding(
        padding: const EdgeInsets.only(bottom: 4),
        child: Row(
          children: [
            Icon(
              Icons.flag_outlined,
              size: 13,
              color: AppTheme.accentPrimary,
            ),
            const SizedBox(width: 5),
            Expanded(
              child: Text(
                ctx.propOr<String>('text', ''),
                style: TextStyle(
                  fontSize: 12.5,
                  fontWeight: FontWeight.w700,
                  color: AppTheme.textPrimary,
                ),
              ),
            ),
          ],
        ),
      );
    });
    r.register('GoalChip', (ctx) => _GoalChip(ctx.propOr<String>('label', ''),
        tone: ctx.propOr<String>('tone', 'neutral')));
    r.register('GoalMetric', (ctx) => _GoalMetric(
        label: ctx.propOr<String>('label', ''),
        value: ctx.propOr<String>('value', '')));
    r.register('GoalChoices', (ctx) {
      final raw = ctx.element.props['options'];
      final opts = <({String label, String message})>[];
      if (raw is List) {
        for (final item in raw) {
          if (item is Map) {
            final label = item['label']?.toString().trim() ?? '';
            if (label.isEmpty) continue;
            final message =
                (item['message']?.toString().trim().isNotEmpty ?? false)
                    ? item['message'].toString().trim()
                    : label;
            opts.add((label: label, message: message));
          } else if (item is String && item.trim().isNotEmpty) {
            opts.add((label: item.trim(), message: item.trim()));
          }
        }
      }
      return _GoalChoices(
        options: opts,
        enabled: !(ctx.loading),
        onSend: (message) => ctx.onAction?.call('send_message', {
          'message': message,
        }),
      );
    });
    r.register('GoalDivider', (_) => const Padding(
          padding: EdgeInsets.symmetric(vertical: 6),
          child: Divider(height: 1),
        ));
    r.register('Column', (ctx) => Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          spacing: (ctx.prop<num>('spacing') ?? 6).toDouble(),
          children: ctx.children,
        ));
    r.register('Row', (ctx) => Wrap(
          spacing: (ctx.prop<num>('spacing') ?? 8).toDouble(),
          runSpacing: 6,
          children: ctx.children,
        ));
    return r;
  }

  static Widget _goalCard(ComponentBuildContext ctx) {
    final title = ctx.propOr<String>('title', '');
    return Container(
      width: double.infinity,
      margin: const EdgeInsets.only(bottom: 6),
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: AppTheme.surface2,
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
        border: Border.all(color: AppTheme.surface3.withValues(alpha: 0.6)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        spacing: 6,
        children: [
          if (title.isNotEmpty)
            Text(
              title,
              style: TextStyle(
                fontSize: 12,
                fontWeight: FontWeight.w700,
                color: AppTheme.accentPrimary,
              ),
            ),
          ...ctx.children,
        ],
      ),
    );
  }
}

/// 语义色（guardrail：AI 只能选语义名，不能给任意色值）。
Color _toneColor(String tone) => switch (tone) {
      'accent' => AppTheme.accentPrimary,
      'success' => AppTheme.success,
      'warning' => AppTheme.warning,
      'error' => AppTheme.error,
      'secondary' => AppTheme.textSecondary,
      'tertiary' => AppTheme.textTertiary,
      _ => AppTheme.textPrimary,
    };

class _GoalText extends StatelessWidget {
  const _GoalText(this.text, {this.size = 12.5, this.bold = false, required this.tone});

  final String text;
  final double size;
  final bool bold;
  final String tone;

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: TextStyle(
        fontSize: size,
        height: 1.55,
        fontWeight: bold ? FontWeight.w600 : FontWeight.w400,
        color: _toneColor(tone),
      ),
    );
  }
}

class _GoalChip extends StatelessWidget {
  const _GoalChip(this.label, {required this.tone});

  final String label;
  final String tone;

  @override
  Widget build(BuildContext context) {
    final color = _toneColor(tone);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 2),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(AppTheme.radiusFull),
      ),
      child: Text(
        label,
        style: TextStyle(
          fontSize: 10.5,
          fontWeight: FontWeight.w600,
          color: color,
        ),
      ),
    );
  }
}

class _GoalMetric extends StatelessWidget {
  const _GoalMetric({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
      decoration: BoxDecoration(
        color: AppTheme.surface3.withValues(alpha: 0.45),
        borderRadius: BorderRadius.circular(AppTheme.radiusSmall),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            value,
            style: TextStyle(
              fontSize: 13.5,
              fontWeight: FontWeight.w700,
              color: AppTheme.textPrimary,
            ),
          ),
          const SizedBox(height: 1),
          Text(
            label,
            style: TextStyle(fontSize: 10.5, color: AppTheme.textTertiary),
          ),
        ],
      ),
    );
  }
}

class _GoalChoices extends StatelessWidget {
  const _GoalChoices({
    required this.options,
    required this.enabled,
    required this.onSend,
  });

  final List<({String label, String message})> options;
  final bool enabled;
  final ValueChanged<String> onSend;

  @override
  Widget build(BuildContext context) {
    if (options.isEmpty) return const SizedBox.shrink();
    return Padding(
      padding: const EdgeInsets.only(top: 2),
      child: Wrap(
        spacing: 6,
        runSpacing: 6,
        children: [
          for (final opt in options)
            ActionChip(
              label: Text(opt.label, style: const TextStyle(fontSize: 12)),
              labelPadding: const EdgeInsets.symmetric(horizontal: 8),
              visualDensity: VisualDensity.compact,
              backgroundColor: AppTheme.accentPrimary.withValues(alpha: 0.08),
              side: BorderSide(
                color: AppTheme.accentPrimary.withValues(alpha: 0.35),
              ),
              onPressed: enabled ? () => onSend(opt.message) : null,
            ),
        ],
      ),
    );
  }
}

/// 解析 elsewhen-ui 围栏块正文为 [UITree]；失败返回 null（调用方回退纯文本）。
UITree? parseGoalUiTree(String raw) {
  try {
    final decoded = jsonDecode(raw);
    if (decoded is! Map<String, dynamic>) return null;
    final tree = UITree.fromJson(decoded);
    if (tree.root.isEmpty || !tree.elements.containsKey(tree.root)) {
      return null;
    }
    return tree;
  } catch (_) {
    return null;
  }
}

/// 一条 assistant 消息里的 elsewhen-ui 块：剥块正文 + 解析出的 UI 树。
class GoalUiBlockData {
  final String strippedContent;
  final UITree tree;
  const GoalUiBlockData({required this.strippedContent, required this.tree});
}

/// 提取消息中的 ```elsewhen-ui 围栏块；无块或 JSON 非法返回 null。
GoalUiBlockData? extractGoalUiBlock(String content) {
  final pattern = RegExp(
    r'```elsewhen-ui\s*\n([\s\S]*?)\n?```',
    caseSensitive: false,
  );
  final match = pattern.firstMatch(content);
  if (match == null) return null;
  final raw = match.group(1) ?? '';
  final tree = parseGoalUiTree(raw);
  if (tree == null) return null;
  final stripped = content.replaceRange(match.start, match.end, '').trim();
  return GoalUiBlockData(strippedContent: stripped, tree: tree);
}

/// 渲染一条 elsewhen-ui 块（已解析的 UITree）。
///
/// [onAction] 由面板接入：send_message → 发送用户消息，
/// fill_draft → 预填输入框。未识别动作静默忽略（guardrail 兑底）。
class GoalUiBlock extends StatelessWidget {
  const GoalUiBlock({super.key, required this.tree, this.onAction});

  final UITree tree;
  final void Function(String actionName, Map<String, dynamic>? params)?
      onAction;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: AppTheme.space3, left: 34),
      child: StandaloneJsonRenderer(
        tree: tree,
        registry: GoalUiCatalog.registry,
        onAction: onAction,
        fallback: (ctx) => const SizedBox.shrink(),
      ),
    );
  }
}
