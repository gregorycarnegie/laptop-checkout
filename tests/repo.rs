//! Integration tests for all the SQL in `repo`, against a real SQLite
//! database in memory. Grouped by area in `tests/repo/`.
#![cfg(not(target_arch = "wasm32"))]

mod common;
mod repo {
    mod borrowers;
    mod data;
    mod email;
    mod laptops;
    mod loans;
    mod settings;
}
