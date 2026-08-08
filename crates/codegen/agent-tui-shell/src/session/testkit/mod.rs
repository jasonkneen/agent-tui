//! Session synthesis and in-process e2e harness for the load-perf and fork
//! bench tests.
//!
//! Lives in `agent-tui-shell` (feature `test-support`) rather than
//! `agent-tui-test-support` because synthesis drives the real
//! `JsonlStorageAdapter`; the reverse dependency would be circular.

pub mod e2e;
pub mod synth;
