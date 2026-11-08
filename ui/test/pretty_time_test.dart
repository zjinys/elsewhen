import 'package:elsewhen_ui/utils/pretty_time.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/widgets.dart';
import 'package:elsewhen_ui/l10n/app_localizations.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:intl/intl.dart';

/// prettyTime：审阅列表等元数据用的相对时间打印。
void main() {
  final now = DateTime(2026, 11, 4, 10, 0);

  test('英文相对时间使用复数，日期按locale展示', () async {
    await initializeDateFormatting();
    final strings = lookupAppLocalizations(const Locale('en'));
    expect(prettyTime(now, now: now, strings: strings), 'Just now');
    expect(
      prettyTime(
        now.subtract(const Duration(minutes: 1)),
        now: now,
        strings: strings,
      ),
      '1 minute ago',
    );
    expect(
      prettyTime(
        now.subtract(const Duration(minutes: 5)),
        now: now,
        strings: strings,
      ),
      '5 minutes ago',
    );
    expect(
      prettyTime(
        now.subtract(const Duration(hours: 2)),
        now: now,
        strings: strings,
      ),
      '2 hours ago',
    );
    expect(
      prettyTime(
        now.subtract(const Duration(days: 2)),
        now: now,
        strings: strings,
      ),
      '2 days ago',
    );
    final date = DateTime(2025, 12, 30);
    expect(
      prettyTime(date, now: now, strings: strings),
      DateFormat.yMd('en').format(date),
    );
  });

  test('一分钟内 → 刚刚', () {
    expect(
      prettyTime(now.subtract(const Duration(seconds: 30)), now: now),
      '刚刚',
    );
  });

  test('小时内 → n 分钟前', () {
    expect(
      prettyTime(now.subtract(const Duration(minutes: 5)), now: now),
      '5 分钟前',
    );
  });

  test('一天内 → n 小时前', () {
    expect(
      prettyTime(now.subtract(const Duration(hours: 3)), now: now),
      '3 小时前',
    );
  });

  test('一周内 → n 天前', () {
    expect(prettyTime(now.subtract(const Duration(days: 2)), now: now), '2 天前');
  });

  test('当年更早 → MM-dd；跨年 → yyyy-MM-dd', () {
    expect(prettyTime(DateTime(2026, 3, 8, 9, 30), now: now), '03-08');
    expect(prettyTime(DateTime(2025, 12, 30), now: now), '2025-12-30');
  });

  test('prettyTimestamp：解析失败回原样，空值回 null', () {
    // 用本地时间戳（不带 Z），避免测试环境的时区偏移影响断言。
    final local = DateTime(2026, 11, 4, 9, 58).toIso8601String();
    expect(prettyTimestamp(local, now: now), '2 分钟前');
    expect(prettyTimestamp('not-a-time'), 'not-a-time');
    expect(prettyTimestamp(null), isNull);
    expect(prettyTimestamp(''), isNull);
  });
}
