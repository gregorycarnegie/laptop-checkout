//! Browser-only code: saving the database on the PC, notifications, clipboard.
//! It compiles natively (so `cargo test` builds the whole crate) but only runs
//! in a browser, where the `wasm-bindgen-test` suite in `tests/browser.rs` and
//! the end-to-end tests exercise it.

pub mod notify;
pub mod persistence;
pub mod storage;
