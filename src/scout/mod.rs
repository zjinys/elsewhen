//! Scout 情报消费端：独立采集服务（scout-cf，CF Workers + D1）的本地镜像。
//!
//! - [`client`]：HTTP 客户端（网络纪律照 RelayClient：HTTPS、禁重定向、
//!   响应上限、出站代理策略）；
//! - [`store`]：本地镜像表 + 已读态 + 拉取设置。
//!
//! 服务端是权威数据源；本地表可清空重建。已读态与拉取水位归本地管。

pub(crate) mod analysis;
pub mod client;
mod research;
mod store;

#[cfg(test)]
mod client_tests;

pub use client::{ScoutClient, ScoutFinding, ScoutSource};
pub use store::{ScoutFindingRow, ScoutSettings};
