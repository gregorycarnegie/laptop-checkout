//! Email templates and "compose" links.
//!
//! A static site can't send mail by itself, so the app fills in a template and
//! opens it in the user's own email app (default mail app, Outlook on the web
//! or Gmail) ready to send.

use crate::models::{Loan, Settings};
use crate::time;

pub struct DefaultTemplate {
    pub name: &'static str,
    pub subject: &'static str,
    pub body: &'static str,
    pub purpose: &'static str,
}

pub fn default_templates() -> Vec<DefaultTemplate> {
    vec![
        DefaultTemplate {
            name: "Overdue notice",
            purpose: "overdue",
            subject: "Overdue laptop {{asset_tag}}: please return it",
            body: "Hi {{first_name}},

Our records show that the laptop you borrowed is overdue.

  Laptop:   {{laptop}} ({{asset_tag}})
  Borrowed: {{checked_out}}
  Due:      {{due_date}} ({{days_late}} late)

Please bring it back to {{return_location}} as soon as you can. If you still need it, reply to this email and we can renew the loan.

Thanks,
{{sender_name}}
{{org_name}}",
        },
        DefaultTemplate {
            name: "Final notice",
            purpose: "overdue",
            subject: "Final notice: laptop {{asset_tag}} is {{days_late}} overdue",
            body: "Hi {{first_name}},

We have emailed you before about laptop {{asset_tag}} ({{laptop}}), which was due back on {{due_date}}. It is now {{days_late}} overdue.

Please return it to {{return_location}} by the end of tomorrow. If the laptop is lost or damaged, let us know straight away so we can help.

{{sender_name}}
{{org_name}}",
        },
        DefaultTemplate {
            name: "Due soon reminder",
            purpose: "reminder",
            subject: "Reminder: laptop {{asset_tag}} is due {{due_date}}",
            body: "Hi {{first_name}},

Just a reminder that the laptop you borrowed ({{laptop}}, {{asset_tag}}) is due back on {{due_date}}.

Please return it to {{return_location}}, or reply if you need to keep it a little longer.

Thanks,
{{sender_name}}
{{org_name}}",
        },
        DefaultTemplate {
            name: "Check-out receipt",
            purpose: "receipt",
            subject: "You've borrowed laptop {{asset_tag}}, due {{due_date}}",
            body: "Hi {{first_name}},

This confirms you borrowed a laptop from {{org_name}} today.

  Laptop:   {{laptop}}
  Asset tag: {{asset_tag}}
  Serial:   {{serial}}
  Due back: {{due_date}}

Please return it to {{return_location}} on or before the due date, with its charger.

Thanks,
{{sender_name}}",
        },
    ]
}

/// Placeholders shown as insertable chips in the template editor.
pub const PLACEHOLDERS: &[(&str, &str)] = &[
    ("first_name", "Borrower's first name"),
    ("name", "Borrower's full name"),
    ("email", "Borrower's email"),
    ("laptop", "Laptop make and model"),
    ("asset_tag", "Asset tag"),
    ("serial", "Serial number"),
    ("checked_out", "Date borrowed"),
    ("due_date", "Due date"),
    ("days_late", "\"3 days\" late"),
    ("return_location", "Where to return it"),
    ("sender_name", "Your name"),
    ("org_name", "Your organisation"),
    ("today", "Today's date"),
];

pub fn looks_like_email(s: &str) -> bool {
    let s = s.trim();
    match s.split_once('@') {
        Some((user, domain)) => {
            !user.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && !s.contains(char::is_whitespace)
        }
        None => false,
    }
}

pub fn first_name(full: &str) -> &str {
    full.split_whitespace().next().unwrap_or(full)
}

/// Replaces `{{placeholder}}`s with details of the loan.
pub fn render(text: &str, loan: &Loan, s: &Settings) -> String {
    let now = time::now();
    let late = time::days_late(loan.due_at, now);
    let values: [(&str, String); 13] = [
        ("first_name", first_name(&loan.borrower_name).to_string()),
        ("name", loan.borrower_name.clone()),
        ("email", loan.borrower_email.clone()),
        ("laptop", if loan.model.is_empty() { "Laptop".into() } else { loan.model.clone() }),
        ("asset_tag", loan.asset_tag.clone()),
        ("serial", if loan.serial.is_empty() { "n/a".into() } else { loan.serial.clone() }),
        ("checked_out", time::long(loan.out_at)),
        ("due_date", time::long(loan.due_at)),
        ("days_late", time::plural(late, "day")),
        ("return_location", s.return_location.clone()),
        ("sender_name", s.sender_name.clone()),
        ("org_name", s.org_name.clone()),
        ("today", time::long(now)),
    ];
    let mut out = text.to_string();
    for (key, value) in values {
        // Accept both {{key}} and {{ key }}.
        out = out.replace(&format!("{{{{{key}}}}}", key = key), &value);
        out = out.replace(&format!("{{{{ {key} }}}}", key = key), &value);
    }
    out
}

fn enc(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

/// Email apps the user can compose in.
pub const APPS: &[(&str, &str)] = &[
    ("mailto", "Default mail app"),
    ("outlook", "Outlook on the web (work or school)"),
    ("outlook_live", "Outlook.com (personal)"),
    ("gmail", "Gmail"),
];

/// A link that opens a ready-to-send message in the chosen email app.
pub fn compose_url(app: &str, to: &str, subject: &str, body: &str) -> String {
    let body = body.replace("\r\n", "\n").replace('\n', "\r\n");
    match app {
        "outlook" => format!(
            "https://outlook.office.com/mail/deeplink/compose?to={}&subject={}&body={}",
            enc(to),
            enc(subject),
            enc(&body)
        ),
        "outlook_live" => format!(
            "https://outlook.live.com/mail/0/deeplink/compose?to={}&subject={}&body={}",
            enc(to),
            enc(subject),
            enc(&body)
        ),
        "gmail" => format!(
            "https://mail.google.com/mail/?view=cm&fs=1&to={}&su={}&body={}",
            enc(to),
            enc(subject),
            enc(&body)
        ),
        _ => format!("mailto:{}?subject={}&body={}", enc(to), enc(subject), enc(&body)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_shapes() {
        assert!(looks_like_email("a.b@example.org"));
        assert!(!looks_like_email("a.b@example"));
        assert!(!looks_like_email("not an email"));
        assert!(!looks_like_email("@example.org"));
    }

    #[test]
    fn first_names() {
        assert_eq!(first_name("Amara Okafor"), "Amara");
        assert_eq!(first_name("Cher"), "Cher");
    }
}
