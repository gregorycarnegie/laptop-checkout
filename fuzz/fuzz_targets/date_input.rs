//! Any text typed into a date box either parses to a date that prints back
//! the same way, or is rejected. Run with `cargo +nightly fuzz run date_input`.
#![no_main]

use laptop_checkout::time;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    if let Some(ms) = time::from_input(text) {
        let back = time::to_input(ms);
        assert_eq!(time::from_input(&back), Some(ms), "{text:?} -> {back:?}");
    }
});
