//! 多设备同步的协议、认证加密及中继传输底座。
//!
//! 设计文档：docs/notes/proposed/architecture/2026-09-30-multi-device-sync.md
//! 当前有内部复制、配对原语、签名成员授权、密钥库、快照/增量投递与 relay v2；
//! App 配对/解锁、方向通道协调与生命周期仍以路线图未完成项为准，
//! 不开放未验证的生产同步入口。

pub mod crypto;
pub(crate) mod datasets;
pub(crate) mod engine;
pub(crate) mod enrollment;
pub(crate) mod fragments;
#[cfg(test)]
pub(crate) mod mem_blob;
pub(crate) mod membership;
pub(crate) mod pairing;
pub mod protocol;
pub(crate) mod runtime;
pub mod transport_relay;
mod transport_relay_smoke_tests;
pub(crate) mod vault;
