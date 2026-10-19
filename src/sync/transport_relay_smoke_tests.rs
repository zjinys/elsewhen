//! Relay 冒烟测试（loopback 真实 Worker 互操作）。
//!
//! 原单文件 738 行拆分：传输层前段在 transport_relay_smoke/mod.rs，
//! 应用层往返在 transport_relay_smoke/roundtrip.rs（行为零变化）。

#[cfg(test)]
pub(crate) mod enrollment_ui;
#[cfg(test)]
mod joiner_ui;
#[cfg(test)]
mod membership;
#[cfg(test)]
mod owner_ui;

#[cfg(test)]
#[path = "transport_relay_smoke/mod.rs"]
mod smoke;
