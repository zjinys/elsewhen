# Conversation API Implementation

## 概述

会话（Conversation）API 已完整实现并通过测试，支持创建会话、发送消息和查询对话历史。所有功能通过 Flutter Rust Bridge 暴露给 Flutter UI。

## 实现状态

✅ **完成** - 所有功能已实现并通过测试

- Rust 后端 API（`src/api.rs`）
- SQLite 数据库存储（`src/storage.rs`）
- Flutter Rust Bridge 绑定（`ui/lib/bridge/generated.dart/`）
- Dart 模型和 Repository（`ui/lib/`）
- 单元测试覆盖（9个测试全部通过）

## API 方法

### 1. 创建会话

```rust
// Rust
#[frb]
pub fn create_conversation(title: Option<String>) -> Result<ConversationDto>
```

```dart
// Dart
Future<Conversation> createConversation({String? title})
```

**参数:**
- `title` (可选): 会话标题，默认为 null（未命名会话）

**返回:**
- `Conversation`: 新创建的会话对象，包含自动生成的 UUID

**示例:**
```dart
final conversation = await bridge.createConversation(title: 'Flutter 讨论');
```

### 2. 列出所有会话

```rust
// Rust
#[frb]
pub fn list_conversations() -> Result<Vec<ConversationDto>>
```

```dart
// Dart
Future<List<Conversation>> listConversations()
```

**返回:**
- `List<Conversation>`: 所有会话列表，按 `updated_at` 倒序排列（最近更新的在前）

**特性:**
- 自动计算每个会话的消息数量（`message_count`）
- 显示最后一条消息的预览（`last_message_preview`，最多50字符）

**示例:**
```dart
final conversations = await bridge.listConversations();
for (var conv in conversations) {
  print('${conv.displayTitle}: ${conv.messageCount} messages');
}
```

### 3. 获取特定会话

```rust
// Rust
#[frb]
pub fn get_conversation(conversation_id: String) -> Result<Option<ConversationDto>>
```

```dart
// Dart
Future<Conversation?> getConversation(String conversationId)
```

**参数:**
- `conversationId`: 会话 UUID

**返回:**
- `Conversation?`: 会话对象，不存在时返回 null

### 4. 发送消息

```rust
// Rust
#[frb]
pub fn send_message(
    conversation_id: String,
    role: String,
    content: String
) -> Result<MessageDto>
```

```dart
// Dart
Future<Message> sendMessage(
  String conversationId,
  String role,
  String content
)
```

**参数:**
- `conversationId`: 会话 UUID
- `role`: 消息角色（'user', 'assistant', 'system'）
- `content`: 消息内容

**返回:**
- `Message`: 新创建的消息对象

**副作用:**
- 自动更新会话的 `updated_at` 时间戳（使用数据库事务保证一致性）

**示例:**
```dart
final message = await bridge.sendMessage(
  conversationId,
  'user',
  '你好，请帮我分析这段代码',
);
```

### 5. 列出会话消息

```rust
// Rust
#[frb]
pub fn list_messages(conversation_id: String) -> Result<Vec<MessageDto>>
```

```dart
// Dart
Future<List<Message>> listMessages(String conversationId)
```

**参数:**
- `conversationId`: 会话 UUID

**返回:**
- `List<Message>`: 该会话的所有消息，按 `created_at` 正序排列（时间顺序）

**示例:**
```dart
final messages = await bridge.listMessages(conversationId);
for (var msg in messages) {
  print('[${msg.role}] ${msg.content}');
}
```

## 数据模型

### Rust DTO

```rust
#[frb(dart_metadata=("freezed"))]
pub struct ConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,      // RFC3339 格式
    pub updated_at: String,      // RFC3339 格式
    pub message_count: i32,
    pub last_message_preview: Option<String>,
}

#[frb(dart_metadata=("freezed"))]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub role: String,            // "user" | "assistant" | "system"
    pub content: String,
    pub created_at: String,      // RFC3339 格式
}
```

### Dart 模型

```dart
@freezed
class Conversation with _$Conversation {
  const factory Conversation({
    required String id,
    String? title,
    required DateTime createdAt,
    required DateTime updatedAt,
    required int messageCount,
    String? lastMessagePreview,
  }) = _Conversation;
}

@freezed
class Message with _$Message {
  const factory Message({
    required String id,
    required String conversationId,
    required MessageRole role,
    required String content,
    required DateTime createdAt,
  }) = _Message;
}

enum MessageRole {
  user,
  assistant,
  system;
}
```

### 扩展方法

```dart
extension ConversationExtensions on Conversation {
  String get displayTitle => title ?? 'Untitled Conversation';
  bool get isEmpty => messageCount == 0;
}

extension MessageExtensions on Message {
  bool get isUser => role == MessageRole.user;
  bool get isAssistant => role == MessageRole.assistant;
  bool get isSystem => role == MessageRole.system;
}
```

## Repository 层

### ConversationRepository

```dart
class ConversationRepository {
  final RustBridgeRepository _bridge;

  ConversationRepository(this._bridge);

  Future<List<Conversation>> getConversations() async {
    return await _bridge.listConversations();
  }

  Future<List<Message>> getMessages(String conversationId) async {
    return await _bridge.listMessages(conversationId);
  }

  Future<Conversation> createConversation() async {
    return await _bridge.createConversation();
  }

  Future<Message> sendMessage(String conversationId, String content) async {
    return await _bridge.sendMessage(conversationId, 'user', content);
  }
}
```

### Riverpod Providers

```dart
// Repository provider
final conversationRepositoryProvider = Provider<ConversationRepository>((ref) {
  final bridge = ref.read(storageRepositoryProvider) as RustBridgeRepository;
  return ConversationRepository(bridge);
});

// Conversations list provider
final conversationsProvider = FutureProvider<List<Conversation>>((ref) async {
  final repo = ref.read(conversationRepositoryProvider);
  return await repo.getConversations();
});

// Selected conversation ID provider
final selectedConversationIdProvider = StateProvider<String?>((ref) => null);

// Messages for selected conversation provider
final messagesProvider = FutureProvider<List<Message>>((ref) async {
  final conversationId = ref.watch(selectedConversationIdProvider);
  if (conversationId == null) return [];

  final repo = ref.read(conversationRepositoryProvider);
  return await repo.getMessages(conversationId);
});

// Message input provider
final messageInputProvider = StateProvider<String>((ref) => '');
```

## 数据库实现

### 表结构

```sql
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation 
    ON messages(conversation_id, created_at);
```

### 关键特性

1. **外键级联删除**: 删除会话时自动删除所有关联消息
2. **角色校验**: CHECK 约束确保 role 只能是 'user', 'assistant', 'system'
3. **索引优化**: 为常用查询（按会话和时间查询消息）添加复合索引
4. **事务一致性**: 发送消息时使用事务同时更新会话时间戳

### Store 实现

```rust
impl Store {
    pub fn create_conversation(&self, title: Option<&str>) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        
        self.connection.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) 
             VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now, now],
        )?;
        
        Ok(id)
    }

    pub fn send_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str
    ) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        
        let tx = self.connection.transaction()?;
        
        tx.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, conversation_id, role, content, now],
        )?;
        
        tx.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now, conversation_id],
        )?;
        
        tx.commit()?;
        Ok(id)
    }
}
```

## 测试覆盖

### 单元测试（src/storage_tests.rs）

所有 9 个测试通过：

1. ✅ **create_conversation_returns_id**: 验证创建会话返回非空 ID
2. ✅ **list_conversations_shows_created_conversations**: 验证会话列表按更新时间倒序
3. ✅ **send_message_creates_message_in_conversation**: 验证消息创建和检索
4. ✅ **list_messages_returns_in_chronological_order**: 验证消息按时间正序排列
5. ✅ **conversation_updated_at_changes_on_message**: 验证发送消息更新会话时间戳
6. ✅ **message_count_reflects_actual_messages**: 验证消息计数准确性
7. ✅ **last_message_preview_shows_latest_content**: 验证最后消息预览
8. ✅ **invalid_role_is_rejected**: 验证角色校验（CHECK 约束）
9. ✅ **get_nonexistent_conversation_returns_none**: 验证不存在的会话返回 None

运行测试：

```bash
cargo test storage_tests
```

### Flutter 集成测试（ui/test/conversation_repository_test.dart）

```dart
test('should create a new conversation', () async {
  final conversation = await repository.createConversation();
  
  expect(conversation.id, isNotEmpty);
  expect(conversation.messageCount, equals(0));
  expect(conversation.createdAt, isNotNull);
});

test('should send a message and retrieve it', () async {
  final conversation = await repository.createConversation();
  
  final message = await repository.sendMessage(
    conversation.id,
    'Hello from test',
  );
  
  expect(message.content, equals('Hello from test'));
  expect(message.role, equals(MessageRole.user));
  
  final messages = await repository.getMessages(conversation.id);
  expect(messages.first.content, equals('Hello from test'));
});
```

运行测试：

```bash
cd ui
fvm flutter test test/conversation_repository_test.dart
```

## 使用示例

### 创建新会话并发送消息

```dart
// 创建会话
final repo = ref.read(conversationRepositoryProvider);
final conversation = await repo.createConversation();

// 发送消息
final message = await repo.sendMessage(
  conversation.id,
  '帮我分析一下这个错误',
);

// 获取消息历史
final messages = await repo.getMessages(conversation.id);
```

### 在 UI 中使用 Provider

```dart
// 获取会话列表
final conversationsAsync = ref.watch(conversationsProvider);

conversationsAsync.when(
  data: (conversations) {
    return ListView.builder(
      itemCount: conversations.length,
      itemBuilder: (context, index) {
        final conv = conversations[index];
        return ListTile(
          title: Text(conv.displayTitle),
          subtitle: Text(conv.lastMessagePreview ?? ''),
          trailing: Text('${conv.messageCount}'),
          onTap: () {
            ref.read(selectedConversationIdProvider.notifier).state = conv.id;
          },
        );
      },
    );
  },
  loading: () => CircularProgressIndicator(),
  error: (error, stack) => Text('Error: $error'),
);

// 显示消息
final messagesAsync = ref.watch(messagesProvider);

messagesAsync.when(
  data: (messages) {
    return ListView.builder(
      itemCount: messages.length,
      itemBuilder: (context, index) {
        final msg = messages[index];
        return MessageBubble(
          content: msg.content,
          isUser: msg.isUser,
          timestamp: msg.createdAt,
        );
      },
    );
  },
  loading: () => CircularProgressIndicator(),
  error: (error, stack) => Text('Error: $error'),
);
```

## 架构优势

1. **统一接口**: 通过 `RustBridgeRepository` 统一访问存储和会话 API
2. **类型安全**: Dart 模型与 Rust DTO 完全对应，使用 Freezed 保证不可变性
3. **可测试性**: Repository 模式便于 mock 和单元测试
4. **状态管理**: Riverpod Provider 实现响应式 UI 更新
5. **错误处理**: Future 返回值支持完整的异步错误处理
6. **数据一致性**: 数据库事务和外键约束保证数据完整性
7. **性能优化**: 索引优化和批量查询减少数据库访问

## 性能考虑

### 查询优化

- `list_conversations()` 使用 LEFT JOIN 一次性获取消息统计，避免 N+1 查询
- `list_messages()` 使用索引加速按会话和时间的查询
- 最后消息预览限制为 50 字符，减少数据传输

### 事务使用

- `send_message()` 使用事务确保消息插入和会话时间戳更新的原子性
- 避免在长时间操作中持有事务锁

## 后续扩展

可选的未来功能：

- [ ] 实现消息流式输出（AI 回复打字机效果）
- [ ] 添加消息搜索功能（全文搜索索引）
- [ ] 支持会话归档/删除（软删除或实现 delete_conversation）
- [ ] 实现会话元数据（标签、分类、星标）
- [ ] 添加消息附件支持（文件上传）
- [ ] 支持消息编辑和删除（保留编辑历史）
- [ ] 实现会话分享功能（导出/导入）

## 故障排查

### 常见问题

**Q: 消息发送后会话列表顺序未更新**  
A: 确保 `send_message` 的事务正确提交，检查 `updated_at` 字段是否更新

**Q: 删除会话后消息仍然存在**  
A: 检查数据库是否正确启用外键约束（SQLite 需要 `PRAGMA foreign_keys = ON`）

**Q: 角色字段接受了无效值**  
A: 检查 CHECK 约束是否正确创建，SQLite 默认忽略约束需要确保约束已启用

### 调试工具

```bash
# 查看数据库内容
sqlite3 ~/.local/share/elsewhen/elsewhen.db

# 检查会话
SELECT * FROM conversations ORDER BY updated_at DESC;

# 检查消息
SELECT m.*, c.title 
FROM messages m 
JOIN conversations c ON m.conversation_id = c.id 
ORDER BY m.created_at;

# 检查外键约束
PRAGMA foreign_keys;
PRAGMA foreign_key_check;
```

## 相关文档

- [存储适配器文档](STORAGE_ADAPTER.md)
- [Flutter Rust Bridge 配置](../flutter_rust_bridge.yaml)
- [数据库模式](../src/storage.rs#L98-L114)
- [API 实现](../src/api.rs#L148-L245)
