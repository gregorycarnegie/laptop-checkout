//! Laptop Checkout: a library-style loan desk for laptops.
//!
//! Leptos (client-side rendered) + SQLite via rusqlite, compiled to
//! WebAssembly and served as a static site. The database stays on the user's PC.

mod app;
mod components;
mod csv_import;
mod db;
mod email;
mod models;
mod notify;
mod pages;
mod repo;
mod state;
mod time;

fn main() {
    console_error_panic_hook::set_once();
    if let Some(el) = leptos::prelude::document().get_element_by_id("boot") {
        el.remove();
    }
    leptos::mount::mount_to_body(app::App);
}
