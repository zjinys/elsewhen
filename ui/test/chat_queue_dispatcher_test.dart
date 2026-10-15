import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/providers/chat_run_provider.dart';
import 'package:elsewhen_ui/bridge/chat_queue_dispatcher.dart';
import 'package:elsewhen_ui/widgets/chat_input_queue.dart';

import 'chat_input_queue_widget_test.dart' show QueueRepository;
import 'chat_run_widget_test.dart' show app, settleFrames;
import 'support/chat_run_fixture.dart' show fixtureRun;

class _ObservationRepository extends QueueRepository {
  @override
  final ValueNotifier<ChatRunDto?> chatQueueRunStarted = ValueNotifier(null);
  int dispatches = 0;
  @override
  Future<Never> dispatchChatInputQueue(String conversationId) async {
    dispatches++;
    throw StateError('Widgets must not dispatch queues');
  }
}

void main() {
  testWidgets('dispatcher renews ownership without a mounted chat view', (
    tester,
  ) async {
    final calls = <String>[];
    var paused = false;
    final dispatcher = ChatQueueDispatcher(
      poll: (id) async {
        calls.add(id);
        return !paused;
      },
    );
    dispatcher.watch('conversation');
    await tester.pump();
    for (var i = 0; i < 35; i++) {
      await tester.pump(const Duration(seconds: 1));
    }
    expect(calls.length, 36);
    expect(calls.toSet(), {'conversation'});
    paused = true;
    await tester.pump(const Duration(seconds: 1));
    final stoppedAt = calls.length;
    await tester.pump(const Duration(seconds: 35));
    expect(
      calls.length,
      stoppedAt,
      reason: 'paused/expired ownership is never resumed',
    );
    dispatcher.dispose();
  });

  testWidgets('uncertain polling retries but disposal prevents further work', (
    tester,
  ) async {
    var calls = 0;
    final dispatcher = ChatQueueDispatcher(
      poll: (_) async {
        if (++calls == 1) throw StateError('synthetic lost response');
        return true;
      },
    );
    dispatcher.watch('conversation');
    await tester.pump();
    await tester.pump(const Duration(seconds: 1));
    expect(calls, 2);
    dispatcher.dispose();
    await tester.pump(const Duration(seconds: 35));
    expect(calls, 2);
  });

  testWidgets('explicit resume supersedes a late paused observation', (
    tester,
  ) async {
    final oldPoll = Completer<bool>();
    var calls = 0;
    final dispatcher = ChatQueueDispatcher(
      poll: (_) async {
        calls++;
        return calls == 1 ? oldPoll.future : true;
      },
    );
    dispatcher.watch('conversation');
    await tester.pump(const Duration(milliseconds: 1));
    expect(calls, 1);
    dispatcher.watch('conversation');
    oldPoll.complete(false);
    await settleFrames(tester);
    await tester.pump(const Duration(seconds: 1));
    await settleFrames(tester);
    expect(calls, 2);
    dispatcher.dispose();
  });

  testWidgets('repository run notifications refresh the visible controller', (
    tester,
  ) async {
    final repo = _ObservationRepository();
    repo.runs['conv-1'] = fixtureRun(status: 'completed');
    await tester.pumpWidget(
      app(repo, const ChatInputQueue(conversationId: 'conv-1')),
    );
    final container = ProviderScope.containerOf(
      tester.element(find.byType(ChatInputQueue)),
    );
    final subscription = container.listen(
      chatRunControllerProvider('conv-1'),
      (_, _) {},
    );
    await settleFrames(tester);
    expect(subscription.read().state.run?.id, 'run-1');
    final next = fixtureRun(id: 'run-2', input: 'input-2');
    repo.runs['conv-1'] = next;
    repo.chatQueueRunStarted.value = next;
    await settleFrames(tester);
    expect(subscription.read().state.run?.id, 'run-2');
    expect(repo.dispatches, 0);
    subscription.close();
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
    repo.chatQueueRunStarted.dispose();
  });

  testWidgets('queue view reads state without starting or renewing dispatch', (
    tester,
  ) async {
    final repo = _ObservationRepository();
    await tester.pumpWidget(
      app(repo, const ChatInputQueue(conversationId: 'conv-1')),
    );
    await settleFrames(tester);
    await tester.pump(const Duration(seconds: 1));
    await settleFrames(tester);
    expect(repo.dispatches, 0);
    await tester.pumpWidget(const SizedBox());
    await tester.pump();
  });
}
