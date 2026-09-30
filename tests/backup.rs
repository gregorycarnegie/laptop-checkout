//! Backups are ordinary SQLite files: write one to disk and read it back.
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::fs;

use common::{seeded, TestResult};
use laptop_checkout::{db, repo, view_model as vm};
use pretty_assertions::assert_eq;
use rstest::rstest;

#[rstest]
fn a_backup_file_restores_every_record(_seeded: ()) -> TestResult {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join(vm::backup_name(common::now()));
    fs::write(&path, db::serialize()?).map_err(|e| e.to_string())?;

    let before = (repo::borrowers(), repo::laptops(), repo::all_loans(), repo::templates());
    repo::clear_records()?;
    assert!(repo::laptops().is_empty());

    db::replace(&fs::read(&path).map_err(|e| e.to_string())?)?;
    repo::migrate()?;
    assert_eq!((repo::borrowers(), repo::laptops(), repo::all_loans(), repo::templates()), before);
    dir.close().map_err(|e| e.to_string())
}

#[rstest]
fn a_backup_is_a_standard_sqlite_file(_seeded: ()) -> TestResult {
    let bytes = db::serialize()?;
    assert!(bytes.starts_with(b"SQLite format 3\0"));
    Ok(())
}

#[rstest]
#[case::a_spreadsheet(b"name,email\nAmara,a@b.org\n".as_slice())]
#[case::random_bytes(&[0xde, 0xad, 0xbe, 0xef, 0x00, 0x01][..])]
fn restoring_a_file_that_is_not_a_database_is_refused(_seeded: (), #[case] bytes: &[u8]) -> TestResult {
    let file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    fs::write(file.path(), bytes).map_err(|e| e.to_string())?;
    let err = db::replace(&fs::read(file.path()).map_err(|e| e.to_string())?).unwrap_err();
    assert_eq!(err, db::NOT_A_DATABASE);
    assert_eq!(repo::laptops().len(), 16, "the open database is untouched");
    Ok(())
}
