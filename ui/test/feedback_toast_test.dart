import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/feedback_toast.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';

import 'support/localizations.dart';

const _surface = ValueKey('feedback-toast');

Future<void> _pump(
  WidgetTester tester, {
  double width = 1200,
  Brightness brightness = Brightness.light,
  EdgeInsets safe = EdgeInsets.zero,
}) async {
  tester.view.physicalSize = Size(width, 800);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  if (Platform.environment['FEEDBACK_PREVIEW'] == '1') {
    debugDisableShadows = false;
    addTearDown(() => debugDisableShadows = true);
    await tester.runAsync(() async {
      final bytes = await File(
        '/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc',
      ).readAsBytes();
      for (final family in ['Noto Sans SC', 'Ahem', 'Inter', 'Roboto']) {
        await (FontLoader(
          family,
        )..addFont(Future.value(ByteData.sublistView(bytes)))).load();
      }
      final icons = await File(
        '/home/pp/fvm/versions/3.50.0/bin/cache/artifacts/material_fonts/MaterialIcons-Regular.otf',
      ).readAsBytes();
      await (FontLoader(
        'MaterialIcons',
      )..addFont(Future.value(ByteData.sublistView(icons)))).load();
    });
  }
  await tester.pumpWidget(
    MaterialApp(
      theme: AppTheme.buildTheme(AppThemePreset.amber, brightness),
      locale: const Locale('zh'),
      supportedLocales: const [Locale('zh'), Locale('en')],
      localizationsDelegates: appTestDelegates(),
      builder: (context, child) => RepaintBoundary(
        key: const ValueKey('feedback-preview'),
        child: MediaQuery(
          data: MediaQuery.of(context).copyWith(padding: safe),
          child: child!,
        ),
      ),
      home: Scaffold(
        body: Builder(
          builder: (context) => Column(
            children: [
              TextButton(
                onPressed: () => showFeedbackToast(
                  context,
                  message: '已记录，并生成一条事件',
                  kind: FeedbackKind.success,
                ),
                child: const Text('record'),
              ),
              const SizedBox(height: 220),
              TextButton(
                onPressed: () => showFeedbackToast(context, message: '另一次操作'),
                child: const Text('next'),
              ),
              TextButton(
                onPressed: () => showDialog<void>(
                  context: context,
                  builder: (context) => AlertDialog(
                    content: TextButton(
                      onPressed: () =>
                          showFeedbackToast(context, message: '弹窗中的反馈'),
                      child: const Text('notify'),
                    ),
                  ),
                ),
                child: const Text('dialog'),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

void main() {
  for (final brightness in Brightness.values) {
    for (final width in [1200.0, 360.0]) {
      testWidgets('feedback stays compact at top right: $width $brightness', (
        tester,
      ) async {
        await _pump(
          tester,
          width: width,
          brightness: brightness,
          safe: const EdgeInsets.only(top: 24),
        );
        await tester.tap(find.text('record'));
        await tester.pumpAndSettle();
        final rect = tester.getRect(find.byKey(_surface));
        expect(rect.top, 24);
        expect(rect.right, width - 12);
        expect(rect.width, lessThanOrEqualTo(400));
        expect(rect.height, lessThan(140));
        expect(find.byType(SnackBar), findsNothing);
        expect(find.text('已记录，并生成一条事件'), findsOneWidget);
        expect(tester.takeException(), isNull);
        if (Platform.environment['FEEDBACK_PREVIEW'] == '1') {
          final boundary = tester.renderObject<RenderRepaintBoundary>(
            find.byKey(const ValueKey('feedback-preview')),
          );
          await tester.runAsync(() async {
            final image = await boundary.toImage();
            final bytes = await image.toByteData(
              format: ui.ImageByteFormat.png,
            );
            await File(
              '/tmp/elsewhen-feedback-${width.toInt()}-${brightness.name}.png',
            ).writeAsBytes(bytes!.buffer.asUint8List());
            image.dispose();
          });
        }
        // A second action still receives clicks outside the toast.
        await tester.tap(find.text('next'));
        await tester.pumpAndSettle();
        expect(find.byKey(_surface), findsOneWidget);
        expect(find.text('另一次操作'), findsOneWidget);
        expect(find.text('已记录，并生成一条事件'), findsNothing);
        await tester.pump(const Duration(seconds: 4));
        expect(find.byKey(_surface), findsNothing);
      });
    }
  }

  testWidgets(
    'feedback overlays a modal and its action survives until selected',
    (tester) async {
      await _pump(tester);
      await tester.tap(find.text('dialog'));
      await tester.pumpAndSettle();
      final context = tester.element(find.text('notify'));
      var actions = 0;
      final closed = FeedbackToast.of(context).show(
        SnackBar(
          content: const Text('需要查看详情'),
          duration: const Duration(seconds: 1),
          action: SnackBarAction(label: '查看', onPressed: () => actions++),
        ),
      );
      await tester.pumpAndSettle();
      await tester.pump(const Duration(seconds: 5));
      expect(find.byKey(_surface), findsOneWidget);
      await tester.tap(find.text('查看'));
      await tester.pumpAndSettle();
      await closed;
      expect(actions, 1);
      expect(find.byKey(_surface), findsNothing);
      expect(find.byType(AlertDialog), findsOneWidget);
      await tester.tap(find.text('notify'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('关闭'));
      await tester.pumpAndSettle();
      expect(find.byKey(_surface), findsNothing);
    },
  );

  testWidgets(
    'long feedback is bounded and remains readable at large text sizes',
    (tester) async {
      await _pump(tester, width: 360);
      final context = tester.element(find.text('record'));
      final handle = FeedbackToast.of(context);
      handle.show(
        SnackBar(
          content: MediaQuery(
            data: MediaQuery.of(context)
                .copyWith(textScaler: TextScaler.linear(2)),
            child: Text(List.filled(40, '保存失败，请检查后重试。').join()),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final rect = tester.getRect(find.byKey(_surface));
      expect(rect.height, lessThanOrEqualTo(240));
      expect(rect.right, 348);
      expect(tester.takeException(), isNull);
      await tester.drag(
        find.byType(SingleChildScrollView),
        const Offset(0, -200),
      );
      await tester.pump();
      handle.dismiss();
      await tester.pumpAndSettle();
    },
  );
}
