import 'dart:io';
import 'dart:ui' as ui;
import 'package:flutter/services.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:elsewhen_ui/theme/app_theme.dart';
import 'package:elsewhen_ui/widgets/local_sync_settings.dart';
import 'package:elsewhen_ui/widgets/owner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/widgets/joiner_sync_pairing_settings.dart';
import 'package:elsewhen_ui/sync/sync_setup_controller.dart';
import 'package:elsewhen_ui/sync/sync_join_controller.dart';
import 'package:elsewhen_ui/sync/sync_key_store.dart';
import 'support/sync_setup.dart';
import 'support/sync_join.dart';
import 'sync_owner_pairing_test.dart' as owner;
import 'sync_pairing_terminal_test.dart' as terminal;
void main() {
 testWidgets('pairing visual preview', (tester) async {
  tester.view.physicalSize = const Size(390, 844);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.runAsync(() async {
   final bytes = await File('/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc').readAsBytes();
   for (final family in ['Noto Sans SC', 'Ahem', 'Inter', 'Roboto']) {
    await (FontLoader(family)..addFont(Future.value(ByteData.sublistView(bytes)))).load();
   }
  });
  AppTheme.apply(Brightness.dark);
  Future<void> capture(Widget child, String name) async {
   await tester.pumpWidget(MaterialApp(theme: AppTheme.darkTheme, home: Scaffold(body: RepaintBoundary(key: const ValueKey('capture'), child: SingleChildScrollView(padding: const EdgeInsets.all(20), child: child)))));
   await tester.pump(const Duration(milliseconds: 100));
   final boundary = tester.renderObject<RenderRepaintBoundary>(find.byKey(const ValueKey('capture')));
   await tester.runAsync(() async {
    final image = await boundary.toImage(pixelRatio: 1.5);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    await File('/tmp/elsewhen-pairing-$name.png').writeAsBytes(data!.buffer.asUint8List());
    image.dispose();
   });
   expect(tester.takeException(), isNull);
  }
  final storage = TestSyncSecureStorage();
  final setup = SyncSetupController(keyStore: SyncKeyStore(storage: storage), bridge: TestSyncSetupApi(storage));
  await setup.refresh();
  await capture(LocalSyncSettings(controller: setup, datasetApi: terminal.NoopDatasetSwitchApi()), 'entry');
  final f = owner.Fixture();
  await f.initialize();
  await f.controller.createInvitation('http://localhost:9876', consent: true);
  await capture(OwnerSyncPairingSettings(controller: f.controller), 'invite');
  final join = SyncJoinController(keyStore: SyncKeyStore(storage: storage), bridge: TestSyncJoinApi(storage));
  await join.preview('SYNTHETIC');
  await join.join(join.state!, 'My phone', 'android', consent: true);
  await capture(JoinerSyncPairingSettings(controller: join, datasetApi: terminal.NoopDatasetSwitchApi()), 'waiting');
  await tester.pumpWidget(const SizedBox());
  await setup.close(); setup.dispose();
  await f.close(); await join.close(); join.dispose();
 });
}
