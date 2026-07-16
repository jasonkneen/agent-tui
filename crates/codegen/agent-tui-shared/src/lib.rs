//! Shared utilities used by both `agent-tui-shell` and its downstream clients
//! (e.g. `agent-tui-pager-render`). This crate sits upstream of `agent-tui-shell`
//! so it must never depend on it.

pub mod clipboard;
pub mod placeholder_images;
pub mod session;
pub mod stderr;
pub mod ui_config;
