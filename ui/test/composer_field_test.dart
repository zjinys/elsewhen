import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/models/settings.dart' show AppThemePreset;
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/composer_field.dart';

/// 输入条（composer）的几何契约：主对话、知识页对话、目标讨论三处共用
/// 同一个组件，这里钉住「胶囊只有一个轮廓、两端留白对称、发送键在胶囊
/// 内部」这些容易被后续改动破坏的性质。
void main() {
  Widget wrap(Widget child) => MaterialApp(
    theme: AppTheme.buildTheme(AppThemePreset.amber, Brightness.dark),
    home: Scaffold(body: Padding(padding: const EdgeInsets.all(16), child: child)),
  );

  Future<void> pumpComposer(
    WidgetTester tester, {
    TextEditingController? controller,
    bool canSend = true,
    bool spinning = false,
    String text = '',
  }) async {
    final c = controller ?? TextEditingController(text: text);
    addTearDown(c.dispose);
    await tester.pumpWidget(
      wrap(
        ComposerField(
          controller: c,
          focusNode: FocusNode(),
          hintText: '输入消息',
          onSubmit: () {},
          canSend: canSend,
          spinning: spinning,
          leading: IconButton(
            tooltip: '添加内容',
            icon: const Icon(Icons.add_circle_outline, size: 20),
            onPressed: () {},
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Finder capsule() => find.byKey(const ValueKey('composer-surface'));

  testWidgets('胶囊两端留白对称，发送键在胶囊内部而不是外面', (tester) async {
    await pumpComposer(tester, text: '你好');

    final send = tester.getRect(find.byType(ComposerSendButton));
    final plus = tester.getRect(find.byTooltip('添加内容'));
    final shell = tester.getRect(capsule());

    // 发送键整体落在胶囊外框之内 —— 旧实现里它是 Row 的第二个兄弟，
    // 挂在胶囊外面、被容器背景隔开。
    expect(send.right, lessThan(shell.right));
    expect(plus.left, greaterThan(shell.left));

    // 左「＋」与右发送键到各自胶囊边的距离对称（±1px）。
    expect(
      ((plus.center.dx - shell.left) - (shell.right - send.center.dx)).abs(),
      lessThanOrEqualTo(1.0),
    );
  });

  testWidgets('单行时发送键与输入框等高同心，胶囊高度只比按钮多一圈描边', (tester) async {
    await pumpComposer(tester, text: '你好');
    final send = tester.getRect(find.byType(ComposerSendButton));
    final field = tester.getRect(find.byType(TextField));
    final shell = tester.getRect(capsule());
    expect(field.height, send.height);
    expect(field.center.dy, closeTo(send.center.dy, 0.5));
    // 48 的内容高度 + 上下各 1 的描边 = 50 的胶囊。
    expect(shell.height, 50);
  });

  testWidgets('多行时发送键贴底，不飘到胶囊顶部', (tester) async {
    await pumpComposer(
      tester,
      text: List.filled(4, '一行内容').join('\n'),
    );
    final send = tester.getRect(find.byType(ComposerSendButton));
    final field = tester.getRect(find.byType(TextField));
    final shell = tester.getRect(capsule());
    expect(field.height, greaterThan(80));
    // 底对齐：发送键下缘离胶囊底部只有一层 padding + 描边。
    expect((shell.bottom - send.bottom).abs(), lessThanOrEqualTo(7.0));
    // 旧实现的 suffixIcon 垂直居中，四行时发送键会浮在胶囊中部偏上。
    expect(send.center.dy, greaterThan(shell.center.dy));
  });

  testWidgets('空文本时发送键只降亮度、仍可点（回车/点击由调用方拦空）', (tester) async {
    var submits = 0;
    final c = TextEditingController();
    addTearDown(c.dispose);
    await tester.pumpWidget(
      wrap(
        ComposerField(
          controller: c,
          focusNode: FocusNode(),
          hintText: '输入消息',
          onSubmit: () => submits++,
          sendTooltip: '发送',
        ),
      ),
    );
    await tester.pumpAndSettle();

    final button = tester.widget<ComposerSendButton>(
      find.byType(ComposerSendButton),
    );
    expect(button.onPressed, isNotNull, reason: '空文本时按钮不该被禁用');
    expect(button.dimmed, isTrue, reason: '空文本时按钮应弱化显示');

    await tester.tap(find.byType(ComposerSendButton));
    await tester.pump();
    expect(submits, 1);
  });

  testWidgets('不可发送时按钮禁用；转圈态不显示箭头', (tester) async {
    // 转圈态用 pump 而不是 pumpAndSettle：菊花是无限动画，settle 不会停。
    await tester.pumpWidget(
      wrap(
        ComposerField(
          controller: TextEditingController(text: '排队中的问题'),
          focusNode: FocusNode(),
          hintText: '输入消息',
          onSubmit: () {},
          canSend: false,
          spinning: true,
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 200));
    final button = tester.widget<ComposerSendButton>(
      find.byType(ComposerSendButton),
    );
    expect(button.onPressed, isNull);
    expect(find.byIcon(Icons.arrow_upward), findsNothing);
    expect(find.byType(CircularProgressIndicator), findsOneWidget);
  });

  testWidgets('聚焦时描边跟随强调色，且只有一层描边', (tester) async {
    await pumpComposer(tester);
    BoxDecoration decoration() {
      final container = tester.widget<AnimatedContainer>(capsule());
      return container.decoration! as BoxDecoration;
    }

    expect((decoration().border! as Border).top.color, AppTheme.surface3);
    await tester.tap(find.byType(TextField));
    await tester.pumpAndSettle();
    expect((decoration().border! as Border).top.color, AppTheme.accentPrimary);
    // 输入框自身不再有边框：只有胶囊一层。
    final input = tester.widget<TextField>(find.byType(TextField));
    expect(input.decoration!.border, InputBorder.none);
    expect(input.decoration!.filled, isNot(true));
  });
}