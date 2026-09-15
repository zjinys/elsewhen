# 时间线 + 标签系统设计

## 概述

实现了基于时间线的对话组织系统，结合标签功能进行分类管理。

## 核心功能

### 1. 时间线分组

对话按更新时间自动分组：
- **今天** - 今日更新的对话
- **昨天** - 昨日更新的对话  
- **本周** - 7天内的对话
- **本月** - 30天内的对话
- **年月** - 更早的对话按"YYYY年M月"分组

```dart
Map<String, List<Conversation>> _groupByDate(List<Conversation> conversations) {
  final grouped = <String, List<Conversation>>{};
  final now = DateTime.now();

  for (final conv in conversations) {
    String key;
    final diff = now.difference(conv.updatedAt).inDays;

    if (diff == 0) {
      key = '今天';
    } else if (diff == 1) {
      key = '昨天';
    } else if (diff < 7) {
      key = '本周';
    } else if (diff < 30) {
      key = '本月';
    } else {
      key = '${conv.updatedAt.year}年${conv.updatedAt.month}月';
    }

    grouped.putIfAbsent(key, () => []).add(conv);
  }

  return grouped;
}
```

### 2. 标签系统

#### 数据模型
```rust
// Rust side - src/storage.rs
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,  // 新增字段
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub last_message_preview: Option<String>,
}
```

```dart
// Flutter side - lib/models/conversation.dart
class Conversation {
  final String id;
  final String? title;
  final String? tag;  // 新增字段
  final DateTime createdAt;
  final DateTime updatedAt;
  final int messageCount;
  final String? lastMessagePreview;
}
```

#### 预定义标签
```dart
const _predefinedTags = ['工作', '学习', '生活', '创意', '其他'];
```

可扩展设计：
- 预定义标签用于快速选择
- 支持自定义标签（未来可扩展）
- 标签存储为字符串，灵活性高

### 3. 标签过滤

右上角过滤按钮：
- 显示所有可用标签
- 点击标签过滤对话列表
- "全部对话" 选项清除过滤

```dart
PopupMenuButton<String?>(
  icon: const Icon(Icons.filter_list),
  onSelected: (tag) {
    setState(() {
      _selectedTag = tag;
    });
  },
  itemBuilder: (context) => [
    const PopupMenuItem(value: null, child: Text('全部对话')),
    const PopupMenuDivider(),
    ..._availableTags.map((tag) => PopupMenuItem(
      value: tag,
      child: Row(
        children: [
          _TagChip(tag: tag, small: true),
          const SizedBox(width: 8),
          Text(tag),
        ],
      ),
    )),
  ],
)
```

### 4. 对话卡片设计

每个对话卡片显示：
- **标题** - 用户设定的标题或"新对话"
- **标签** - 带颜色编码的小标签
- **预览** - 最后一条消息的前两行
- **统计** - 消息数量
- **时间** - 相对时间（N分钟前/小时前）或日期

```dart
Card(
  child: Padding(
    child: Column(
      children: [
        Row(
          children: [
            Expanded(child: Text(conversation.displayTitle)),
            if (conversation.tag != null)
              _TagChip(tag: conversation.tag!),
          ],
        ),
        Text(conversation.lastMessagePreview),
        Row(
          children: [
            Icon(Icons.message),
            Text('${conversation.messageCount} 条消息'),
            Spacer(),
            Text(_formatTime(conversation.updatedAt)),
          ],
        ),
      ],
    ),
  ),
)
```

### 5. 标签颜色编码

不同标签使用不同颜色，便于视觉识别：

```dart
Color _getTagColor(String tag) {
  switch (tag) {
    case '工作': return Colors.blue;
    case '学习': return Colors.green;
    case '生活': return Colors.orange;
    case '创意': return Colors.purple;
    case '其他': return Colors.grey;
    default: return Colors.teal;
  }
}
```

## 数据库变更

### Schema 更新

```sql
-- 添加 tag 列到 conversations 表
ALTER TABLE conversations ADD COLUMN tag TEXT;
```

### 迁移逻辑

在 `Store::open()` 中自动执行：

```rust
// Check and add tag column if missing
let has_tag = conn.query_row(
    "SELECT COUNT(*) FROM pragma_table_info('conversations') WHERE name='tag'",
    [],
    |row| row.get::<_, i64>(0),
)?;

if has_tag == 0 {
    conn.execute("ALTER TABLE conversations ADD COLUMN tag TEXT", [])?;
}
```

### API 更新

```rust
// src/api.rs
pub fn create_conversation(title: Option<String>, tag: Option<String>) -> Result<ConversationDto>

pub struct ConversationDto {
    pub id: String,
    pub title: Option<String>,
    pub tag: Option<String>,  // 新增
    pub created_at: String,
    pub updated_at: String,
    pub message_count: i64,
    pub last_message_preview: Option<String>,
}
```

## UI 组件

### ConversationTimelineScreen

主要界面，包含：
- AppBar with filter button
- FutureBuilder for loading conversations
- ListView with grouped timeline sections
- FloatingActionButton for creating new conversations

### _TimelineSection

时间分组的标题和对话列表：
```dart
Column(
  children: [
    Text(date), // "今天", "昨天", etc.
    ...conversations.map((conv) => _ConversationCard(conversation: conv)),
  ],
)
```

### _ConversationCard

单个对话的卡片展示

### _TagChip

标签显示组件，支持两种尺寸：
- `small: true` - 用于下拉菜单
- `small: false` - 用于卡片展示

## 使用流程

### 创建对话

1. 点击右下角"新建对话"按钮
2. 输入标题（可选）
3. 选择标签（可选）
4. 点击"创建"

```dart
void _showCreateDialog() {
  showDialog(
    builder: (context) => AlertDialog(
      content: Column(
        children: [
          TextField(controller: titleController),
          DropdownButtonFormField<String?>(
            items: _predefinedTags,
            onChanged: (value) => selectedTag = value,
          ),
        ],
      ),
      actions: [
        FilledButton(
          onPressed: () async {
            await repo.createConversation(
              title: title,
              tag: selectedTag,
            );
          },
        ),
      ],
    ),
  );
}
```

### 浏览对话

1. 对话按时间分组自动展示
2. 点击右上角过滤按钮选择标签
3. 查看特定标签下的对话
4. 点击"全部对话"清除过滤

### 对话详情

点击对话卡片进入详情页（待实现）

## 扩展建议

### 1. 自定义标签输入

```dart
// 在创建对话对话框中添加
TextField(
  decoration: InputDecoration(
    labelText: '或输入自定义标签',
  ),
  onChanged: (value) {
    if (value.isNotEmpty) {
      customTag = value;
    }
  },
)
```

### 2. 标签管理

创建专门的标签管理界面：
- 查看所有使用过的标签
- 重命名标签
- 合并标签
- 删除未使用的标签

### 3. 多标签支持

将 `tag: String?` 改为 `tags: List<String>`：

```rust
// 数据库层面可以用 JSON
tags TEXT, -- JSON array: ["工作", "重要"]

// 或者创建关联表
CREATE TABLE conversation_tags (
    conversation_id TEXT,
    tag TEXT,
    PRIMARY KEY (conversation_id, tag)
);
```

### 4. 智能标签推荐

基于对话内容自动推荐标签：
- 分析对话主题
- AI 生成标签建议
- 用户确认或修改

### 5. 标签统计

显示每个标签下的对话数量：
```dart
'工作 (12)'  // 12 条对话
'学习 (5)'
```

### 6. 标签颜色自定义

允许用户为标签选择自定义颜色

### 7. 搜索功能

同时搜索标题、内容和标签：
```dart
List<Conversation> search(String query) {
  return conversations.where((conv) =>
    conv.title?.contains(query) == true ||
    conv.tag?.contains(query) == true ||
    conv.lastMessagePreview?.contains(query) == true
  ).toList();
}
```

### 8. 归档功能

添加 `archived` 字段：
```rust
pub struct ConversationSummary {
    // ...
    pub archived: bool,
}
```

归档的对话不在主时间线显示，但可以通过"归档"标签访问。

## 技术要点

### 1. 状态管理

使用 Riverpod 的 `storageRepositoryProvider`：
```dart
final repo = ref.read(storageRepositoryProvider) as RustBridgeRepository;
```

### 2. 时间格式化

相对时间显示：
```dart
String _formatTime(DateTime time) {
  final diff = now.difference(time);
  if (diff.inMinutes < 60) return '${diff.inMinutes} 分钟前';
  if (diff.inHours < 24) return '${diff.inHours} 小时前';
  return '${time.month}/${time.day}';
}
```

### 3. 响应式更新

创建/更新对话后调用 `setState()` 刷新列表：
```dart
await repo.createConversation(title: title, tag: selectedTag);
if (context.mounted) {
  Navigator.pop(context);
  setState(() {}); // Refresh
}
```

## 测试

### Rust 测试

所有测试已通过：
```bash
$ cargo test --lib
test result: ok. 24 passed; 0 failed; 0 ignored
```

### Flutter 测试 (待添加)

```dart
testWidgets('Timeline groups conversations by date', (tester) async {
  // Mock conversations
  // Verify grouping
});

testWidgets('Tag filter works correctly', (tester) async {
  // Select tag
  // Verify filtered list
});
```

## 总结

时间线 + 标签系统提供了：
- ✅ 直观的时间组织
- ✅ 灵活的标签分类
- ✅ 快速的过滤功能
- ✅ 清晰的视觉层级
- ✅ 可扩展的架构

这套设计平衡了简单性和功能性，为用户提供了有效的对话管理工具。
