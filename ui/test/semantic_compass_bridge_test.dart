import 'package:elsewhen_ui/bridge/api.dart' as api;
import 'package:flutter_test/flutter_test.dart';

import 'support/isolated_bridge.dart';

void main() {
  test(
    'semantic compass contract and feedback cross the isolated real bridge',
    () async {
      final repo = await createIsolatedBridge();
      expect(await api.getActiveCompassNudge(), isNull);
      expect(await api.tickGoalCompass(), isNull);
      for (final locale in ['en', 'zh-Hans']) {
        final contract = (await api.listPromptDefinitions(locale: locale))
            .singleWhere((prompt) => prompt.key == 'compass.fixed_contract');
        expect(contract.editable, isFalse);
        expect(contract.currentBody, contains('drifting'));
      }
      await expectLater(
        repo.submitCompassFeedback(
          'missing',
          'still_progressing',
          note: '线下已推进',
        ),
        throwsA(anything),
      );
      await expectLater(
        api.submitCompassFeedback(id: 'missing', kind: 'auto_update', note: ''),
        throwsA(anything),
      );
      await reinitializeIsolatedBridge(repo);
      expect(await api.getActiveCompassNudge(), isNull);
    },
  );
}
