import 'dart:convert';
import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/utils/window_geometry_store.dart';

void main() {
  const minSize = Size(800, 600);

  group('WindowGeometry.tryParse', () {
    test('合法 JSON → 解析成功', () {
      final geo = WindowGeometry.tryParse({
        'x': 120.0,
        'y': 80.0,
        'width': 1440.0,
        'height': 900.0,
      });
      expect(geo, isNotNull);
      expect(geo!.size, const Size(1440, 900));
      expect(geo.position, const Offset(120, 80));
    });

    test('缺失字段 / 类型不对 / 尺寸非法 → null', () {
      expect(WindowGeometry.tryParse({'x': 1, 'y': 2, 'width': 3}), isNull);
      expect(
        WindowGeometry.tryParse({'x': 'a', 'y': 2, 'width': 3, 'height': 4}),
        isNull,
      );
      expect(
        WindowGeometry.tryParse({'x': 1, 'y': 2, 'width': 0, 'height': 4}),
        isNull,
      );
      expect(WindowGeometry.tryParse('not a map'), isNull);
    });

    test('toJson/round-trip', () {
      const geo = WindowGeometry(x: 1, y: 2, width: 800, height: 600);
      final restored = WindowGeometry.tryParse(geo.toJson());
      expect(restored, isNotNull);
      expect(restored!.x, geo.x);
      expect(restored.y, geo.y);
      expect(restored.width, geo.width);
      expect(restored.height, geo.height);
    });
  });

  group('WindowGeometryStore 文件存取', () {
    late Directory dir;
    late WindowGeometryStore store;

    setUp(() async {
      dir = await Directory.systemTemp.createTemp('window_geo_test_');
      store = WindowGeometryStore(directoryOverride: dir.path);
    });

    tearDown(() async {
      if (await dir.exists()) await dir.delete(recursive: true);
    });

    test('无文件 → load 返回 null', () {
      expect(store.load(), isNull);
    });

    test('save → load 往返一致', () {
      const geo = WindowGeometry(x: 100, y: 50, width: 1280, height: 800);
      store.save(geo);
      final loaded = store.load();
      expect(loaded, isNotNull);
      expect(loaded!.size, const Size(1280, 800));
      expect(loaded.position, const Offset(100, 50));
      // 落盘的是可读 JSON 字段名
      final raw = jsonDecode(
        File(
          '${dir.path}${Platform.pathSeparator}${WindowGeometryStore.fileName}',
        ).readAsStringSync(),
      );
      expect(raw, containsPair('width', 1280));
    });

    test('损坏内容 → load 返回 null', () {
      File('${dir.path}/${WindowGeometryStore.fileName}')
          .writeAsStringSync('{broken json');
      expect(store.load(), isNull);
    });

    test('合法目录自动创建（父目录不存在也能 save）', () async {
      final nested = Directory(
        '${dir.path}${Platform.pathSeparator}nested${Platform.pathSeparator}deep',
      );
      final nestedStore = WindowGeometryStore(directoryOverride: nested.path);
      nestedStore.save(
        const WindowGeometry(x: 0, y: 0, width: 900, height: 700),
      );
      expect(await nested.exists(), isTrue);
      expect(nestedStore.load(), isNotNull);
    });
  });

  group('WindowGeometryStore.computeRestoreBounds', () {
    const primary = Rect.fromLTWH(0, 0, 1920, 1080);
    // 左侧副屏（负坐标显示器）
    const secondary = Rect.fromLTWH(-1280, 0, 1280, 1024);

    test('可见区内合法几何 → 原样恢复（不小于最小尺寸）', () {
      const saved = WindowGeometry(x: 120, y: 80, width: 1440, height: 900);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        primary,
      ], minSize: minSize);
      expect(r, const Rect.fromLTWH(120, 80, 1440, 900));
    });

    test('小于最小尺寸 → 撑到最小尺寸', () {
      const saved = WindowGeometry(x: 10, y: 10, width: 500, height: 400);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        primary,
      ], minSize: minSize);
      expect(r!.width, 800);
      expect(r.height, 600);
    });

    test('比可见区大 → 夹到可见区，位置夹回', () {
      // 中心（1000,1000）仍在屏内，尺寸超出 → 夹到整屏、位置归零
      const saved = WindowGeometry(x: 0, y: 0, width: 2000, height: 2000);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        primary,
      ], minSize: minSize);
      expect(r!, const Rect.fromLTWH(0, 0, 1920, 1080));
    });

    test('部分在屏幕外 → 位置推回完全可见', () {
      // 右缘超出主屏（中心 1750 仍在屏内）
      const saved = WindowGeometry(x: 1300, y: 100, width: 900, height: 800);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        primary,
      ], minSize: minSize);
      expect(r, isNotNull);
      expect(r!.right, 1920);
      expect(r.width, 900);
      // 上缘超出（中心 550,100 仍在屏内）
      const savedTop = WindowGeometry(x: 100, y: -300, width: 900, height: 800);
      final r2 = WindowGeometryStore.computeRestoreBounds(savedTop, [
        primary,
      ], minSize: minSize);
      expect(r2!.top, 0);
    });

    test('多屏：保存在副屏 → 恢复仍落副屏', () {
      const saved = WindowGeometry(x: -1100, y: 50, width: 1000, height: 800);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        secondary,
        primary,
      ], minSize: minSize);
      expect(r, isNotNull);
      expect(r!.left, -1100);
      expect(r.top, 50);
    });

    test('中心不在任何可见区（拔了外接屏）→ null', () {
      const saved = WindowGeometry(x: 2000, y: 2000, width: 800, height: 600);
      final r = WindowGeometryStore.computeRestoreBounds(saved, [
        primary,
      ], minSize: minSize);
      expect(r, isNull);
    });

    test('无可见区信息 → 尺寸夹最小、位置原样', () {
      const saved = WindowGeometry(x: 300, y: 200, width: 640, height: 480);
      final r = WindowGeometryStore.computeRestoreBounds(
        saved,
        const [],
        minSize: minSize,
      );
      expect(r, const Rect.fromLTWH(300, 200, 800, 600));
    });
  });
}
