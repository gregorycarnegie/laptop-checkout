# Laptop Checkout

Check laptops in and out like library books. Built with [Leptos](https://leptos.dev)
and SQLite ([rusqlite](https://github.com/rusqlite/rusqlite)), compiled to WebAssembly
and served as a plain static site. **The database stays on your own PC** — nothing is
sent to a server.

## Features

- **Circulation desk** – check out and check in by scanning or typing an asset tag
  (USB barcode scanners work: they type the tag and press Enter). Pick a borrower by
  name, email or ID card number, choose a due date, add a note.
- **Overdue tracking** – library-style due-date stamps, "due today/tomorrow" and
  overdue lists, one-click **Renew**.
- **Browser notifications** – while the app is open in a tab it checks every minute
  and raises a desktop notification when a laptop becomes overdue (repeat interval is
  configurable). If notifications are blocked, alerts appear in the page.
- **One-click emails** – "Email all late borrowers" steps through each overdue loan
  with a filled-in template and opens it in your default mail app, Outlook on the web,
  Outlook.com or Gmail, ready to send. Every email is logged against the loan.
- **Email templates** – overdue notice, final notice, due-soon reminder and check-out
  receipt out of the box; edit them or add your own with `{{placeholders}}` and a live
  preview.
- **Borrowers** – add one at a time or **import a CSV** (flexible headers such as
  `First name` + `Surname`, `Email address`, `Form`, comma/semicolon/tab separated),
  with a preview that flags bad rows and duplicates before anything is saved.
- **Laptops** – inventory with status (in service / in repair / retired), CSV import,
  and loan history counts.

## Where the data lives

SQLite runs inside the WebAssembly bundle (rusqlite). After every change the
database is saved, from Rust, to:

1. the browser's storage (IndexedDB) on this PC, always; and
2. in Chrome or Edge, a real `.sqlite` file you choose (**Settings → Save to a new
   file…**). The browser asks for permission again after it restarts; click
   **Reconnect**.

You can also download a backup copy or restore one from **Settings**. The `.sqlite`
file opens in any SQLite tool (DB Browser for SQLite, `sqlite3`, …).

There is no hand-written JavaScript in the app: storage, the file pickers,
clipboard, downloads and notifications all go through `web-sys`/`wasm-bindgen`.

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
trunk serve --open        # dev server with live reload
trunk build --release     # static site in ./dist
```

Deploy `dist/` to any static host. `.github/workflows/pages.yml` publishes it to
GitHub Pages on every push to `main` (enable Pages → "GitHub Actions" first).

## Layout

| Path | What it does |
| --- | --- |
| `src/repo.rs` | All SQL: schema, queries, writes, sample data |
| `src/db.rs` | rusqlite connection, JSON-param queries, change reporting |
| `src/time.rs` | Calendar maths in pure Rust, with a controllable test clock |
| `src/view_model.rs` | Every decision the screens make (filters, pickers, messages) |
| `src/email.rs` | Default templates, placeholder rendering, compose links |
| `src/csv_import.rs`, `src/import.rs` | CSV parsing, header matching, import previews |
| `src/alerts.rs` | Which overdue alerts to raise (delivery is a trait, mocked in tests) |
| `src/persist.rs` | When to save, as a pure state machine |
| `src/web/` | Browser-only: IndexedDB, `.sqlite` files, clipboard, notifications |
| `src/ui/` | Leptos views: wiring and markup only |

## Testing

Business rules live in plain Rust modules so they can be tested natively and
fast; the browser layer and the UI are tested in real headless Chrome.

| Kind | Where | Run |
| --- | --- | --- |
| Unit tests (incl. `rstest` cases, table and macro-generated tests) | `#[cfg(test)] mod tests` in each module | `cargo nextest run` (or `cargo test`) |
| Property tests (`proptest`) | date maths, percent-encoding, CSV parser, token insertion | included above |
| Mocks (`mockall`) | the overdue-alert sink in `alerts.rs` | included above |
| Snapshot tests (`insta`) | rendered email templates in `src/snapshots/` | `cargo insta review` after changes |
| Integration tests | `tests/repo/*`, `tests/workflows.rs`, `tests/backup.rs` (shared set-up in `tests/common/`) | included above |
| Doc tests | examples in `time`, `db`, `email`, `persist` | `cargo test --doc` |
| Browser tests (`wasm-bindgen-test`) | `tests/browser_*.rs`, fakes in `tests/browser_support/` | `cargo test --target wasm32-unknown-unknown` |
| Fuzzing (`cargo-fuzz`) | `fuzz/fuzz_targets/` | `cd fuzz && cargo +nightly fuzz run csv_import` |
| Mutation testing (`cargo-mutants`) | whole crate | see below |

Browser tests need `wasm-bindgen-test-runner` (`cargo install wasm-bindgen-cli`
at the version in `Cargo.lock`) and a chromedriver that matches your Chrome, on
`PATH` or in `CHROMEDRIVER`. `webdriver.json` lets the headless browser show
notifications.

### Mutation testing

`cargo-mutants` changes the code in small ways (flips `<` to `<=`, deletes a `!`,
returns a default) and checks that some test fails for every change.

```sh
# Core logic, natively (about an hour on 4 cores):
cargo mutants --exclude 'src/ui/**' --exclude 'src/web/**' --exclude src/main.rs -j 4
# Browser code, in headless Chrome:
scripts/mutants-browser.sh -j 2
```

Code that no test can judge is marked `#[mutants::skip]` with the reason next to
it (only the entry point, console logging and the browser's clock/timezone
lookups). Remaining survivors are listed in [`MUTANTS.md`](MUTANTS.md) with why
each one can't change behaviour.
