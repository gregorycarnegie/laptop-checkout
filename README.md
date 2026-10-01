# Laptop Library

[![Tests](https://github.com/gregorycarnegie/laptop-library/actions/workflows/tests.yml/badge.svg)](https://github.com/gregorycarnegie/laptop-library/actions/workflows/tests.yml)
[![GitHub Pages](https://github.com/gregorycarnegie/laptop-library/actions/workflows/pages.yml/badge.svg)](https://github.com/gregorycarnegie/laptop-library/actions/workflows/pages.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built_with-Rust-dea584.svg)](https://www.rust-lang.org/)

Manage laptop loans, borrowers and inventory in your browser, with barcode scanning,
due dates, overdue reminders and CSV imports. Built for a simple circulation desk,
with local SQLite storage and no backend to set up.

[Open the app](https://gregorycarnegie.github.io/laptop-library/) ·
[Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md)

The app uses [Leptos](https://leptos.dev)
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

## Getting started

1. **Open the app** in Chrome or Edge on the PC at your desk. There's nothing to
   install.
2. **Remove the example data.** The first time it opens, the app fills itself with
   example borrowers, laptops and loans so you can try it out. When you're ready,
   go to **Settings → Start fresh → Remove the example data**. Your templates and
   settings stay.
3. **Fill in Settings**: organisation name, sender name, where laptops are
   returned, the default loan length, and which email app to use (default mail
   app, Outlook on the web, Outlook.com or Gmail).
4. **Save to a file**: **Settings → Save to a new file…**, somewhere that's backed
   up. Without it, the data lives only in this browser's storage (see
   [Where the data lives](#where-the-data-lives)). Download a backup now and then
   too.
5. **Import your laptops and borrowers** from CSV (see below).
6. **Lend and return** from the Desk. Scan or type an asset tag (USB barcode
   scanners work), pick the borrower, choose a due date and check it out. Checking
   in works the same way.

## Importing from CSV

Export from Excel, your MIS or your HR system as CSV. Commas, semicolons and tabs
all work. Header names are matched loosely: case, spaces and punctuation are
ignored, so `Email Address`, `email_address` and `EMAIL` are the same column.
Before anything is saved, a preview flags bad rows and duplicates, and rows with
problems are skipped.

### Laptops

Only the asset tag is required.

```csv
asset_tag,model,serial,notes
LT-0201,Dell Latitude 3440,7HQ9ZK3,
LT-0202,Dell Latitude 3440,7HQ9ZK4,Cracked lid
```

| Column | Also accepted |
| --- | --- |
| `asset_tag` | `asset`, `tag`, `asset number`, `asset id`, `device name`, `hostname`, `computer name`, `name` |
| `model` | `make`, `make model`, `description`, `device`, `type` |
| `serial` | `serial number`, `serial no`, `SN`, `service tag` |
| `notes` | `note`, `comments`, `comment` |

The asset tag is what you scan at the desk, so use whatever is printed on the
barcode label.

### Borrowers

A name and an email address are required. Rows without a valid email are skipped;
add those people one at a time instead, where email is optional.

```csv
name,email,department,id,phone,notes
Amara Okafor,amara.okafor@school.org,Year 11,S20931,,
Grace Whitfield,g.whitfield@school.org,English,T0388,07700 900123,
```

| Column | Also accepted |
| --- | --- |
| `name` | `full name`, `display name`, `student name`, `staff name`, `pupil name`, `borrower`; or `first name` (`forename`, `given name`, `preferred name`) and `last name` (`surname`, `family name`), which are joined |
| `email` | `email address`, `mail`, `work email`, `school email`, `UPN`, `user principal name` |
| `department` | `dept`, `form`, `class`, `year`, `year group`, `tutor group`, `homeroom`, `team`, `group`, `course` |
| `id` | `student id`, `staff id`, `employee id`, `id number`, `card number`, `library card`, `barcode`, `admission number` |
| `phone` | `mobile`, `telephone`, `tel`, `phone number` |
| `notes` | `note`, `comments`, `comment` |

Fill in `id` if you want to find people by ID card number at the desk.

### Re-importing

Tick **Update … already here** on the import screen to update existing records
instead of skipping them. Laptops are matched by asset tag and borrowers by email,
so you can re-import a fresh export each term. A file without a header row is read
in the column order of the examples above, but a header row is safer.

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

Install Rust 1.88 or newer and LLVM/Clang (needed to compile SQLite for WebAssembly).
Make sure `clang` is on `PATH`; on Windows, the usual LLVM install directory is
`C:\Program Files\LLVM\bin`. Then run:

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
| `src/db.rs` | rusqlite connection, queries decoded by column name, change reporting |
| `src/time.rs` | Calendar maths in pure Rust, with a controllable test clock |
| `src/view_model/` | Every decision the screens make (filters, pickers, messages), one file per page |
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
