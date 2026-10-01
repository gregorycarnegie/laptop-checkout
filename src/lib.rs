//! Laptop Library: a library-style loan desk for laptops.
//!
//! Leptos (client-side rendered) + SQLite via rusqlite, compiled to
//! WebAssembly and served as a static site. The database stays on the user's PC.
//!
//! The crate is split so that almost everything can be tested natively:
//!
//! * Pure logic: [`time`], [`email`], [`csv_import`], [`persist`], [`alerts`],
//!   [`view_model`], [`import`].
//! * SQLite: [`db`] (connection) and [`repo`] (all the SQL), tested against an
//!   in-memory database.
//! * Browser-only: [`web`] (storage on the PC, notifications) and [`ui`] (Leptos
//!   views), tested in headless Chrome.

pub mod alerts;
pub mod csv_import;
pub mod db;
pub mod email;
pub mod import;
pub mod models;
pub mod persist;
pub mod repo;
pub mod time;
pub mod ui;
pub mod view_model;
pub mod web;

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod test_support;
