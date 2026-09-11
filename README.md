# elsewhen

Personal Event System 的 Rust 本地核心。当前实现的是 Phase 1 的最小垂直切片：使用平台数据目录中的 SQLite 保存不可变原始事件。

## 运行

```bash
cargo run -- record "今天完成了 F429 的 USART 修改"
cargo run -- list
```

测试或便携运行时可设置 `PERSONALD_DATA_DIR=/path/to/data` 覆盖默认平台数据目录。

Capture 窗口、全局快捷键、AI 分析和规则引擎将在后续阶段接入；它们不能绕过 `Store::insert_event` 的本地提交边界。
