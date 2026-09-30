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

SQLite runs inside the WebAssembly bundle. After every change the database is saved:

1. to the browser's storage (IndexedDB) on this PC, always; and
2. in Chrome or Edge, to a real `.sqlite` file you choose (**Settings → Save to a new
   file…**). The browser asks for permission again after it restarts; click
   **Reconnect**.

You can also download a backup copy or restore one from **Settings**. The `.sqlite`
file opens in any SQLite tool (DB Browser for SQLite, `sqlite3`, …).

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
trunk serve --open        # dev server with live reload
trunk build --release     # static site in ./dist
```

Deploy `dist/` to any static host. A GitHub Actions workflow
(`.github/workflows/pages.yml`) builds and publishes it to GitHub Pages on every push
to `main` (enable Pages → "GitHub Actions" in the repo settings).

## Layout

| Path | What it does |
| --- | --- |
| `src/repo.rs` | All SQL: schema, queries, writes, sample data |
| `src/db.rs` | rusqlite connection, JSON-param queries, autosave |
| `js/storage.js` | Tiny shim for IndexedDB and the File System Access API |
| `src/pages/` | Desk, Loans, Laptops, Borrowers, Email templates, Settings |
| `src/email.rs` | Default templates, placeholder rendering, compose links |
| `src/csv_import.rs` | CSV parsing and header matching |
| `src/notify.rs` | Overdue browser notifications |
| `style/main.css` | Styles (light and dark) |
