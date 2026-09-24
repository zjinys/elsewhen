mod frb_generated; /* AUTO INJECTED BY flutter_rust_bridge. This line may not be accurate, and you can change it according to your needs. */

// Core modules
pub mod ai;
pub mod config;
pub mod event;
pub mod fonts;
pub mod local_sources;
pub mod storage;
pub mod wiki;

// Bridge API module - exposes Rust core to Flutter
pub mod api;

// Tests
#[cfg(test)]
mod storage_tests;
