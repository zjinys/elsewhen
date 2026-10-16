import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/providers/conversation_provider.dart';
import 'package:elsewhen_ui/providers/state_holder.dart';

/// 多设备实时刷新：同步 tick 真正拉到对端数据（pulledAccepted>0）时 bump
/// **同步到达修订号**（syncArrivalRevision），会话列表与打开中的消息列表
/// 据此重查——数据自动上台，无需重启 App。
///
/// 为什么不挂 knowledgeRevision：分析/消化产出的 bump 与会话视图无关，
/// 挂上去会让消息列表被无关重查反复拆装（闪烁、滚动位置丢失），
/// 这里同时用「knowledgeRevision bump 不重查」锁死该回归。

class _CountingRepo extends ConversationRepository {
  _CountingRepo() : super(RustBridgeRepository());
  int listCalls = 0;
  int messageCalls = 0;

  @override
  Future<List<Conversation>> getConversations() async {
    listCalls++;
    return const <Conversation>[];
  }

  @override
  Future<List<Conversation>> getArchivedConversations() async {
    return const <Conversation>[];
  }

  @override
  Future<List<Message>> getMessages(String conversationId) async {
    messageCalls++;
    return const <Message>[];
  }
}

class _RevisionNotifier extends Notifier<int> {
  @override
  int build() => 0;
  void bump() => state++;
}

final _syncArrivalTestProvider = NotifierProvider<_RevisionNotifier, int>(
  _RevisionNotifier.new,
);
final _knowledgeTestProvider = NotifierProvider<_RevisionNotifier, int>(
  _RevisionNotifier.new,
);

ProviderContainer _container(_CountingRepo repo) {
  final container = ProviderContainer(
    overrides: [
      conversationRepositoryProvider.overrideWithValue(repo),
      syncArrivalRevisionProvider.overrideWith(
        (ref) => ref.watch(_syncArrivalTestProvider),
      ),
      knowledgeRevisionProvider.overrideWith(
        (ref) => ref.watch(_knowledgeTestProvider),
      ),
      selectedConversationIdProvider.overrideWith(
        () => StateHolder<String?>('conv-1'),
      ),
    ],
  );
  return container;
}

void main() {
  test('同步到达修订号 bump 后会话列表自动重查（对端新增对话无需重启才出现）', () async {
    final repo = _CountingRepo();
    final container = _container(repo);
    addTearDown(container.dispose);

    // 首次读取：正常查库一次。
    await container.read(conversationsProvider.future);
    expect(repo.listCalls, 1);

    // 没有新数据（未 bump）不重查——FutureProvider 缓存住。
    await container.read(conversationsProvider.future);
    expect(repo.listCalls, 1);

    // 同步 tick 拉到对端数据（bump）→ 依赖变化 → 下次读取时重查。
    container.read(_syncArrivalTestProvider.notifier).bump();
    await container.read(conversationsProvider.future);
    expect(repo.listCalls, 2);
  });

  test('打开中的会话在同步到达修订号 bump 后消息列表自动重查', () async {
    final repo = _CountingRepo();
    final container = _container(repo);
    addTearDown(container.dispose);

    await container.read(messagesProvider.future);
    expect(repo.messageCalls, 1);

    container.read(_syncArrivalTestProvider.notifier).bump();
    await container.read(messagesProvider.future);
    expect(repo.messageCalls, 2);
  });

  test('知识修订号 bump **不再**触发会话/消息重查（防列表闪跳回归）', () async {
    final repo = _CountingRepo();
    final container = _container(repo);
    addTearDown(container.dispose);

    await container.read(conversationsProvider.future);
    await container.read(messagesProvider.future);
    expect(repo.listCalls, 1);
    expect(repo.messageCalls, 1);

    // 分析/消化产出的 bump（_drainKnowledgeDigest 等）不应惊动会话视图。
    container.read(_knowledgeTestProvider.notifier).bump();
    await container.read(conversationsProvider.future);
    await container.read(messagesProvider.future);
    expect(repo.listCalls, 1);
    expect(repo.messageCalls, 1);
  });
}
