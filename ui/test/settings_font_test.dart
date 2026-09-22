import 'package:elsewhen_ui/providers/settings_provider.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('systemFontFamiliesProvider 枚举系统字体：非空、去重、无转义残留', () async {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    final fonts = await container.read(systemFontFamiliesProvider.future);

    // 注意：flutter test 运行器会用内置 fonts.conf 隔离 fontconfig，
    // 测试环境只能枚举到 Flutter 自带字体（Roboto 等 3 个）；
    // 真实 App 进程中枚举的是完整系统字体（本机实测 345 个）。
    expect(fonts, isNotEmpty, reason: '至少应有 Flutter 测试运行器自带的字体');
    expect(
      fonts.any((f) => f.contains(r'\')),
      isFalse,
      reason: 'fc-list 的反斜杠转义（如 FZSongS\\-Extended）应被反转义',
    );
    expect(
      fonts.toSet().length,
      fonts.length,
      reason: '列表应已去重',
    );
  });
}
