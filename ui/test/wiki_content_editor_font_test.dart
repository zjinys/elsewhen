import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('fontAwareTextStyleConfiguration：编辑器正文跟随全局字体', () {
    test('注入字体族到各款默认样式，保留编辑器原有字号', () {
      final cfg = fontAwareTextStyleConfiguration('Noto Sans SC');

      expect(cfg.text.fontFamily, 'Noto Sans SC');
      expect(cfg.bold.fontFamily, 'Noto Sans SC');
      expect(cfg.italic.fontFamily, 'Noto Sans SC');
      expect(cfg.underline.fontFamily, 'Noto Sans SC');
      expect(cfg.href.fontFamily, 'Noto Sans SC');
      expect(cfg.code.fontFamily, 'Noto Sans SC');
      expect(cfg.autoComplete.fontFamily, 'Noto Sans SC');
      expect(cfg.text.fontSize, 16, reason: '字号保持编辑器默认不变');
      expect(cfg.bold.fontWeight, FontWeight.bold, reason: '字重保持默认');
    });

    test('fontFamily 为空时返回 vendor 默认配置（跟随系统）', () {
      final none = fontAwareTextStyleConfiguration(null);
      expect(none.text.fontFamily, isNull);
      expect(none.text.fontSize, 16);
      // 与 vendor 默认等价
      expect(none.text, const TextStyleConfiguration().text);
    });

    test('字体族相同的样式不打重复字体族（copyWith 不改变其它属性）', () {
      final withFamily = fontAwareTextStyleConfiguration('Inter');
      // copyWith 即使值相同也产生新对象，但语义应等价于直接复制
      expect(withFamily.text.fontFamily, 'Inter');
      expect(withFamily.text.fontSize, 16);
      expect(withFamily.bold.fontWeight, FontWeight.bold);
    });
  });
}