//! 多设备同步：设备管理、增量协议、三个传输（局域网 P2P / 盲中继 / 文件）。
//!
//! 设计文档：docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md
//!
//! 当前交付：盲中继客户端（transport_relay）+ 自托管 Worker（relay/）。
//! 局域网 P2P、扫码绑定、文件导出为后续迭代。

pub mod transport_relay;
mod transport_relay_smoke_tests;
