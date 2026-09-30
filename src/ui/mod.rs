//! Leptos views. They hold no business rules of their own: decisions live in
//! `view_model`, data in `repo`, so these files are wiring and markup.

pub mod app;
pub mod components;
pub mod pages;
pub mod state;

/// Starts the app: removes the "Opening…" placeholder and mounts the UI.
pub fn start() {
    if let Some(el) = leptos::prelude::document().get_element_by_id("boot") {
        el.remove();
    }
    leptos::mount::mount_to_body(app::App);
}
