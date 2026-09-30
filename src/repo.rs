//! All SQL for the app: schema, queries and writes.

use serde::Deserialize;
use serde_json::json;

use crate::db::{self, exec, exec_quiet, query, query_one, scalar, transaction};
use crate::email;
use crate::models::*;
use crate::time::{self, DAY, HOUR};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS borrowers (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    email       TEXT    NOT NULL DEFAULT '',
    department  TEXT    NOT NULL DEFAULT '',
    external_id TEXT    NOT NULL DEFAULT '',
    phone       TEXT    NOT NULL DEFAULT '',
    notes       TEXT    NOT NULL DEFAULT '',
    active      INTEGER NOT NULL DEFAULT 1,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS borrowers_email ON borrowers (lower(email));

CREATE TABLE IF NOT EXISTS laptops (
    id         INTEGER PRIMARY KEY,
    asset_tag  TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    model      TEXT    NOT NULL DEFAULT '',
    serial     TEXT    NOT NULL DEFAULT '',
    notes      TEXT    NOT NULL DEFAULT '',
    status     TEXT    NOT NULL DEFAULT 'available'
               CHECK (status IN ('available', 'repair', 'retired')),
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS loans (
    id               INTEGER PRIMARY KEY,
    laptop_id        INTEGER NOT NULL REFERENCES laptops (id),
    borrower_id      INTEGER NOT NULL REFERENCES borrowers (id),
    out_at           INTEGER NOT NULL,
    due_at           INTEGER NOT NULL,
    returned_at      INTEGER,
    note             TEXT    NOT NULL DEFAULT '',
    last_notified_at INTEGER,
    last_emailed_at  INTEGER,
    emails_sent      INTEGER NOT NULL DEFAULT 0,
    renewals         INTEGER NOT NULL DEFAULT 0
);
-- A laptop can only be out to one person at a time.
CREATE UNIQUE INDEX IF NOT EXISTS loans_one_open_per_laptop
    ON loans (laptop_id) WHERE returned_at IS NULL;
CREATE INDEX IF NOT EXISTS loans_borrower ON loans (borrower_id);

CREATE TABLE IF NOT EXISTS email_templates (
    id      INTEGER PRIMARY KEY,
    name    TEXT NOT NULL,
    subject TEXT NOT NULL,
    body    TEXT NOT NULL,
    purpose TEXT NOT NULL DEFAULT 'general'
);

CREATE TABLE IF NOT EXISTS email_log (
    id          INTEGER PRIMARY KEY,
    loan_id     INTEGER REFERENCES loans (id),
    borrower_id INTEGER REFERENCES borrowers (id),
    to_addr     TEXT    NOT NULL,
    subject     TEXT    NOT NULL,
    template    TEXT    NOT NULL DEFAULT '',
    sent_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const LOAN_SELECT: &str = r#"
SELECT l.id, l.laptop_id, l.borrower_id, l.out_at, l.due_at, l.returned_at, l.note,
       l.last_notified_at, l.last_emailed_at, l.emails_sent, l.renewals,
       lp.asset_tag, lp.model, lp.serial,
       b.name AS borrower_name, b.email AS borrower_email, b.department
  FROM loans l
  JOIN laptops lp ON lp.id = l.laptop_id
  JOIN borrowers b ON b.id = l.borrower_id
"#;

pub fn migrate() -> Result<(), String> {
    db::run_script(SCHEMA)?;
    if scalar("SELECT count(*) AS n FROM email_templates", json!([])) == 0 {
        for t in email::default_templates() {
            exec_quiet(
                "INSERT INTO email_templates (name, subject, body, purpose) VALUES (?, ?, ?, ?)",
                json!([t.name, t.subject, t.body, t.purpose]),
            )?;
        }
    }
    db::announce_change();
    Ok(())
}

// ---------------------------------------------------------------- settings

#[derive(Deserialize)]
struct Kv {
    key: String,
    value: String,
}

pub fn settings() -> Settings {
    let rows: Vec<Kv> = query("SELECT key, value FROM settings", json!([]));
    let get = |k: &str| rows.iter().find(|r| r.key == k).map(|r| r.value.clone());
    Settings {
        org_name: get("org_name").unwrap_or_else(|| "IT Services".into()),
        sender_name: get("sender_name").unwrap_or_else(|| "The IT Desk".into()),
        return_location: get("return_location").unwrap_or_else(|| "the IT desk".into()),
        loan_days: get("loan_days").and_then(|v| v.parse().ok()).unwrap_or(7),
        email_app: get("email_app").unwrap_or_else(|| "mailto".into()),
        notify_enabled: get("notify_enabled").map(|v| v == "1").unwrap_or(true),
        renotify_hours: get("renotify_hours").and_then(|v| v.parse().ok()).unwrap_or(24),
        late_template: get("late_template").and_then(|v| v.parse().ok()),
        sample_data: get("sample_data").map(|v| v == "1").unwrap_or(false),
    }
}

pub fn set_setting(key: &str, value: &str) -> Result<(), String> {
    exec(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        json!([key, value]),
    )
    .map(|_| ())
}

// ---------------------------------------------------------------- borrowers

pub fn borrowers() -> Vec<Borrower> {
    query(
        r#"SELECT b.*,
                  (SELECT count(*) FROM loans l WHERE l.borrower_id = b.id AND l.returned_at IS NULL) AS open_loans,
                  (SELECT count(*) FROM loans l WHERE l.borrower_id = b.id AND l.returned_at IS NULL AND l.due_at < ?) AS late_loans,
                  (SELECT count(*) FROM loans l WHERE l.borrower_id = b.id) AS total_loans
             FROM borrowers b
            ORDER BY b.active DESC, b.name COLLATE NOCASE"#,
        json!([time::now()]),
    )
}

pub fn borrower(id: i64) -> Option<Borrower> {
    borrowers().into_iter().find(|b| b.id == id)
}

fn clean_borrower(b: &BorrowerInput) -> Result<BorrowerInput, String> {
    let out = BorrowerInput {
        name: b.name.trim().to_string(),
        email: b.email.trim().to_string(),
        department: b.department.trim().to_string(),
        external_id: b.external_id.trim().to_string(),
        phone: b.phone.trim().to_string(),
        notes: b.notes.trim().to_string(),
    };
    if out.name.is_empty() {
        return Err("Enter the borrower's name.".into());
    }
    if !out.email.is_empty() && !email::looks_like_email(&out.email) {
        return Err(format!("\"{}\" doesn't look like an email address.", out.email));
    }
    Ok(out)
}

pub fn find_borrower_by_email(email: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Id {
        id: i64,
    }
    if email.trim().is_empty() {
        return None;
    }
    query_one::<Id>("SELECT id FROM borrowers WHERE lower(email) = lower(?) LIMIT 1", json!([email.trim()]))
        .map(|r| r.id)
}

pub fn add_borrower(b: &BorrowerInput) -> Result<i64, String> {
    let b = clean_borrower(b)?;
    if find_borrower_by_email(&b.email).is_some() {
        return Err(format!("A borrower with {} already exists.", b.email));
    }
    exec(
        "INSERT INTO borrowers (name, email, department, external_id, phone, notes, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        json!([b.name, b.email, b.department, b.external_id, b.phone, b.notes, time::now()]),
    )
    .map(|r| r.last_id)
}

fn write_borrower(id: i64, b: &BorrowerInput) -> Result<(), String> {
    exec_quiet(
        "UPDATE borrowers SET name = ?, email = ?, department = ?, external_id = ?, phone = ?, notes = ?
         WHERE id = ?",
        json!([b.name, b.email, b.department, b.external_id, b.phone, b.notes, id]),
    )
    .map(|_| ())
}

pub fn update_borrower(id: i64, b: &BorrowerInput) -> Result<(), String> {
    let b = clean_borrower(b)?;
    if let Some(other) = find_borrower_by_email(&b.email) {
        if other != id {
            return Err(format!("Another borrower already uses {}.", b.email));
        }
    }
    write_borrower(id, &b)?;
    db::announce_change();
    Ok(())
}

pub fn set_borrower_active(id: i64, active: bool) -> Result<(), String> {
    exec("UPDATE borrowers SET active = ? WHERE id = ?", json!([active as i64, id])).map(|_| ())
}

pub fn delete_borrower(id: i64) -> Result<(), String> {
    if scalar("SELECT count(*) AS n FROM loans WHERE borrower_id = ?", json!([id])) > 0 {
        return Err("This borrower has loan history, so they can only be deactivated.".into());
    }
    exec("DELETE FROM borrowers WHERE id = ?", json!([id])).map(|_| ())
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

/// Imports borrowers in one transaction. Rows matching an existing email are
/// updated when `update_existing` is set, otherwise skipped.
pub fn import_borrowers(rows: &[BorrowerInput], update_existing: bool) -> Result<ImportResult, String> {
    transaction(|| {
        let mut r = ImportResult::default();
        let now = time::now();
        for row in rows {
            let Ok(b) = clean_borrower(row) else {
                r.skipped += 1;
                continue;
            };
            match find_borrower_by_email(&b.email) {
                Some(id) if update_existing => {
                    write_borrower(id, &b)?;
                    r.updated += 1;
                }
                Some(_) => r.skipped += 1,
                None => {
                    exec_quiet(
                        "INSERT INTO borrowers (name, email, department, external_id, phone, notes, created_at)
                         VALUES (?, ?, ?, ?, ?, ?, ?)",
                        json!([b.name, b.email, b.department, b.external_id, b.phone, b.notes, now]),
                    )?;
                    r.added += 1;
                }
            }
        }
        Ok(r)
    })
}

// ---------------------------------------------------------------- laptops

pub fn laptops() -> Vec<Laptop> {
    query(
        r#"SELECT lp.id, lp.asset_tag, lp.model, lp.serial, lp.notes, lp.status,
                  l.id AS loan_id, l.due_at, b.name AS borrower_name,
                  (SELECT count(*) FROM loans x WHERE x.laptop_id = lp.id) AS total_loans
             FROM laptops lp
             LEFT JOIN loans l ON l.laptop_id = lp.id AND l.returned_at IS NULL
             LEFT JOIN borrowers b ON b.id = l.borrower_id
            ORDER BY lp.asset_tag COLLATE NOCASE"#,
        json!([]),
    )
}

pub fn laptop(id: i64) -> Option<Laptop> {
    laptops().into_iter().find(|l| l.id == id)
}

fn clean_laptop(l: &LaptopInput) -> Result<LaptopInput, String> {
    let out = LaptopInput {
        asset_tag: l.asset_tag.trim().to_string(),
        model: l.model.trim().to_string(),
        serial: l.serial.trim().to_string(),
        notes: l.notes.trim().to_string(),
    };
    if out.asset_tag.is_empty() {
        return Err("Enter the laptop's asset tag.".into());
    }
    Ok(out)
}

pub fn find_laptop_by_tag(tag: &str) -> Option<i64> {
    #[derive(Deserialize)]
    struct Id {
        id: i64,
    }
    query_one::<Id>("SELECT id FROM laptops WHERE asset_tag = ? COLLATE NOCASE", json!([tag.trim()])).map(|r| r.id)
}

pub fn add_laptop(l: &LaptopInput) -> Result<i64, String> {
    let l = clean_laptop(l)?;
    if find_laptop_by_tag(&l.asset_tag).is_some() {
        return Err(format!("Asset tag {} is already in use.", l.asset_tag));
    }
    exec(
        "INSERT INTO laptops (asset_tag, model, serial, notes, created_at) VALUES (?, ?, ?, ?, ?)",
        json!([l.asset_tag, l.model, l.serial, l.notes, time::now()]),
    )
    .map(|r| r.last_id)
}

pub fn update_laptop(id: i64, l: &LaptopInput) -> Result<(), String> {
    let l = clean_laptop(l)?;
    if let Some(other) = find_laptop_by_tag(&l.asset_tag) {
        if other != id {
            return Err(format!("Asset tag {} is already in use.", l.asset_tag));
        }
    }
    exec(
        "UPDATE laptops SET asset_tag = ?, model = ?, serial = ?, notes = ? WHERE id = ?",
        json!([l.asset_tag, l.model, l.serial, l.notes, id]),
    )
    .map(|_| ())
}

pub fn set_laptop_status(id: i64, status: &str) -> Result<(), String> {
    exec("UPDATE laptops SET status = ? WHERE id = ?", json!([status, id])).map(|_| ())
}

pub fn delete_laptop(id: i64) -> Result<(), String> {
    if scalar("SELECT count(*) AS n FROM loans WHERE laptop_id = ?", json!([id])) > 0 {
        return Err("This laptop has loan history, so it can only be retired.".into());
    }
    exec("DELETE FROM laptops WHERE id = ?", json!([id])).map(|_| ())
}

pub fn import_laptops(rows: &[LaptopInput], update_existing: bool) -> Result<ImportResult, String> {
    transaction(|| {
        let mut r = ImportResult::default();
        let now = time::now();
        for row in rows {
            let Ok(l) = clean_laptop(row) else {
                r.skipped += 1;
                continue;
            };
            match find_laptop_by_tag(&l.asset_tag) {
                Some(id) if update_existing => {
                    exec_quiet(
                        "UPDATE laptops SET model = ?, serial = ?, notes = ? WHERE id = ?",
                        json!([l.model, l.serial, l.notes, id]),
                    )?;
                    r.updated += 1;
                }
                Some(_) => r.skipped += 1,
                None => {
                    exec_quiet(
                        "INSERT INTO laptops (asset_tag, model, serial, notes, created_at) VALUES (?, ?, ?, ?, ?)",
                        json!([l.asset_tag, l.model, l.serial, l.notes, now]),
                    )?;
                    r.added += 1;
                }
            }
        }
        Ok(r)
    })
}

// ---------------------------------------------------------------- loans

pub fn open_loans() -> Vec<Loan> {
    query(&format!("{LOAN_SELECT} WHERE l.returned_at IS NULL ORDER BY l.due_at"), json!([]))
}

pub fn all_loans() -> Vec<Loan> {
    query(
        &format!("{LOAN_SELECT} ORDER BY (l.returned_at IS NULL) DESC, coalesce(l.returned_at, l.due_at) DESC"),
        json!([]),
    )
}

pub fn loan(id: i64) -> Option<Loan> {
    query_one(&format!("{LOAN_SELECT} WHERE l.id = ?"), json!([id]))
}

pub fn check_out(laptop_id: i64, borrower_id: i64, due_at: i64, note: &str) -> Result<i64, String> {
    #[derive(Deserialize)]
    struct State {
        asset_tag: String,
        status: String,
        open: i64,
    }
    let state: State = query_one(
        "SELECT asset_tag, status,
                (SELECT count(*) FROM loans WHERE laptop_id = laptops.id AND returned_at IS NULL) AS open
           FROM laptops WHERE id = ?",
        json!([laptop_id]),
    )
    .ok_or("That laptop no longer exists.")?;
    if state.open > 0 {
        return Err(format!("{} is already checked out. Check it in first.", state.asset_tag));
    }
    match state.status.as_str() {
        "repair" => return Err(format!("{} is marked as in repair.", state.asset_tag)),
        "retired" => return Err(format!("{} is retired.", state.asset_tag)),
        _ => {}
    }
    let now = time::now();
    if due_at <= now {
        return Err("Pick a due date in the future.".into());
    }
    exec(
        "INSERT INTO loans (laptop_id, borrower_id, out_at, due_at, note) VALUES (?, ?, ?, ?, ?)",
        json!([laptop_id, borrower_id, now, due_at, note.trim()]),
    )
    .map(|r| r.last_id)
}

pub fn check_in(loan_id: i64, note: &str) -> Result<Loan, String> {
    let note = note.trim();
    exec(
        "UPDATE loans
            SET returned_at = ?,
                note = CASE WHEN ? = '' THEN note
                            WHEN note = '' THEN 'Returned: ' || ?
                            ELSE note || char(10) || 'Returned: ' || ? END
          WHERE id = ? AND returned_at IS NULL",
        json!([time::now(), note, note, note, loan_id]),
    )?;
    loan(loan_id).ok_or_else(|| "That loan no longer exists.".into())
}

/// Library-style renewal: the new due date counts from today or the old due
/// date, whichever is later.
pub fn renew(loan_id: i64, days: i64) -> Result<i64, String> {
    let l = loan(loan_id).ok_or("That loan no longer exists.")?;
    let base = l.due_at.max(time::now());
    let due = time::end_of_day_after(base, days);
    exec(
        "UPDATE loans SET due_at = ?, renewals = renewals + 1, last_notified_at = NULL WHERE id = ?",
        json!([due, loan_id]),
    )?;
    Ok(due)
}

/// Overdue loans that haven't triggered a browser notification within `every_ms`.
pub fn loans_needing_alert(now: i64, every_hours: i64) -> Vec<Loan> {
    query(
        &format!(
            "{LOAN_SELECT} WHERE l.returned_at IS NULL AND l.due_at < ?
               AND (l.last_notified_at IS NULL OR l.last_notified_at < ?)
             ORDER BY l.due_at"
        ),
        json!([now, now - every_hours.max(1) * HOUR]),
    )
}

pub fn mark_notified(ids: &[i64], now: i64) {
    let _ = transaction(|| {
        for id in ids {
            exec_quiet("UPDATE loans SET last_notified_at = ? WHERE id = ?", json!([now, id]))?;
        }
        Ok(())
    });
}

// ---------------------------------------------------------------- email

pub fn templates() -> Vec<Template> {
    query(
        "SELECT id, name, subject, body, purpose FROM email_templates
          ORDER BY CASE purpose WHEN 'overdue' THEN 0 WHEN 'reminder' THEN 1 WHEN 'receipt' THEN 2 ELSE 3 END,
                   name COLLATE NOCASE",
        json!([]),
    )
}

pub fn save_template(id: Option<i64>, name: &str, subject: &str, body: &str, purpose: &str) -> Result<i64, String> {
    if name.trim().is_empty() {
        return Err("Give the template a name.".into());
    }
    if subject.trim().is_empty() {
        return Err("Add a subject line.".into());
    }
    match id {
        Some(id) => exec(
            "UPDATE email_templates SET name = ?, subject = ?, body = ?, purpose = ? WHERE id = ?",
            json!([name.trim(), subject, body, purpose, id]),
        )
        .map(|_| id),
        None => exec(
            "INSERT INTO email_templates (name, subject, body, purpose) VALUES (?, ?, ?, ?)",
            json!([name.trim(), subject, body, purpose]),
        )
        .map(|r| r.last_id),
    }
}

pub fn delete_template(id: i64) -> Result<(), String> {
    exec("DELETE FROM email_templates WHERE id = ?", json!([id])).map(|_| ())
}

pub fn restore_default_templates() -> Result<usize, String> {
    let existing: Vec<String> = templates().into_iter().map(|t| t.name).collect();
    let missing: Vec<_> =
        email::default_templates().into_iter().filter(|t| !existing.iter().any(|n| n == t.name)).collect();
    let n = missing.len();
    transaction(|| {
        for t in missing {
            exec_quiet(
                "INSERT INTO email_templates (name, subject, body, purpose) VALUES (?, ?, ?, ?)",
                json!([t.name, t.subject, t.body, t.purpose]),
            )?;
        }
        Ok(n)
    })
}

pub fn log_email(loan: &Loan, to: &str, subject: &str, template: &str) -> Result<(), String> {
    let now = time::now();
    transaction(|| {
        exec_quiet(
            "INSERT INTO email_log (loan_id, borrower_id, to_addr, subject, template, sent_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            json!([loan.id, loan.borrower_id, to, subject, template, now]),
        )?;
        exec_quiet(
            "UPDATE loans SET last_emailed_at = ?, emails_sent = emails_sent + 1 WHERE id = ?",
            json!([now, loan.id]),
        )?;
        Ok(())
    })
}

pub fn email_log(limit: i64) -> Vec<EmailLogEntry> {
    query(
        "SELECT e.id, e.to_addr, e.subject, e.template, e.sent_at,
                b.name AS borrower_name, lp.asset_tag
           FROM email_log e
           LEFT JOIN borrowers b ON b.id = e.borrower_id
           LEFT JOIN loans l ON l.id = e.loan_id
           LEFT JOIN laptops lp ON lp.id = l.laptop_id
          ORDER BY e.sent_at DESC LIMIT ?",
        json!([limit]),
    )
}

// ---------------------------------------------------------------- data management

/// Removes every borrower, laptop and loan. Templates and settings stay.
pub fn clear_records() -> Result<(), String> {
    transaction(|| {
        db::run_script(
            "DELETE FROM email_log; DELETE FROM loans; DELETE FROM laptops; DELETE FROM borrowers;
             DELETE FROM settings WHERE key = 'sample_data';",
        )?;
        Ok(())
    })
}

pub fn seed_sample() -> Result<(), String> {
    const PEOPLE: &[(&str, &str, &str, &str)] = &[
        ("Amara Okafor", "amara.okafor@example.org", "Year 11", "S20931"),
        ("Liam Chen", "liam.chen@example.org", "Year 10", "S21077"),
        ("Priya Natarajan", "p.natarajan@example.org", "Science", "T0412"),
        ("Tomás Álvarez", "tomas.alvarez@example.org", "Sixth Form", "S19854"),
        ("Grace Whitfield", "g.whitfield@example.org", "English", "T0388"),
        ("Yusuf Rahman", "yusuf.rahman@example.org", "Year 9", "S22310"),
        ("Hannah Kowalski", "hannah.kowalski@example.org", "Sixth Form", "S19902"),
        ("Oliver Bennett", "o.bennett@example.org", "Admin", "T0520"),
        ("Zara Mensah", "zara.mensah@example.org", "Year 12", "S20115"),
        ("Ethan Murphy", "ethan.murphy@example.org", "Year 10", "S21140"),
        ("Sofia Rossi", "s.rossi@example.org", "Maths", "T0401"),
        ("Noah Fitzgerald", "noah.fitzgerald@example.org", "Year 11", "S20977"),
        ("Aisha Begum", "aisha.begum@example.org", "Library", "T0455"),
        ("Jack Thornton", "jack.thornton@example.org", "Year 13", "S19633"),
    ];
    const LAPTOPS: &[(&str, &str, &str, &str)] = &[
        ("LT-0101", "Dell Latitude 3440", "7HQ2ZK3", "available"),
        ("LT-0102", "Dell Latitude 3440", "7HQ5XB3", "available"),
        ("LT-0103", "Lenovo ThinkPad E14 Gen 5", "PF4A9K2M", "available"),
        ("LT-0104", "Lenovo ThinkPad E14 Gen 5", "PF4A9K7Q", "available"),
        ("LT-0105", "HP ProBook 440 G10", "5CD3127XJN", "available"),
        ("LT-0106", "HP ProBook 440 G10", "5CD3127XKQ", "repair"),
        ("LT-0107", "Apple MacBook Air 13\" M2", "C02HM4KQ1WFV", "available"),
        ("LT-0108", "Apple MacBook Air 13\" M2", "C02HM4KR2WFV", "available"),
        ("LT-0109", "Acer Chromebook Spin 514", "NXK6JEK00123", "available"),
        ("LT-0110", "Acer Chromebook Spin 514", "NXK6JEK00157", "available"),
        ("LT-0111", "Dell Latitude 5440", "9JR1MV3", "available"),
        ("LT-0112", "Dell Latitude 5440", "9JR4NV3", "available"),
        ("LT-0113", "Microsoft Surface Laptop Go 3", "0F21AB34567", "available"),
        ("LT-0114", "Microsoft Surface Laptop Go 3", "0F21AB34612", "available"),
        ("LT-0115", "HP ProBook 440 G8", "5CD1044PLM", "retired"),
        ("LT-0116", "Lenovo ThinkPad E14 Gen 5", "PF4A9L0C", "available"),
    ];
    /// A loan in the example data, with days counted from today.
    struct SampleLoan {
        laptop: usize,
        borrower: usize,
        out_days_ago: i64,
        /// Negative for a loan that fell due in the past.
        due_in_days: i64,
        returned_days_ago: Option<i64>,
        emails: i64,
    }
    const LOANS: &[SampleLoan] = &[
        SampleLoan { laptop: 2, borrower: 0, out_days_ago: 12, due_in_days: -5, returned_days_ago: None, emails: 1 },
        SampleLoan { laptop: 6, borrower: 3, out_days_ago: 9, due_in_days: -2, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 11, borrower: 6, out_days_ago: 8, due_in_days: -1, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 0, borrower: 1, out_days_ago: 2, due_in_days: 0, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 3, borrower: 8, out_days_ago: 1, due_in_days: 1, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 8, borrower: 4, out_days_ago: 1, due_in_days: 6, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 9, borrower: 10, out_days_ago: 0, due_in_days: 13, returned_days_ago: None, emails: 0 },
        SampleLoan { laptop: 12, borrower: 13, out_days_ago: 3, due_in_days: 4, returned_days_ago: None, emails: 0 },
        SampleLoan {
            laptop: 1,
            borrower: 5,
            out_days_ago: 20,
            due_in_days: -13,
            returned_days_ago: Some(12),
            emails: 0,
        },
        SampleLoan {
            laptop: 4,
            borrower: 2,
            out_days_ago: 16,
            due_in_days: -9,
            returned_days_ago: Some(10),
            emails: 0,
        },
        SampleLoan {
            laptop: 7,
            borrower: 9,
            out_days_ago: 30,
            due_in_days: -23,
            returned_days_ago: Some(18),
            emails: 2,
        },
        SampleLoan {
            laptop: 13,
            borrower: 11,
            out_days_ago: 14,
            due_in_days: -7,
            returned_days_ago: Some(7),
            emails: 0,
        },
        SampleLoan {
            laptop: 0,
            borrower: 12,
            out_days_ago: 25,
            due_in_days: -18,
            returned_days_ago: Some(19),
            emails: 0,
        },
    ];
    let now = time::now();
    transaction(|| {
        let mut people = Vec::new();
        for (name, email, dept, ext) in PEOPLE {
            let r = exec_quiet(
                "INSERT INTO borrowers (name, email, department, external_id, created_at) VALUES (?, ?, ?, ?, ?)",
                json!([name, email, dept, ext, now - 40 * DAY]),
            )?;
            people.push(r.last_id);
        }
        let mut machines = Vec::new();
        for (tag, model, serial, status) in LAPTOPS {
            let r = exec_quiet(
                "INSERT INTO laptops (asset_tag, model, serial, status, created_at) VALUES (?, ?, ?, ?, ?)",
                json!([tag, model, serial, status, now - 60 * DAY]),
            )?;
            machines.push(r.last_id);
        }
        for &SampleLoan {
            laptop: lap,
            borrower: who,
            out_days_ago: out_ago,
            due_in_days: due_in,
            returned_days_ago: returned_ago,
            emails,
        } in LOANS
        {
            let out = now - out_ago * DAY - 2 * HOUR;
            let due = time::end_of_day_after(now, due_in);
            let returned = returned_ago.map(|d| now - d * DAY);
            let emailed = (emails > 0).then(|| now - 2 * DAY);
            exec_quiet(
                "INSERT INTO loans (laptop_id, borrower_id, out_at, due_at, returned_at, last_emailed_at, emails_sent)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                json!([machines[lap], people[who], out, due, returned, emailed, emails]),
            )?;
        }
        exec_quiet("INSERT OR REPLACE INTO settings (key, value) VALUES ('sample_data', '1')", json!([]))?;
        Ok(())
    })
}
