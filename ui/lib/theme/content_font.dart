import 'package:flutter/material.dart';

/// 内容字体作用域。
///
/// 字体分两类：界面文案（按钮、菜单、侧栏、设置项、提示）固定走主题字体
/// （[AppTheme.buildTheme]），用户内容（知识库正文、对话消息、事件原文、
/// 网页正文预览）跟随「外观 → 内容字体」。本 widget 挂在 MaterialApp 之上
/// （对话框路由也能取到），携带已解析的内容字体族；内容区用
/// [ContentFontScope] 或 [ContentFont.styleOf] 取用。
class ContentFont extends InheritedWidget {
  const ContentFont({super.key, required this.family, required super.child});

  /// 已解析的内容字体族（[resolveFontFamily] 的结果）；null = 跟随系统字体。
  final String? family;

  /// 内容字体样式（仅字体族 + 回退链，可与任意 TextStyle merge）。
  /// 树上没有 [ContentFont]（如测试 harness）时返回 null，调用方保持原样式。
  static TextStyle? styleOf(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<ContentFont>();
    if (scope == null) return null;
    final theme = Theme.of(context);
    // 跟随系统：界面 textTheme 已被固定字体覆盖，回到平台 typography 的字体族
    // （Linux Roboto→Ubuntu/Cantarell 回退，Windows Segoe UI 等）。
    final family =
        scope.family ?? theme.typography.black.bodyMedium?.fontFamily;
    return TextStyle(
      fontFamily: family,
      fontFamilyFallback: theme.textTheme.bodyMedium?.fontFamilyFallback,
    );
  }

  @override
  bool updateShouldNotify(ContentFont oldWidget) => family != oldWidget.family;
}

/// 把子树的默认文本字体换成内容字体。只影响经 [DefaultTextStyle] 继承字体的
/// Text / SelectableText；直接用 RichText 的地方需自行 merge [ContentFont.styleOf]。
class ContentFontScope extends StatelessWidget {
  const ContentFontScope({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final style = ContentFont.styleOf(context);
    if (style == null) return child;
    return DefaultTextStyle.merge(style: style, child: child);
  }
}
