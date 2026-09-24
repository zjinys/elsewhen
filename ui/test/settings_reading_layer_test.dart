import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/models/settings.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter_test/flutter_test.dart';

/// 知识页阅读参数「两层覆盖模型」回归：
/// `实际值 = 编辑器覆盖(可空) ?? 全局值`，见
/// docs/notes/proposed/product/2026-09-23-editor-reading-settings-layer.md。
void main() {
  group('AppSettings 两层覆盖求值', () {
    test('contentFontSize：编辑器覆盖 ?? 全局字号', () {
      final base = AppSettings.defaults().copyWith(fontSize: 18);
      expect(base.contentFontSize, 18, reason: '无覆盖 → 回落到全局字号');
      final overridden = base.copyWithEditorSettings(fontSizeOverride: 21);
      expect(overridden.contentFontSize, 21, reason: '覆盖优先于全局');
      expect(overridden.fontSize, 18, reason: '全局字号不受覆盖影响');
    });

    test('contentLineHeight：编辑器覆盖 ?? 全局默认 1.5', () {
      final base = AppSettings.defaults();
      expect(base.contentLineHeight, 1.5, reason: '尚无全局行距，回落 vendor 默认');
      final overridden = base.copyWithEditorSettings(lineHeightOverride: 2.0);
      expect(overridden.contentLineHeight, 2.0);
    });

    test('clearing（传 null）= 跟随全局：清空覆盖、回落全局', () {
      final overridden = AppSettings.defaults().copyWithEditorSettings(
        font: 'Noto Serif SC',
        fontSizeOverride: 20,
        lineHeightOverride: 2.0,
      );
      expect(overridden.hasEditorOverrides, isTrue);
      final cleared = overridden.copyWithEditorSettings(
        font: null,
        fontSizeOverride: null,
        lineHeightOverride: null,
      );
      expect(cleared.editorFontName, isNull);
      expect(cleared.editorFontSize, isNull);
      expect(cleared.editorLineHeight, isNull);
      expect(cleared.hasEditorOverrides, isFalse);
      expect(cleared.contentFontSize, cleared.fontSize);
    });

    test('越界覆盖值在写入时被钳制到合法区间', () {
      final s = AppSettings.defaults().copyWithEditorSettings(
        fontSizeOverride: 99.0,
        lineHeightOverride: 9.9,
      );
      expect(s.editorFontSize, AppFonts.maxFontSize);
      expect(s.editorLineHeight, AppFonts.maxLineHeight);
      expect(s.contentFontSize, AppFonts.maxFontSize);
      expect(s.contentLineHeight, AppFonts.maxLineHeight);
    });

    test('copyWith 原子性：改全局不冲掉编辑器覆盖', () {
      final s = AppSettings.defaults().copyWithEditorSettings(fontSizeOverride: 20);
      final changed = s.copyWith(fontSize: 12);
      expect(changed.editorFontSize, 20, reason: '编辑器覆盖独立保留');
      expect(changed.contentFontSize, 20, reason: '渲染取值仍以覆盖为准');
      expect(changed.fontSize, 12);
    });
  });

  group('fontAwareTextStyleConfiguration 行距注入', () {
    test('传入 lineHeight 2.0 → 配置顶层行距 2.0', () {
      final cfg = fontAwareTextStyleConfiguration(null, lineHeight: 2.0);
      expect(cfg.lineHeight, 2.0);
    });

    test('不传 lineHeight → 回落 vendor 默认 1.5', () {
      final cfg = fontAwareTextStyleConfiguration(null);
      expect(cfg.lineHeight, 1.5);
    });
  });
}