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

  group('fontAwareTextStyleConfiguration：正文颜色挂钩主题', () {
    test('注入主题色到基础样式，保留字号/字体族', () {
      const color = Color(0xFF123456);
      final cfg = fontAwareTextStyleConfiguration('Inter', color: color);

      expect(cfg.text.color, color);
      expect(cfg.bold.color, color);
      expect(cfg.italic.color, color);
      expect(cfg.underline.color, color);
      expect(cfg.strikethrough.color, color);
      expect(cfg.text.fontSize, 16, reason: '字号保持编辑器默认不变');
      expect(cfg.text.fontFamily, 'Inter');
      expect(cfg.bold.fontWeight, FontWeight.bold, reason: '字重保持默认');
    });

    test('vendor 自带语义色不被主题色覆盖（href/code/autoComplete）', () {
      const color = Color(0xFF111111);
      final cfg = fontAwareTextStyleConfiguration('Inter', color: color);

      expect(cfg.href.color, Colors.lightBlue);
      expect(cfg.code.color, Colors.red);
      expect(cfg.autoComplete.color, Colors.grey);
    });

    test('仅 color 时：基础样式着色、字体族保持默认', () {
      const color = Color(0xFF111111);
      final cfg = fontAwareTextStyleConfiguration(null, color: color);

      expect(cfg.text.color, color);
      expect(cfg.text.fontFamily, isNull);
      expect(cfg.text.fontSize, 16);
    });

    test('family 与 color 都为空时返回 vendor 默认（零影响）', () {
      final none = fontAwareTextStyleConfiguration(null);
      expect(none.text, const TextStyleConfiguration().text);
      expect(none.text.color, isNull);
    });
  });

  group('fontAwareTextStyleConfiguration：正文字号挂钩设置', () {
    test('注入字号到基础 text；组合样式无字号留空由 combine 继承', () {
      final cfg = fontAwareTextStyleConfiguration(
        'Inter',
        color: const Color(0xFF111111),
        fontSize: 18,
      );

      expect(cfg.text.fontSize, 18, reason: '基础样式携带设置字号');
      // 组合样式自身不带 fontSize（null）——渲染时 AppFlowy combine
      // 会把 text 的基础字号带到 bold/italic/... 上
      expect(cfg.bold.fontSize, isNull);
      expect(cfg.italic.fontSize, isNull);
    });

    test('未显式传字号保持 vendor 默认；0/负数不注入', () {
      expect(fontAwareTextStyleConfiguration('Inter').text.fontSize, 16);
      expect(
        fontAwareTextStyleConfiguration('Inter', fontSize: 0).text.fontSize,
        16,
      );
      expect(
        fontAwareTextStyleConfiguration('Inter', fontSize: -2).text.fontSize,
        16,
      );
    });

    test('仅 fontSize 时：只落字号，字体族/颜色保持默认', () {
      final cfg = fontAwareTextStyleConfiguration(null, fontSize: 20);
      expect(cfg.text.fontSize, 20);
      expect(cfg.text.fontFamily, isNull);
      expect(cfg.text.color, isNull);
      expect(cfg.href.color, Colors.lightBlue, reason: '语义色不受影响');
    });
  });
}