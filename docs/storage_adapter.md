# Storage Adapter 设计

## 概述

为了支持未来切换不同的存储实现，引入了 `StorageAdapter` trait。当前使用 SQLite 实现，未来可以添加其他后端（如 PostgreSQL、远程 API 等）。

## 架构

```
┌─────────────────┐
│   api.rs (FFI)  │
└────────┬────────┘
         │ 使用
         ▼
┌─────────────────┐
│ StorageAdapter  │  <-- trait 定义
│   (trait)       │
└────────┬────────┘
         │ 实现
         ▼
┌─────────────────┐
│  Store (SQLite) │  <-- 当前实现
└─────────────────┘
```

## StorageAdapter Trait

定义在 `src/storage/adapter.rs`：

```rust
pub trait StorageAdapter {
    // 事件操作
    fn insert_event(&self, event: NewEvent) -> Result<String>;
    fn list_events(&self) -> Result<Vec<EventSummary>>;

    // 分析操作
    fn list_analyses(&self) -> Result<Vec<AnalysisSummary>>;
    fn claim_analysis_job(&self) -> Result<Option<AnalysisJob>>;
    fn complete_analysis(&self, job: &AnalysisJob, prompt_version: &str, result_json: &str) -> Result<()>;
    fn fail_analysis(&self, job: &AnalysisJob, error: &str) -> Result<()>;

    // AI 配置操作
    fn active_ai_provider_config(&self) -> Result<Option<AiProviderConfig>>;
    fn upsert_ai_provider_config(&self, base_url: &str, model: &str, api_key: &str) -> Result<()>;
}
```

## 当前实现：Store (SQLite)

`Store` struct 实现了 `StorageAdapter` trait：

```rust
impl StorageAdapter for Store {
    fn insert_event(&self, event: NewEvent) -> Result<String> {
        Store::insert_event(self, event)
    }
    // ... 其他方法
}
```

## 未来扩展

### 添加新的存储后端

1. 创建新的 struct（例如 `PostgresStore`）
2. 实现 `StorageAdapter` trait
3. 在初始化时根据配置选择实现

示例：

```rust
// src/storage/postgres.rs
pub struct PostgresStore {
    pool: sqlx::PgPool,
}

impl StorageAdapter for PostgresStore {
    fn insert_event(&self, event: NewEvent) -> Result<String> {
        // PostgreSQL 实现
    }
    // ...
}
```

### 配置切换

```rust
// 未来可以根据配置选择实现
let storage: Box<dyn StorageAdapter> = match config.storage_type {
    "sqlite" => Box::new(Store::open(&config.database_path)?),
    "postgres" => Box::new(PostgresStore::new(&config.postgres_url)?),
    _ => panic!("Unknown storage type"),
};
```

## 设计决策

### 为什么不要求 Send + Sync？

初始版本尝试了 `StorageAdapter: Send + Sync`，但遇到问题：

- `rusqlite::Connection` 包含 `RefCell`，不是 `Sync`
- SQLite 连接本身是单线程的
- 当前架构中每次操作都打开新连接，不需要跨线程共享

如果未来需要跨线程共享（如连接池），可以：
1. 使用 `Arc<Mutex<Store>>` 包装
2. 或实现支持并发的新 adapter

### 对象安全性

Trait 是对象安全的（object-safe），支持 `Box<dyn StorageAdapter>` 动态分发。

## 文件结构

```
src/
├── storage/
│   └── adapter.rs       # Trait 定义和相关类型
└── storage.rs           # Store 实现 + adapter 实现
```

## 使用示例

```rust
use crate::storage::{Store, StorageAdapter};

let store = Store::open(&db_path)?;
let adapter: &dyn StorageAdapter = &store;

// 通过 trait 调用
let events = adapter.list_events()?;
```
