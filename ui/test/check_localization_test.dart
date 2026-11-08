import 'package:flutter_test/flutter_test.dart';

import '../tool/check_localization.dart';

void main() {
  const path = 'ui/lib/new_view.dart';
  test('AST inventories interpolation, adjacent and nested expressions', () {
    final rows = inventorySource(path, r"""
// ignored: 中文注释
const a = '新界面';
String c(bool flag) => '${flag ? "隐藏文案" : ""}' ' ASCII';
String b(bool flag, String value) => '前段' ' 中段 $value ${flag ? '内部' : value}';
""");
    expect(rows.map((r) => r['text']), [
      '新界面',
      '隐藏文案',
      '前段 中段 {arg0} {arg1}',
      '内部',
    ]);
    expect(
      () => inventorySource(path, "const a = 'bad"),
      throwsFormatException,
    );
  });
  test(
    'new files and occurrences fail; exceptions are exact and cannot go stale',
    () {
      final rows = inventorySource(path, "const a = '机器标记';");
      final exception = <String, dynamic>{
        'path': path,
        'text': '机器标记',
        'count': 1,
        'category': 'protocol',
        'reason': 'Persisted protocol fixture',
      };
      expect(checkOwnership(rows, []), isNotEmpty);
      expect(checkOwnership(rows, [exception]), isEmpty);
      expect(checkOwnership([...rows, ...rows], [exception]), isNotEmpty);
      expect(checkOwnership([], [exception]), isNotEmpty);
      expect(checkOwnership(rows, [exception, exception]), isNotEmpty);
      expect(
        checkOwnership(rows, [
          {...exception, 'category': 'UI'},
        ]),
        isNotEmpty,
      );
      expect(
        checkOwnership(rows, [
          {...exception, 'reason': ''},
        ]),
        isNotEmpty,
      );
      expect(
        checkOwnership(rows, [
          {...exception, 'path': 'ui/lib/other.dart'},
        ]),
        isNotEmpty,
      );
    },
  );
}
