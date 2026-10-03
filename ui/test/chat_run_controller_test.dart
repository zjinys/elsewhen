import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/api.dart';
import 'package:elsewhen_ui/providers/chat_run_provider.dart';

import 'support/chat_run_fixture.dart';

void main() {
  ChatRunController controller(
    RunTestRepository repo, {
    String conversation = 'conv-1',
  }) {
    final value = ChatRunController(
      repo,
      conversation,
      pollInterval: const Duration(days: 1),
    );
    addTearDown(value.dispose);
    return value;
  }

  test(
    'restores durable state and retries the original input, not latest message',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun(
        status: 'interrupted',
        input: 'old-input',
      );
      await repo.sendMessage('conv-1', 'a later input');
      final observer = controller(repo);
      await observer.restore();
      expect(observer.state.canRetry, isTrue);
      await observer.retry();
      expect(repo.starts.single.$2, 'old-input');
      expect(repo.messages.length, 1);
      expect(repo.legacyCalls, 0);
    },
  );

  test(
    'an uncertain start is redelivered with the same input and key',
    () async {
      final repo = RunTestRepository()..loseStartResponse = true;
      final observer = controller(repo);
      await observer.start('saved-input');
      expect(observer.state.connectionError, isNotNull);
      expect(observer.state.busy, isTrue);
      await observer.start('different-input');
      expect(repo.starts.length, 1);
      await observer.reconnect();
      expect(repo.starts.length, 2);
      expect(repo.starts[0], repo.starts[1]);
      expect(repo.requests.length, 1);
      expect(observer.state.run?.inputMessageId, 'saved-input');
      expect(observer.state.connectionError, isNull);
    },
  );

  test(
    'bounded event replay rebuilds tools and deduplicates draft deltas',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun(sequence: 1, content: 'checkpoint');
      repo.events.addAll([
        fixtureEvent(1, 'created'),
        fixtureEvent(2, 'draft_reset'),
        fixtureEvent(
          3,
          'text_delta',
          payload: {'text': 'draft', 'validated': false},
        ),
        fixtureEvent(
          3,
          'text_delta',
          payload: {'text': 'duplicate', 'validated': false},
        ),
        fixtureEvent(
          4,
          'tool_started',
          payload: {'call_id': 'a', 'name': 'save_knowledge_draft'},
        ),
        fixtureEvent(
          5,
          'tool_result',
          payload: {
            'call_id': 'a',
            'name': 'save_knowledge_draft',
            'outcome': 'proposal_ready',
            'content': 'private raw output',
          },
        ),
      ]);
      final observer = controller(repo);
      await observer.restore();
      await observer.poll();
      expect(observer.state.draft, 'draft');
      expect(observer.state.tools.single.outcome, 'proposal_ready');
      expect(repo.pageLimits.single, 64);
      expect(observer.state.active, isTrue);
    },
  );

  test(
    'sequence gap falls back to snapshot, not forged terminal event',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun(sequence: 1);
      final observer = controller(repo);
      await observer.restore();
      repo.runs['conv-1'] = fixtureRun(
        sequence: 9,
        content: 'authoritative',
        status: 'completed',
      );
      repo.events.add(
        fixtureEvent(8, 'text_delta', payload: {'text': 'must not append'}),
      );
      await observer.poll();
      expect(observer.state.run?.status, 'completed');
      expect(observer.state.draft, 'authoritative');
      expect(observer.state.historyResynced, isTrue);
      expect(repo.snapshots, 1);
    },
  );

  test('missing terminal event is recovered from snapshot; late deltas cannot revive it', () async {
    final repo = RunTestRepository();
    repo.runs['conv-1'] = fixtureRun();
    final observer = controller(repo);
    await observer.restore();
    repo.runs['conv-1'] = fixtureRun(
      status: 'completed',
      sequence: 3,
      content: 'verified answer',
    );
    await observer.poll();
    repo.events.addAll([
      fixtureEvent(1, 'created'),
      fixtureEvent(2, 'text_delta', payload: {'text': 'late'}),
      fixtureEvent(3, 'completed'),
      fixtureEvent(4, 'text_delta', payload: {'text': 'zombie'}),
    ]);
    await observer.poll();
    expect(observer.state.run?.status, 'completed');
    expect(observer.state.draft, 'verified answer');
    expect(observer.state.active, isFalse);
  });

  test(
    'wrong-run event resynchronizes without applying private data',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun(content: 'safe');
      repo.pageOverride = (_, run, _) async => ChatRunEventPageDto(
        runId: run,
        events: [
          fixtureEvent(
            1,
            'text_delta',
            run: 'other',
            payload: {'text': 'private'},
          ),
        ],
        nextSequence: 1,
        hasMore: false,
      );
      final observer = controller(repo);
      await observer.restore();
      await observer.poll();
      expect(observer.state.draft, 'safe');
      expect(observer.state.historyResynced, isTrue);
    },
  );

  test(
    'stop is scoped to one conversation; disposing never cancels a run',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun();
      repo.runs['other'] = fixtureRun(id: 'other-run', conversation: 'other');
      final observer = controller(repo);
      final other = ChatRunController(
        repo,
        'other',
        pollInterval: const Duration(days: 1),
      );
      await observer.restore();
      await other.restore();
      other.dispose();
      expect(repo.stops, isEmpty);
      await observer.stop();
      expect(repo.stops, [('conv-1', 'run-1')]);
      expect(repo.runs['other']?.status, 'running');
      expect(observer.state.run?.status, 'cancelled');
    },
  );

  test(
    'polls never overlap and a late response after disposal is ignored',
    () async {
      final repo = RunTestRepository();
      repo.runs['conv-1'] = fixtureRun();
      final gate = Completer<ChatRunEventPageDto>();
      repo.pageOverride = (_, _, _) => gate.future;
      final observer = ChatRunController(
        repo,
        'conv-1',
        pollInterval: const Duration(days: 1),
      );
      await observer.restore();
      final pending = observer.poll();
      await observer.poll();
      expect(repo.pageLimits.length, 1);
      observer.dispose();
      gate.complete(
        const ChatRunEventPageDto(
          runId: 'run-1',
          events: [],
          nextSequence: 0,
          hasMore: false,
        ),
      );
      await pending;
      expect(repo.stops, isEmpty);
    },
  );
}
