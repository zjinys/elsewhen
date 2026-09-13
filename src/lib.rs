// Bridge API module - exposes Rust core to Flutter
pub mod api;

// Re-export core modules for bridge usage
pub use crate::config;
pub use crate::event;
pub use crate::storage;
