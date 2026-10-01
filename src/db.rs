//! SQLite via rusqlite: the same code runs in the browser (WebAssembly) and
//! natively in tests.
//!
//! The database lives in memory while the app runs. Whoever embeds it (the web
//! layer, or a test) installs a [`set_change_hook`] to hear about writes, so it
//! can save the bytes and refresh the UI.
//!
//! Queries take ordinary rusqlite parameters and decode rows into any
//! `serde::Deserialize` type by column name, which keeps `repo.rs` compact:
//!
//! ```
//! use laptop_checkout::db;
//! use rusqlite::params;
//!
//! db::open_empty().unwrap();
//! db::run_script("CREATE TABLE t (name TEXT, n INTEGER)").unwrap();
//! db::exec("INSERT INTO t VALUES (?, ?)", params!["tag", 3]).unwrap();
//! assert_eq!(db::scalar("SELECT n FROM t WHERE name = ?", ["tag"]), 3);
//! ```

use std::{cell::RefCell, rc::Rc};

use rusqlite::{types::ValueRef, Connection, Params, MAIN_DB};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Number, Value};

/// What a write changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Data changed and needs saving; more writes are coming (inside a transaction).
    Pending,
    /// Data changed and is complete: save it and refresh the screen.
    Committed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecResult {
    pub changes: usize,
    pub last_id: i64,
}

type Hook = Rc<dyn Fn(Change)>;

thread_local! {
    static CONN: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

/// Installs the function told about every write.
pub fn set_change_hook(hook: impl Fn(Change) + 'static) {
    HOOK.with_borrow_mut(|h| *h = Some(Rc::new(hook)));
}

fn changed(change: Change) {
    // Clone out of the RefCell so the hook may itself use the database.
    if let Some(hook) = HOOK.with_borrow(Clone::clone) {
        hook(change);
    }
}

/// Tells listeners a batch of changes is complete (e.g. after a migration or
/// after opening a different database file).
pub fn announce_change() {
    changed(Change::Committed);
}

// ---------------------------------------------------------------- connection

pub const NOT_A_DATABASE: &str = "That file isn't a Laptop Checkout (SQLite) database.";

fn open(bytes: &[u8]) -> Result<Connection, String> {
    let mut conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    if !bytes.is_empty() {
        conn.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false).map_err(|e| e.to_string())?;
    }
    // Touch the schema so a file that isn't SQLite fails here, not later.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map_err(|_| NOT_A_DATABASE.to_string())?;
    // SQLite ignores REFERENCES unless this is on, per connection.
    conn.pragma_update(None, "foreign_keys", true).map_err(|e| e.to_string())?;
    Ok(conn)
}

/// Swaps in a database from its bytes (an empty slice gives a new database).
pub fn replace(bytes: &[u8]) -> Result<(), String> {
    let conn = open(bytes)?;
    CONN.with_borrow_mut(|c| *c = Some(conn));
    Ok(())
}

/// Starts with a new, empty database.
pub fn open_empty() -> Result<(), String> {
    replace(&[])
}

pub fn is_open() -> bool {
    CONN.with_borrow(Option::is_some)
}

fn with_conn<T>(f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    CONN.with_borrow(|c| match c {
        Some(conn) => f(conn),
        None => Err("The database isn't open yet.".into()),
    })
}

/// The whole database as bytes, in SQLite's file format.
pub fn serialize() -> Result<Vec<u8>, String> {
    with_conn(|c| c.serialize(MAIN_DB).map(|data| data.to_vec()).map_err(|e| e.to_string()))
}

// ---------------------------------------------------------------- values

fn to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Number::from_f64(f).map_or(Value::Null, Value::Number),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
    }
}

/// Rows as JSON objects keyed by column name.
pub fn rows(sql: &str, args: impl Params) -> Result<Vec<Value>, String> {
    with_conn(|c| {
        let mut stmt = c.prepare_cached(sql).map_err(|e| e.to_string())?;
        let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
        let mut out = Vec::new();
        let mut rows = stmt.query(args).map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut obj = Map::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                let v = row.get_ref(i).map_err(|e| e.to_string())?;
                obj.insert(name.clone(), to_json(v));
            }
            out.push(Value::Object(obj));
        }
        Ok(out)
    })
}

#[mutants::skip] // Only writes to the browser console.
fn log(msg: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(msg));
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{msg}");
}

/// A broken query or a row that doesn't fit its struct is a bug. Debug builds
/// (and so every test) panic on it; release builds log it and carry on, so the
/// page shows an empty list instead of breaking.
fn bug(msg: &str) {
    log(msg);
    debug_assert!(false, "{msg}");
}

/// Runs a query and decodes each row. Before a database is open it gives no rows.
pub fn query<T: DeserializeOwned>(sql: &str, args: impl Params) -> Vec<T> {
    match rows(sql, args) {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|r| {
                serde_json::from_value(r).map_err(|e| bug(&format!("Couldn't read a row ({e}) for: {sql}"))).ok()
            })
            .collect(),
        Err(e) if is_open() => {
            bug(&format!("{e} in: {sql}"));
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

pub fn query_one<T: DeserializeOwned>(sql: &str, args: impl Params) -> Option<T> {
    query(sql, args).into_iter().next()
}

/// A single integer from a column named `n` (0 if there's no row or it's NULL).
pub fn scalar(sql: &str, args: impl Params) -> i64 {
    #[derive(Deserialize)]
    struct N {
        n: Option<i64>,
    }
    query_one::<N>(&format!("SELECT ({sql}) AS n"), args).and_then(|r| r.n).unwrap_or(0)
}

/// Runs a write without refreshing the screen. Use inside `transaction`.
/// A write that changed no rows asks for nothing.
pub fn exec_quiet(sql: &str, args: impl Params) -> Result<ExecResult, String> {
    let r = with_conn(|c| {
        let mut stmt = c.prepare_cached(sql).map_err(|e| e.to_string())?;
        let changes = stmt.execute(args).map_err(|e| e.to_string())?;
        Ok(ExecResult { changes, last_id: c.last_insert_rowid() })
    })?;
    if r.changes > 0 {
        changed(Change::Pending);
    }
    Ok(r)
}

pub fn exec(sql: &str, args: impl Params) -> Result<ExecResult, String> {
    let r = exec_quiet(sql, args)?;
    if r.changes > 0 {
        changed(Change::Committed);
    }
    Ok(r)
}

fn batch(sql: &str) -> Result<(), String> {
    with_conn(|c| c.execute_batch(sql).map_err(|e| e.to_string()))
}

/// Runs several statements with no parameters.
pub fn run_script(sql: &str) -> Result<(), String> {
    batch(sql)?;
    changed(Change::Pending);
    Ok(())
}

/// Runs `f` inside BEGIN/COMMIT, rolling back if it returns an error.
pub fn transaction<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    batch("BEGIN")?;
    match f() {
        Ok(v) => {
            batch("COMMIT")?;
            changed(Change::Committed);
            Ok(v)
        }
        Err(e) => {
            batch("ROLLBACK")?;
            Err(e)
        }
    }
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::test_support::record_changes;
    use pretty_assertions::assert_eq;
    use rstest::rstest;
    use rusqlite::params;
    use serde_json::json;

    fn table() -> Result<(), String> {
        open_empty()?;
        run_script("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, n INTEGER, x REAL)")
    }

    #[derive(Debug, PartialEq, Deserialize)]
    struct Row {
        id: i64,
        name: String,
    }

    // ------------------------------------------------------------ values

    #[rstest]
    #[case::null(ValueRef::Null, json!(null))]
    #[case::blob(ValueRef::Blob(b"x"), json!(null))]
    #[case::integer(ValueRef::Integer(7), json!(7))]
    #[case::real(ValueRef::Real(2.5), json!(2.5))]
    #[case::not_a_number(ValueRef::Real(f64::NAN), json!(null))]
    #[case::text(ValueRef::Text(b"hi"), json!("hi"))]
    fn sql_values_become_json(#[case] v: ValueRef<'static>, #[case] expected: Value) {
        assert_eq!(to_json(v), expected);
    }

    // ------------------------------------------------------------ queries

    #[test]
    fn rows_are_keyed_by_column_name() -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name, n, x) VALUES ('a', 1, 0.5)", [])?;
        assert_eq!(rows("SELECT name, n, x FROM t", [])?, vec![json!({"name": "a", "n": 1, "x": 0.5})]);
        Ok(())
    }

    #[test]
    fn query_decodes_rows_into_structs() -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name) VALUES (?), (?)", params!["a", "b"])?;
        let got: Vec<Row> = query("SELECT id, name FROM t ORDER BY id", []);
        assert_eq!(got, vec![Row { id: 1, name: "a".into() }, Row { id: 2, name: "b".into() }]);
        Ok(())
    }

    #[test]
    #[should_panic(expected = "Couldn't read a row")]
    fn a_row_that_does_not_fit_the_struct_is_a_bug() {
        table().unwrap();
        exec("INSERT INTO t (name) VALUES (NULL)", []).unwrap();
        let _: Vec<Row> = query("SELECT id, name FROM t", []);
    }

    #[test]
    #[should_panic(expected = "no such table: missing")]
    fn a_bad_query_is_a_bug() {
        table().unwrap();
        let _: Vec<Row> = query("SELECT nope FROM missing", []);
    }

    #[test]
    fn foreign_keys_are_enforced() -> Result<(), String> {
        open_empty()?;
        run_script("CREATE TABLE p (id INTEGER PRIMARY KEY); CREATE TABLE c (p INTEGER REFERENCES p (id))")?;
        let err = exec("INSERT INTO c VALUES (1)", []).unwrap_err();
        assert!(err.contains("FOREIGN KEY"), "{err}");
        Ok(())
    }

    #[test]
    fn query_one_takes_the_first_row() -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name) VALUES ('a'), ('b')", [])?;
        let first: Option<Row> = query_one("SELECT id, name FROM t ORDER BY id DESC", []);
        assert_eq!(first.map(|r| r.name), Some("b".into()));
        Ok(())
    }

    #[rstest]
    #[case::a_value("SELECT n FROM t WHERE name = 'a'", 3)]
    #[case::no_rows("SELECT n FROM t WHERE name = 'zzz'", 0)]
    #[case::null("SELECT NULL", 0)]
    #[case::count("SELECT count(*) FROM t", 1)]
    fn scalar_reads_one_number(#[case] sql: &str, #[case] n: i64) -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name, n) VALUES ('a', 3)", [])?;
        assert_eq!(scalar(sql, []), n);
        Ok(())
    }

    #[test]
    fn nothing_works_before_a_database_is_open() {
        assert!(!is_open());
        assert_eq!(exec("SELECT 1", []).unwrap_err(), "The database isn't open yet.");
        assert_eq!(scalar("SELECT 1", []), 0);
        assert_eq!(serialize().unwrap_err(), "The database isn't open yet.");
    }

    // ------------------------------------------------------------ writes and change reports

    #[test]
    fn exec_reports_changed_rows_and_the_new_id() -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name) VALUES ('a')", [])?;
        let r = exec("INSERT INTO t (name) VALUES ('b')", [])?;
        assert_eq!(r, ExecResult { changes: 1, last_id: 2 });
        assert_eq!(exec("UPDATE t SET n = 1", [])?.changes, 2);
        Ok(())
    }

    #[test]
    fn exec_asks_for_a_save_and_a_refresh() -> Result<(), String> {
        table()?;
        let log = record_changes();
        exec("INSERT INTO t (name) VALUES ('a')", [])?;
        assert_eq!(*log.borrow(), [Change::Pending, Change::Committed]);
        Ok(())
    }

    #[test]
    fn exec_quiet_asks_only_for_a_save() -> Result<(), String> {
        table()?;
        let log = record_changes();
        exec_quiet("INSERT INTO t (name) VALUES ('a')", [])?;
        run_script("UPDATE t SET n = 2")?;
        assert_eq!(*log.borrow(), [Change::Pending, Change::Pending]);
        Ok(())
    }

    #[test]
    fn a_failed_write_reports_no_change() -> Result<(), String> {
        table()?;
        let log = record_changes();
        assert!(exec("INSERT INTO missing VALUES (1)", []).is_err());
        assert!(run_script("NOT SQL").is_err());
        assert!(log.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn a_write_that_changes_nothing_reports_no_change() -> Result<(), String> {
        table()?;
        let log = record_changes();
        assert_eq!(exec("UPDATE t SET n = 1", [])?.changes, 0);
        assert_eq!(exec_quiet("DELETE FROM t", [])?.changes, 0);
        assert!(log.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn announce_change_asks_for_a_refresh() {
        let log = record_changes();
        announce_change();
        assert_eq!(*log.borrow(), [Change::Committed]);
    }

    #[test]
    fn the_change_hook_may_use_the_database() -> Result<(), String> {
        table()?;
        set_change_hook(|_| {
            let _ = scalar("SELECT count(*) FROM t", []);
        });
        exec("INSERT INTO t (name) VALUES ('a')", [])?;
        Ok(())
    }

    // ------------------------------------------------------------ transactions

    #[test]
    fn a_transaction_commits_and_refreshes_once() -> Result<(), String> {
        table()?;
        let log = record_changes();
        let n = transaction(|| {
            exec_quiet("INSERT INTO t (name) VALUES ('a')", [])?;
            exec_quiet("INSERT INTO t (name) VALUES ('b')", [])?;
            Ok(2)
        })?;
        assert_eq!(n, 2);
        assert_eq!(scalar("SELECT count(*) FROM t", []), 2);
        assert_eq!(*log.borrow(), [Change::Pending, Change::Pending, Change::Committed]);
        Ok(())
    }

    #[test]
    fn a_failing_transaction_rolls_everything_back() -> Result<(), String> {
        table()?;
        let log = record_changes();
        let r: Result<(), String> = transaction(|| {
            exec_quiet("INSERT INTO t (name) VALUES ('a')", [])?;
            Err("stop".into())
        });
        assert_eq!(r, Err("stop".into()));
        assert_eq!(scalar("SELECT count(*) FROM t", []), 0);
        assert!(!log.borrow().contains(&Change::Committed));
        Ok(())
    }

    #[test]
    fn a_transaction_cannot_start_inside_another() -> Result<(), String> {
        table()?;
        let r = transaction(|| transaction(|| Ok(())));
        assert!(r.is_err());
        // The outer transaction was rolled back, so a new one can start.
        transaction(|| Ok(()))
    }

    // ------------------------------------------------------------ bytes

    #[test]
    fn a_database_survives_serialize_and_replace() -> Result<(), String> {
        table()?;
        exec("INSERT INTO t (name) VALUES ('kept')", [])?;
        let bytes = serialize()?;
        open_empty()?;
        replace(&bytes)?;
        let got: Vec<Row> = query("SELECT id, name FROM t", []);
        assert_eq!(got, vec![Row { id: 1, name: "kept".into() }]);
        Ok(())
    }

    #[test]
    fn replace_rejects_bytes_that_are_not_sqlite() -> Result<(), String> {
        table()?;
        assert_eq!(replace(b"name,email\nAmara,a@b.org\n"), Err(NOT_A_DATABASE.into()));
        // The open database is left alone.
        assert_eq!(scalar("SELECT count(*) FROM t", []), 0);
        Ok(())
    }

    #[test]
    fn an_empty_file_opens_as_a_new_database() -> Result<(), String> {
        replace(&[])?;
        assert!(is_open());
        assert_eq!(scalar("SELECT count(*) FROM sqlite_master", []), 0);
        Ok(())
    }
}
