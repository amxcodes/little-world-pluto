// Keep the conversion safety suite independent of native shell test fixtures.
#![cfg(all(windows, feature = "product-app"))]
#[allow(dead_code)]
#[path = "../app/conversion.rs"]
mod conversion;
