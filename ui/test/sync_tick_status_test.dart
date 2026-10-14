import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:elsewhen_ui/widgets/sync_tick_status.dart';

void main() {
  api.SyncTickResultDto tick({
    bool ran = true,
    int uploaded = 0,
    int pulledAccepted = 0,
    int pulledWaiting = 0,
    int pulledRejected = 0,
    int pulledVersionMismatch = 0,
    bool rosterConverged = false,
    bool acknowledgementFailed = false,
  }) => api.SyncTickResultDto(
        ran: ran,
        stagedOperations: 0,
        uploaded: uploaded,
        pulledAccepted: pulledAccepted,
        pulledWaiting: pulledWaiting,
        pulledRejected: pulledRejected,
        pulledVersionMismatch: pulledVersionMismatch,
        rosterConverged: rosterConverged,
        acknowledgementFailed: acknowledgementFailed,
        resnapshotPending: const [],
      );

  Future<void> pump(
    WidgetTester tester, {
    required ValueNotifier<api.SyncTickResultDto?> ticks,
    required ValueNotifier<Object?> errors,
  }) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SyncTickStatusLine(ticks: ticks, errors: errors),
        ),
      ),
    );
  }

  testWidgets('shows P4e diagnostic signals when present', (tester) async {
    final ticks = ValueNotifier<api.SyncTickResultDto?>(
      tick(uploaded: 2, pulledAccepted: 5, rosterConverged: true, pulledVersionMismatch: 3),
    );
    final errors = ValueNotifier<Object?>(null);
    await pump(tester, ticks: ticks, errors: errors);
    expect(find.textContaining('上行 2 · 到达 5'), findsOneWidget);
    expect(find.textContaining('名册已补发'), findsOneWidget);
    expect(find.textContaining('版本不匹配 3'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('omits silent signals and shows skip when not run', (
    tester,
  ) async {
    final ticks = ValueNotifier<api.SyncTickResultDto?>(tick(ran: false));
    final errors = ValueNotifier<Object?>(null);
    await pump(tester, ticks: ticks, errors: errors);
    expect(find.textContaining('本轮跳过'), findsOneWidget);
    expect(find.textContaining('名册已补发'), findsNothing);
    expect(find.textContaining('版本不匹配'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('errors take precedence over tick lines', (tester) async {
    final ticks = ValueNotifier<api.SyncTickResultDto?>(tick());
    final errors = ValueNotifier<Object?>('relay unreachable');
    await pump(tester, ticks: ticks, errors: errors);
    expect(find.textContaining('最近同步错误'), findsOneWidget);
    expect(find.textContaining('最近同步：'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
