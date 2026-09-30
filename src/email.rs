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

  Laptop:    {{laptop}}
  Asset tag: {{asset_tag}}
  Serial:    {{serial}}
  Due back:  {{due_date}}

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

/// Percent-encodes like JavaScript's `encodeURIComponent`: everything except
/// `A–Z a–z 0–9 - _ . ! ~ * ' ( )` becomes `%XX` UTF-8 bytes.
///
/// ```
/// use laptop_checkout::email::encode_component;
/// assert_eq!(encode_component("a b&c@d.é"), "a%20b%26c%40d.%C3%A9");
/// ```
pub fn encode_component(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn enc(s: &str) -> String {
    encode_component(s)
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
        "gmail" => {
            format!("https://mail.google.com/mail/?view=cm&fs=1&to={}&su={}&body={}", enc(to), enc(subject), enc(&body))
        }
        _ => format!("mailto:{}?subject={}&body={}", enc(to), enc(subject), enc(&body)),
    }
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::time::{clock, days_from_civil, end_of_day, DAY, HOUR};
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use rstest::{fixture, rstest};

    fn settings() -> Settings {
        Settings {
            org_name: "Hillside Academy".into(),
            sender_name: "Ms Patel, IT".into(),
            return_location: "the library desk".into(),
            loan_days: 7,
            email_app: "mailto".into(),
            notify_enabled: true,
            renotify_hours: 24,
            late_template: None,
            sample_data: false,
        }
    }

    /// Tuesday 6 October 2026, 10:00 UTC; the loan was due on Friday 2 October.
    #[fixture]
    fn loan() -> Loan {
        let now = days_from_civil(2026, 10, 6) * DAY + 10 * HOUR;
        clock::set_now(now);
        Loan {
            id: 7,
            laptop_id: 3,
            borrower_id: 1,
            out_at: days_from_civil(2026, 9, 25) * DAY + 9 * HOUR,
            due_at: end_of_day(days_from_civil(2026, 10, 2) * DAY),
            returned_at: None,
            note: String::new(),
            last_notified_at: None,
            last_emailed_at: None,
            emails_sent: 0,
            renewals: 0,
            asset_tag: "LT-0103".into(),
            model: "Lenovo ThinkPad E14".into(),
            serial: "PF4A9K2M".into(),
            borrower_name: "Amara Okafor".into(),
            borrower_email: "amara@example.org".into(),
            department: "Year 11".into(),
        }
    }

    // ------------------------------------------------------------ addresses

    #[rstest]
    #[case("a.b@example.org", true)]
    #[case("  padded@example.org  ", true)]
    #[case("a.b@example", false)]
    #[case("not an email", false)]
    #[case("@example.org", false)]
    #[case("user@.org", false)]
    #[case("user@example.", false)]
    #[case("two words@example.org", false)]
    fn looks_like_email_accepts_only_plausible_addresses(#[case] input: &str, #[case] ok: bool) {
        assert_eq!(looks_like_email(input), ok, "{input}");
    }

    #[rstest]
    #[case("Amara Okafor", "Amara")]
    #[case("Cher", "Cher")]
    #[case("  Grace  Whitfield ", "Grace")]
    #[case("", "")]
    fn first_name_is_the_first_word(#[case] full: &str, #[case] first: &str) {
        assert_eq!(first_name(full), first);
    }

    // ------------------------------------------------------------ rendering

    #[rstest]
    fn render_fills_in_every_placeholder(loan: Loan) {
        let all: String = PLACEHOLDERS.iter().map(|(k, _)| format!("{{{{{k}}}}}|")).collect();
        let out = render(&all, &loan, &settings());
        assert_eq!(
            out,
            "Amara|Amara Okafor|amara@example.org|Lenovo ThinkPad E14|LT-0103|PF4A9K2M|\
             Friday 25 September 2026|Friday 2 October 2026|4 days|the library desk|Ms Patel, IT|\
             Hillside Academy|Tuesday 6 October 2026|"
        );
    }

    #[rstest]
    fn render_accepts_placeholders_with_spaces(loan: Loan) {
        assert_eq!(render("Hi {{ first_name }}", &loan, &settings()), "Hi Amara");
    }

    #[rstest]
    fn render_leaves_unknown_placeholders_alone(loan: Loan) {
        assert_eq!(render("{{shoe_size}}", &loan, &settings()), "{{shoe_size}}");
    }

    #[rstest]
    fn render_names_a_laptop_with_no_model(mut loan: Loan) {
        loan.model.clear();
        loan.serial.clear();
        assert_eq!(render("{{laptop}} / {{serial}}", &loan, &settings()), "Laptop / n/a");
    }

    #[rstest]
    fn render_says_zero_days_for_a_loan_not_yet_due(mut loan: Loan) {
        loan.due_at = end_of_day(clock::now() + DAY);
        assert_eq!(render("{{days_late}}", &loan, &settings()), "0 days");
    }

    #[test]
    fn default_templates_cover_every_purpose() {
        let purposes: Vec<&str> = default_templates().iter().map(|t| t.purpose).collect();
        assert_eq!(purposes, ["overdue", "overdue", "reminder", "receipt"]);
    }

    /// Snapshot of each built-in template, rendered for a real loan.
    #[rstest]
    fn default_templates_render_as_expected(loan: Loan) {
        for t in default_templates() {
            let text =
                format!("Subject: {}\n\n{}", render(t.subject, &loan, &settings()), render(t.body, &loan, &settings()));
            insta::assert_snapshot!(t.name.to_lowercase().replace([' ', '-'], "_"), text);
        }
    }

    // ------------------------------------------------------------ links

    #[rstest]
    #[case("a b", "a%20b")]
    #[case("a+b&c=d", "a%2Bb%26c%3Dd")]
    #[case("keep-_.!~*'()", "keep-_.!~*'()")]
    #[case("line\nbreak", "line%0Abreak")]
    #[case("Tomás", "Tom%C3%A1s")]
    #[case("€", "%E2%82%AC")]
    fn encode_component_matches_javascript(#[case] input: &str, #[case] encoded: &str) {
        assert_eq!(encode_component(input), encoded);
    }

    fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    proptest! {
        #[test]
        fn encoding_is_reversible(s in "\\PC*") {
            prop_assert_eq!(decode(&encode_component(&s)), s);
        }

        #[test]
        fn encoded_text_is_safe_in_a_url(s in "\\PC*") {
            let e = encode_component(&s);
            prop_assert!(e.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.!~*'()%".contains(&b)));
        }
    }

    #[rstest]
    #[case("mailto", "mailto:a%40b.org?subject=Hi%20there&body=Line%201%0D%0ALine%202")]
    #[case(
        "outlook",
        "https://outlook.office.com/mail/deeplink/compose?to=a%40b.org&subject=Hi%20there&body=Line%201%0D%0ALine%202"
    )]
    #[case(
        "outlook_live",
        "https://outlook.live.com/mail/0/deeplink/compose?to=a%40b.org&subject=Hi%20there&body=Line%201%0D%0ALine%202"
    )]
    #[case(
        "gmail",
        "https://mail.google.com/mail/?view=cm&fs=1&to=a%40b.org&su=Hi%20there&body=Line%201%0D%0ALine%202"
    )]
    #[case("anything-else", "mailto:a%40b.org?subject=Hi%20there&body=Line%201%0D%0ALine%202")]
    fn compose_url_opens_each_email_app(#[case] app: &str, #[case] url: &str) {
        assert_eq!(compose_url(app, "a@b.org", "Hi there", "Line 1\nLine 2"), url);
    }

    #[test]
    fn compose_url_does_not_double_windows_line_endings() {
        assert!(compose_url("mailto", "a@b.org", "s", "a\r\nb").ends_with("body=a%0D%0Ab"));
    }

    #[test]
    fn every_email_app_has_a_label() {
        let keys: Vec<&str> = APPS.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, ["mailto", "outlook", "outlook_live", "gmail"]);
    }
}
