import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import '../theme/app_theme.dart';

/// 注入 AppFlowyEditor 的 text span decorator（§5.2）：把行内 `wikilink`
/// 属性渲染成与 `MarkdownView` 一致的视觉（accentPrimary + 下划线 + w600），
/// 点击回调 [onTapWikiLink]，参数为 wikilink 目标 slug。
///
/// 非 wikilink 属性一律委托内置 [defaultTextSpanDecoratorForAttribute]，
/// 保留 href 点击、选区定位等原生行为。
TextSpanDecoratorForAttribute wikiTextSpanDecorator({
  required void Function(String slug) onTapWikiLink,
}) {
  return (context, node, index, text, before, after) {
    final attributes = text.attributes;
    if (attributes != null) {
      final target = attributes[BuiltInAttributeKey.wikilink] as String?;
      if (target != null && target.isNotEmpty) {
        return TextSpan(
          style: (before.style ?? const TextStyle()).copyWith(
            color: AppTheme.accentPrimary,
            decoration: TextDecoration.underline,
            decorationColor: AppTheme.accentMuted,
            fontWeight: FontWeight.w600,
          ),
          text: text.text,
          recognizer: TapGestureRecognizer()..onTap = () => onTapWikiLink(target),
          mouseCursor: SystemMouseCursors.click,
        );
      }
    }
    return defaultTextSpanDecoratorForAttribute(
      context,
      node,
      index,
      text,
      before,
      after,
    );
  };
}