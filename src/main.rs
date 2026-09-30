#[mutants::skip] // The entry point only; `ui::start` is what the browser tests run.
fn main() {
    console_error_panic_hook::set_once();
    laptop_checkout::ui::start();
}
