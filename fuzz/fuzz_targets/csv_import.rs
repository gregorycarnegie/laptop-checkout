//! Spreadsheet exports come from anywhere: no input may crash the importer.
//! Run with `cargo +nightly fuzz run csv_import`.
#![no_main]

use laptop_checkout::csv_import::{borrowers, laptops, mark_repeats, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let Ok(rows) = parse(text) else { return };
    let mut people = borrowers(&rows);
    mark_repeats(&mut people, |b| b.email.clone());
    for p in &people {
        // Rows without problems always have a name and a plausible email.
        if p.problem.is_none() {
            assert!(!p.record.name.is_empty());
            assert!(laptop_checkout::email::looks_like_email(&p.record.email));
        }
    }
    let mut kit = laptops(&rows);
    mark_repeats(&mut kit, |l| l.asset_tag.clone());
    assert!(kit.iter().all(|l| l.problem.is_some() || !l.record.asset_tag.is_empty()));
});
