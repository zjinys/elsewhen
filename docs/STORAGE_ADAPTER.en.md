# Storage Adapter Architecture

## 概述

存储访问通过 `StorageRepository` 接口进行，支持多种后端实现。

## 接口定义

```dart
abstract class StorageRepository {
  Future<void> initialize();
  Future<Event> recordEvent(String rawText);
  Future<List<Event>> listEvents();
  Future<List<Analysis>> listAnalyses();
  Future<String?> getAiProvider();
  Future<bool> triggerAnalysis();
}
```

## 实现

### 1. RustBridgeRepository (生产环境)
- 位置: `ui/lib/bridge/rust_bridge_repository.dart`
- 通过 Flutter Rust Bridge 调用 Rust 后端
- SQLite 存储
- 完整的 AI 分析功能

### 2. MockStorageRepository (测试环境)
- 位置: `ui/lib/data/mock_storage_repository.dart`
- 内存存储
- 用于单元测试和 Widget 测试

## 使用方式

### Provider 配置

```dart
// 生产环境
final storageRepositoryProvider = Provider<StorageRepository>((ref) {
  return RustBridgeRepository();
});

// 测试环境
final storageRepositoryProvider = Provider<StorageRepository>((ref) {
  return MockStorageRepository();
});
```

### 调用示例

```dart
final repo = ref.watch(storageRepositoryProvider);

// 初始化
await repo.initialize();

// 记录事件
final event = await repo.recordEvent('Meeting with team at 3pm');

// 列出事件
final events = await repo.listEvents();

// 获取分析结果
final analyses = await repo.listAnalyses();
```

## 添加新实现

1. 创建实现类，继承 `StorageRepository`
2. 实现所有接口方法
3. 在 Provider 中配置切换逻辑

```dart
class CustomStorageRepository implements StorageRepository {
  @override
  Future<void> initialize() async {
    // 自定义初始化逻辑
  }
  
  @override
  Future<Event> recordEvent(String rawText) async {
    // 自定义存储逻辑
  }
  
  // ... 实现其他方法
}
```

## 数据模型

### Event
- `id`: 唯一标识
- `rawText`: 原始文本
- `recordedAt`: 记录时间
- `source`: 来源（flutter_gui, cli 等）
- `status`: 状态（pending, completed 等）
- `analysis`: 关联的分析结果（可选）

### Analysis
- `eventType`: 事件类型
- `confidence`: 置信度 (0.0-1.0)
- `summary`: 摘要
- `clarifications`: 需要澄清的问题列表

## 测试

### 运行测试

```bash
cd ui/
fvm flutter test test/mock_storage_test.dart
```

测试覆盖：
- ✅ 初始化流程
- ✅ 事件记录和列表
- ✅ 分析触发
- ✅ 分析结果列表
- ✅ 数据清理
- ✅ 接口实现验证
- ✅ 自动初始化

## 当前状态

✅ 接口已定义
✅ Rust 实现已完成并测试
✅ Mock 实现已完成
✅ Provider 配置已完成
✅ 与 UI 集成完成
✅ 单元测试已编写 (7 tests passing)
