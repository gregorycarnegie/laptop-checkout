//! Email templates and the email queue.

use super::*;

pub const PURPOSES: [(&str, &str); 4] = [
    ("overdue", "Overdue"),
    ("reminder", "Due-soon reminder"),
    ("receipt", "Check-out receipt"),
    ("general", "General"),
];

pub fn purpose_label(p: &str) -> &'static str {
    PURPOSES.iter().find(|(k, _)| *k == p).map_or("General", |(_, v)| *v)
}

/// The template to start an email with. For overdue emails the one chosen in
/// Settings wins; otherwise the oldest template for the purpose (the everyday
/// one, not the "final notice").
pub fn pick_template(templates: &[Template], purpose: &str, preferred: Option<i64>) -> Option<i64> {
    if purpose == "overdue" {
        if let Some(id) = preferred.filter(|id| templates.iter().any(|t| t.id == *id)) {
            return Some(id);
        }
    }
    templates.iter().filter(|t| t.purpose == purpose).min_by_key(|t| t.id).or_else(|| templates.first()).map(|t| t.id)
}

pub fn find_template(templates: &[Template], id: Option<i64>) -> Option<Template> {
    templates.iter().find(|t| Some(t.id) == id).cloned()
}

/// Whether the template editor has unsaved edits.
pub fn template_changed(saved: &Template, name: &str, subject: &str, body: &str, purpose: &str) -> bool {
    saved.name != name || saved.subject != subject || saved.body != body || saved.purpose != purpose
}

/// Short name of an email app for buttons.
pub fn short_app(k: &str) -> &'static str {
    match k {
        "outlook" => "Outlook (work)",
        "outlook_live" => "Outlook.com",
        "gmail" => "Gmail",
        _ => "mail app",
    }
}

/// Mail links open in a new tab, except `mailto:` which hands over to the mail app.
pub fn link_target(app: &str) -> &'static str {
    if app == "mailto" {
        "_self"
    } else {
        "_blank"
    }
}

/// Moves through an email queue without leaving it.
pub fn step_index(index: usize, len: usize, delta: isize) -> usize {
    let next = index as isize + delta;
    if next >= 0 && (next as usize) < len {
        next as usize
    } else {
        index
    }
}

/// "Email 2 of 5", or just "Email" for one message.
pub fn queue_label(index: usize, len: usize) -> String {
    if len > 1 {
        format!("Email {} of {len}", index + 1)
    } else {
        "Email".into()
    }
}

/// Inserts `{{key}}` into `text` over the selection `start..end` (UTF-16
/// offsets, as browsers report them). Returns the new text and caret offset.
pub fn insert_token(text: &str, start: usize, end: usize, key: &str) -> (String, usize) {
    let token = format!("{{{{{key}}}}}");
    let units: Vec<u16> = text.encode_utf16().collect();
    let start = start.min(units.len());
    let end = end.clamp(start, units.len());
    let out =
        format!("{}{}{}", String::from_utf16_lossy(&units[..start]), token, String::from_utf16_lossy(&units[end..]));
    (out, start + token.encode_utf16().count())
}

/// A believable loan for template previews when there's no real one.
pub fn sample_loan(now: i64) -> Loan {
    Loan {
        id: 0,
        laptop_id: 0,
        borrower_id: 0,
        out_at: now - 10 * DAY,
        due_at: time::end_of_day(now - 3 * DAY),
        returned_at: None,
        note: String::new(),
        last_notified_at: None,
        last_emailed_at: None,
        emails_sent: 0,
        renewals: 0,
        asset_tag: "LT-0103".into(),
        model: "Lenovo ThinkPad E14 Gen 5".into(),
        serial: "PF4A9K2M".into(),
        borrower_name: "Amara Okafor".into(),
        borrower_email: "amara.okafor@example.org".into(),
        department: "Year 11".into(),
    }
}
