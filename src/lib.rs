//! Shared product logic for Island Desktop.
//!
//! Keep this package modular but small. Platform UI and OS adapters will be
//! added after the Phase 0 host spike selects their implementation.

pub mod latest;
pub mod motion;
pub mod shell_state;
pub mod visual;
