# 下一步工作建议

## 已完成
✅ Storage Adapter 架构实现完成
✅ 抽象接口定义 (StorageRepository)
✅ 生产实现 (RustBridgeRepository)
✅ 测试实现 (MockStorageRepository)
✅ 单元测试覆盖 (7 tests passing)

## 建议的后续任务

根据项目需求文档 (FR-PES-003-Flutter统一GUI.md) 和当前代码状态，以下是优先级排序的任务：

### 1. 完善 Rust Bridge API (高优先级)

**现状**: `src/api.rs` 中已有基础事件 API，但缺少会话/消息相关接口

**需要添加的 API**:
```rust
// 会话管理
create_conversation() -> Result<ConversationDto>
list_conversations() -> Result<Vec<ConversationDto>>
get_conversation(conversation_id: String) -> Result<ConversationDto>

// 消息管理
send_message(conversation_id: String, content: String) -> Result<MessageDto>
list_messages(conversation_id: String) -> Result<Vec<MessageDto>>

// Capture 模式
set_capture_mode(enabled: bool) -> Result<()>
```

**文件位置**:
- `src/api.rs` - 添加新的 FFI 函数
- `src/storage.rs` - 添加相应的数据库操作
- 运行 `flutter_rust_bridge_codegen` 重新生成 Dart 绑定

### 2. 集成 Conversation Repository (高优先级)

**现状**: `ui/lib/providers/conversation_provider.dart` 使用 mock 数据

**任务**:
- 将 `ConversationRepository` 从 mock 改为调用 Rust bridge API
- 删除 mock 数据，使用真实的数据库查询
- 测试会话创建、列表、消息发送功能

**文件**: `ui/lib/providers/conversation_provider.dart:14-122`

### 3. 实现 Capture 模式切换 (中优先级)

**需求**: 主应用模式和 Capture 模式互斥显示

**任务**:
- 添加全局状态管理 Capture 模式开关
- 实现窗口尺寸和位置调整
- 添加平台特定的窗口控制 (置顶、聚焦)
- Capture 提交后自动切换回主应用模式

**涉及文件**:
- `ui/lib/providers/app_provider.dart` - 添加 capture mode state
- `ui/lib/screens/capture_screen.dart` - 优化为紧凑模式
- 平台层 (method channel) - 窗口控制

### 4. 添加 Conversation DTOs (中优先级)

**现状**: Rust 端缺少 Conversation 和 Message 的 DTO 定义

**任务**:
```rust
#[frb(dart_metadata=("freezed"))]
#[derive(Clone, Debug)]
pub struct ConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i32,
}

#[frb(dart_metadata=("freezed"))]
#[derive(Clone, Debug)]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub role: String,  // "user" or "assistant"
    pub content: String,
    pub created_at: String,
}
```

**文件**: `src/api.rs`

### 5. 数据库 Schema 扩展 (中优先级)

**需求**: 添加 conversations 和 messages 表

**SQL Schema**:
```sql
CREATE TABLE conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY(conversation_id) REFERENCES conversations(id)
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id, created_at);
```

**文件**: `src/storage.rs`

### 6. 集成测试 (低优先级)

**任务**:
- 添加 RustBridgeRepository 的集成测试
- 测试完整的事件记录 → AI 分析流程
- 测试会话创建和消息发送

**文件**: `ui/test/integration/`

## 推荐的实施顺序

1. **先做后端** (任务 4 → 5 → 1): 定义 DTOs → 扩展数据库 → 实现 Rust API
2. **再做前端** (任务 2): 集成 Conversation Repository 使用真实数据
3. **最后优化** (任务 3 → 6): Capture 模式切换 → 集成测试

## 快速开始命令

```bash
# 1. 检查当前 Rust 编译状态
cargo build

# 2. 运行 Flutter 应用
cd ui && fvm flutter run -d linux

# 3. 查看现有 API
grep "pub fn" src/api.rs

# 4. 重新生成 bridge 绑定 (在添加新 API 后)
flutter_rust_bridge_codegen

# 5. 运行所有测试
cargo test && cd ui && fvm flutter test
```

## 参考文档

- `docs/requirements/product/FR-PES-003-Flutter统一GUI.md` - GUI 需求
- `docs/STORAGE_ADAPTER.md` - Storage 架构文档
- `CLAUDE.md` - 项目开发指南
