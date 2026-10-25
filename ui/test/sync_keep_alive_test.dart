import 'package:elsewhen_ui/sync/sync_keep_alive.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final calls = <String>[];
  final texts = <String?>[];

  setUp(() {
    calls.clear();
    texts.clear();
    SyncKeepAlive.debugSupportedOverride = true;
    SyncKeepAlive.debugInvoke = (method, arguments) {
      calls.add(method);
      texts.add(
        arguments is Map && arguments['text'] is String
            ? arguments['text'] as String
            : null,
      );
    };
  });

  tearDown(() {
    SyncKeepAlive.debugSupportedOverride = null;
    SyncKeepAlive.debugInvoke = null;
    // 清掉跨用例残留的持有者。
    SyncKeepAlive.releaseAll();
    calls.clear();
  });

  test('多个持有者只拉起一次，最后一个释放才停掉', () async {
    SyncKeepAlive.hold('sync-join', text: '正在与已有设备配对');
    SyncKeepAlive.hold('sync-enrollment', text: '正在完成设备交接与初始同步');
    await pumpEventQueue();
    expect(calls, ['start']);
    expect(texts.first, '正在与已有设备配对');

    SyncKeepAlive.release('sync-join');
    await pumpEventQueue();
    expect(calls, ['start'], reason: '还有持有者时不能关灯');

    SyncKeepAlive.release('sync-enrollment');
    await pumpEventQueue();
    expect(calls, ['start', 'stop']);
  });

  test('重复释放同一个持有者不会误停别人的保活', () async {
    SyncKeepAlive.hold('sync-setup');
    SyncKeepAlive.hold('sync-pairing-owner');
    await pumpEventQueue();
    SyncKeepAlive.release('sync-setup');
    SyncKeepAlive.release('sync-setup');
    await pumpEventQueue();
    expect(calls, ['start']);
    SyncKeepAlive.release('sync-pairing-owner');
    await pumpEventQueue();
    expect(calls, ['start', 'stop']);
  });

  test('收尾可以一次清空所有持有者', () async {
    SyncKeepAlive.hold('sync-join');
    SyncKeepAlive.hold('sync-enrollment');
    await pumpEventQueue();
    SyncKeepAlive.releaseAll();
    await pumpEventQueue();
    expect(calls, ['start', 'stop']);
    SyncKeepAlive.releaseAll();
    await pumpEventQueue();
    expect(calls, ['start', 'stop'], reason: '没挂着的时候不必再发一次 stop');
  });

  test('不支持的平台上是彻底的空操作', () async {
    SyncKeepAlive.debugSupportedOverride = false;
    SyncKeepAlive.hold('sync-join');
    SyncKeepAlive.release('sync-join');
    SyncKeepAlive.releaseAll();
    await pumpEventQueue();
    expect(calls, isEmpty);
    expect(SyncKeepAlive.active, isFalse);
  });

  test('缺省文案不会为空', () async {
    SyncKeepAlive.hold('sync-join');
    await pumpEventQueue();
    expect(texts.single, isNotNull);
    expect(texts.single, isNotEmpty);
  });
}
