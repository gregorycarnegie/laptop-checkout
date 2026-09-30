//! Turns spreadsheet exports (CSV, semicolon- or tab-separated) into borrower
//! and laptop records. Column headers are matched loosely, so exports from
//! MIS/HR systems usually work without editing.

use crate::{
    email::looks_like_email,
    models::{BorrowerInput, LaptopInput},
};

#[derive(Clone, Debug, PartialEq)]
pub struct Draft<T> {
    /// 1-based line in the file, for error messages.
    pub line: usize,
    pub record: T,
    pub problem: Option<String>,
}

fn normalise(h: &str) -> String {
    h.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The separator used most in the first line. Ties (and none at all) go to
/// the comma, the standard CSV separator.
fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    let count = |c: char| first.matches(c).count();
    let mut best = (b',', count(','));
    for candidate in [(b';', count(';')), (b'\t', count('\t'))] {
        if candidate.1 > best.1 {
            best = candidate;
        }
    }
    best.0
}

/// Parses CSV text into rows of trimmed cells, skipping blank lines.
pub fn parse(text: &str) -> Result<Vec<Vec<String>>, String> {
    let text = text.trim_start_matches('\u{feff}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .delimiter(sniff_delimiter(text))
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for rec in reader.records() {
        let rec = rec.map_err(|e| format!("Couldn't read the CSV: {e}"))?;
        let row: Vec<String> = rec.iter().map(str::to_string).collect();
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return Err("The file is empty.".into());
    }
    Ok(rows)
}

fn find(headers: &[String], names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| headers.iter().position(|h| h == n))
}

fn cell(row: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i)).cloned().unwrap_or_default()
}

pub const BORROWER_HEADERS: &str = "name,email,department,id,phone,notes";

pub fn borrowers(rows: &[Vec<String>]) -> Vec<Draft<BorrowerInput>> {
    let headers: Vec<String> = rows[0].iter().map(|h| normalise(h)).collect();
    let name =
        find(&headers, &["name", "fullname", "borrower", "displayname", "studentname", "staffname", "pupilname"]);
    let first = find(&headers, &["firstname", "first", "givenname", "forename", "preferredname"]);
    let last = find(&headers, &["lastname", "last", "surname", "familyname"]);
    let email =
        find(&headers, &["email", "emailaddress", "mail", "workemail", "schoolemail", "upn", "userprincipalname"]);
    let has_header = name.is_some() || first.is_some() || email.is_some();

    let (name, email, dept, ext, phone, notes, skip) = if has_header {
        (
            name,
            email,
            find(
                &headers,
                &[
                    "department",
                    "dept",
                    "group",
                    "class",
                    "form",
                    "team",
                    "year",
                    "yeargroup",
                    "tutorgroup",
                    "homeroom",
                    "course",
                ],
            ),
            find(
                &headers,
                &[
                    "id",
                    "studentid",
                    "staffid",
                    "employeeid",
                    "idnumber",
                    "cardnumber",
                    "librarycard",
                    "barcode",
                    "admissionnumber",
                    "upn2",
                ],
            ),
            find(&headers, &["phone", "mobile", "telephone", "tel", "phonenumber"]),
            find(&headers, &["notes", "note", "comments", "comment"]),
            1,
        )
    } else {
        // No header row: assume name, email, department.
        (Some(0), Some(1), Some(2), Some(3), None, None, 0)
    };

    rows.iter()
        .enumerate()
        .skip(skip)
        .map(|(i, row)| {
            let mut full = cell(row, name);
            if full.is_empty() {
                full = format!("{} {}", cell(row, first), cell(row, last)).trim().to_string();
            }
            let record = BorrowerInput {
                name: full,
                email: cell(row, email),
                department: cell(row, dept),
                external_id: cell(row, ext),
                phone: cell(row, phone),
                notes: cell(row, notes),
            };
            let problem = if record.name.is_empty() {
                Some("No name".to_string())
            } else if record.email.is_empty() {
                Some("No email address".to_string())
            } else if !looks_like_email(&record.email) {
                Some(format!("\"{}\" isn't an email address", record.email))
            } else {
                None
            };
            Draft { line: i + 1, record, problem }
        })
        .collect()
}

pub const LAPTOP_HEADERS: &str = "asset_tag,model,serial,notes";

pub fn laptops(rows: &[Vec<String>]) -> Vec<Draft<LaptopInput>> {
    let headers: Vec<String> = rows[0].iter().map(|h| normalise(h)).collect();
    let tag = find(
        &headers,
        &[
            "assettag",
            "asset",
            "tag",
            "assetno",
            "assetnumber",
            "assetid",
            "devicename",
            "hostname",
            "computername",
            "name",
        ],
    );
    let (tag, model, serial, notes, skip) = if tag.is_some() {
        (
            tag,
            find(&headers, &["model", "makemodel", "make", "description", "device", "type"]),
            find(&headers, &["serial", "serialnumber", "sn", "servicetag", "serialno"]),
            find(&headers, &["notes", "note", "comments", "comment"]),
            1,
        )
    } else {
        (Some(0), Some(1), Some(2), Some(3), 0)
    };
    rows.iter()
        .enumerate()
        .skip(skip)
        .map(|(i, row)| {
            let record = LaptopInput {
                asset_tag: cell(row, tag),
                model: cell(row, model),
                serial: cell(row, serial),
                notes: cell(row, notes),
            };
            let problem = record.asset_tag.is_empty().then(|| "No asset tag".to_string());
            Draft { line: i + 1, record, problem }
        })
        .collect()
}

/// Flags rows that repeat an earlier row's key within the same file.
pub fn mark_repeats<T>(drafts: &mut [Draft<T>], key: impl Fn(&T) -> String) {
    let mut seen = std::collections::HashSet::new();
    for d in drafts.iter_mut().filter(|d| d.problem.is_none()) {
        let k = key(&d.record).to_lowercase();
        if !seen.insert(k.clone()) {
            d.problem = Some(format!("{k} appears earlier in the file"));
        }
    }
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use fake::{
        faker::{internet::en::SafeEmail, name::en::Name},
        Fake,
    };
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use rstest::rstest;

    fn rows(text: &str) -> Vec<Vec<String>> {
        parse(text).expect("valid CSV")
    }

    // ------------------------------------------------------------ parsing

    #[rstest]
    #[case::commas("a,b,c", b',')]
    #[case::tie_goes_to_commas("a,b;c", b',')]
    #[case::tabs_beat_commas("a\tb\tc,d", b'\t')]
    #[case::semicolons("a;b;c,d", b';')]
    #[case::tabs("a\tb\tc", b'\t')]
    #[case::single_column("name", b',')]
    #[case::empty("", b',')]
    fn the_delimiter_is_whichever_appears_most_in_the_first_line(#[case] text: &str, #[case] delimiter: u8) {
        assert_eq!(sniff_delimiter(text), delimiter);
    }

    #[test]
    fn normalise_keeps_only_lowercase_letters_and_digits() {
        assert_eq!(normalise(" E-mail Address 2 "), "emailaddress2");
    }

    #[test]
    fn parse_trims_cells_and_skips_blank_lines() {
        assert_eq!(rows(" a , b \n\n,,\nc,d\n"), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_strips_a_byte_order_mark() {
        assert_eq!(rows("\u{feff}name,email\n")[0][0], "name");
    }

    #[test]
    fn parse_handles_quoted_commas() {
        assert_eq!(rows("\"Okafor, Amara\",a@b.org")[0], vec!["Okafor, Amara", "a@b.org"]);
    }

    #[test]
    fn parse_rejects_an_empty_file() {
        assert_eq!(parse("\n , \n"), Err("The file is empty.".to_string()));
    }

    #[test]
    fn rows_of_different_lengths_are_allowed() {
        assert_eq!(rows("a,b,c\nd").len(), 2);
    }

    proptest! {
        #[test]
        fn parsing_arbitrary_text_never_panics(text in "\\PC{0,400}") {
            if let Ok(r) = parse(&text) {
                let _ = borrowers(&r);
                let _ = laptops(&r);
            }
        }
    }

    // ------------------------------------------------------------ borrowers

    /// Generates a test per header spelling: each must be read as the right field.
    macro_rules! borrower_headers {
        ($($name:ident: $header:literal => $field:ident)*) => {
            $(
                #[test]
                fn $name() {
                    let d = borrowers(&rows(&format!("name,email,{}\nAmara Okafor,a@b.org,VALUE", $header)));
                    assert_eq!(d[0].record.$field, "VALUE");
                }
            )*
        };
    }

    borrower_headers! {
        department_header_dept: "Dept" => department
        department_header_form: "Form" => department
        department_header_tutor_group: "Tutor Group" => department
        department_header_year_group: "Year Group" => department
        department_header_class: "class" => department
        id_header_student_id: "Student ID" => external_id
        id_header_staff_id: "Staff ID" => external_id
        id_header_card_number: "Card Number" => external_id
        id_header_employee_id: "Employee ID" => external_id
        phone_header_mobile: "Mobile" => phone
        phone_header_telephone: "Telephone" => phone
        notes_header_comments: "Comments" => notes
    }

    #[test]
    fn first_name_and_surname_columns_are_joined() {
        let d = borrowers(&rows("First Name,Surname,Email Address\nAmara,Okafor,amara@example.org"));
        assert_eq!(d[0].record.name, "Amara Okafor");
        assert_eq!(d[0].record.email, "amara@example.org");
    }

    #[test]
    fn a_full_name_column_wins_over_first_and_last() {
        let d = borrowers(&rows("Full Name,First,Last,Email\nDr Amara Okafor,Amara,Okafor,a@b.org"));
        assert_eq!(d[0].record.name, "Dr Amara Okafor");
    }

    #[rstest]
    #[case::name_only("Name,Form\nAmara Okafor,11B")]
    #[case::first_name_only("First Name,Surname\nAmara,Okafor")]
    #[case::email_only("Email,Form\namara@example.org,11B")]
    fn any_one_known_column_marks_a_header_row(#[case] text: &str) {
        let d = borrowers(&rows(text));
        assert_eq!(d.len(), 1, "the header row isn't read as a person");
        assert_eq!(d[0].line, 2);
    }

    #[test]
    fn email_alone_is_enough_to_recognise_a_header_row() {
        let d = borrowers(&rows("Forename,E-mail\nAmara,a@b.org"));
        assert_eq!((d.len(), d[0].record.name.as_str()), (1, "Amara"));
    }

    #[test]
    fn a_file_without_headers_is_read_as_name_email_department_id() {
        let d = borrowers(&rows("Grace Whitfield;g.whitfield@example.org;English;T0388"));
        assert_eq!(
            d[0].record,
            BorrowerInput {
                name: "Grace Whitfield".into(),
                email: "g.whitfield@example.org".into(),
                department: "English".into(),
                external_id: "T0388".into(),
                phone: String::new(),
                notes: String::new(),
            }
        );
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn line_numbers_count_the_header_row() {
        let d = borrowers(&rows("name,email\nA,a@b.org\nB,b@b.org"));
        assert_eq!(d.iter().map(|r| r.line).collect::<Vec<_>>(), [2, 3]);
    }

    #[rstest]
    #[case::no_name(",a@b.org", "No name")]
    #[case::no_email("Amara,", "No email address")]
    #[case::bad_email("Amara,amara", "\"amara\" isn't an email address")]
    fn rows_with_problems_say_what_is_wrong(#[case] row: &str, #[case] problem: &str) {
        let d = borrowers(&rows(&format!("name,email\n{row}")));
        assert_eq!(d[0].problem.as_deref(), Some(problem));
    }

    #[test]
    fn a_good_row_has_no_problem() {
        assert_eq!(borrowers(&rows("name,email\nAmara,a@b.org"))[0].problem, None);
    }

    #[test]
    fn short_rows_leave_missing_cells_empty() {
        let d = borrowers(&rows("name,email,department\nAmara,a@b.org"));
        assert_eq!(d[0].record.department, "");
    }

    /// Property-style round trip with realistic fake people.
    #[test]
    fn fake_borrowers_survive_a_csv_round_trip() {
        let people: Vec<(String, String)> = (0..200).map(|_| (Name().fake(), SafeEmail().fake())).collect();
        let mut csv = String::from("Name,Email\n");
        for (name, email) in &people {
            csv.push_str(&format!("\"{name}\",{email}\n"));
        }
        let d = borrowers(&rows(&csv));
        let back: Vec<(String, String)> = d.iter().map(|r| (r.record.name.clone(), r.record.email.clone())).collect();
        assert_eq!(back, people);
        assert!(d.iter().all(|r| r.problem.is_none()));
    }

    // ------------------------------------------------------------ laptops

    #[rstest]
    #[case("Asset Tag,Model,Serial Number")]
    #[case("Asset No,Make,S/N")]
    #[case("Hostname,Description,Service Tag")]
    #[case("Device Name,Type,Serial No")]
    fn laptop_header_spellings_are_recognised(#[case] header: &str) {
        let d = laptops(&rows(&format!("{header}\nLT-1,Dell 3440,ABC123")));
        assert_eq!(
            d[0].record,
            LaptopInput {
                asset_tag: "LT-1".into(),
                model: "Dell 3440".into(),
                serial: "ABC123".into(),
                notes: String::new()
            }
        );
    }

    #[test]
    fn laptops_without_headers_are_read_as_tag_model_serial_notes() {
        let d = laptops(&rows("LT-9,HP 440,5CD1,Spare charger"));
        assert_eq!(d[0].record.notes, "Spare charger");
        assert_eq!(d[0].line, 1);
    }

    #[test]
    fn a_laptop_needs_an_asset_tag() {
        let d = laptops(&rows("asset_tag,model\n,Dell"));
        assert_eq!(d[0].problem.as_deref(), Some("No asset tag"));
    }

    #[test]
    fn repeats_within_a_file_are_flagged_ignoring_case() {
        let mut d = laptops(&rows("Asset Tag,Model\nLT-1,Dell\nlt-1,Dell\nLT-2,Dell"));
        mark_repeats(&mut d, |l| l.asset_tag.clone());
        let problems: Vec<Option<&str>> = d.iter().map(|r| r.problem.as_deref()).collect();
        assert_eq!(problems, [None, Some("lt-1 appears earlier in the file"), None]);
    }

    #[test]
    fn rows_that_already_have_a_problem_are_not_counted_as_repeats() {
        let mut d = laptops(&rows("Asset Tag,Model\n,Dell\n,Dell"));
        mark_repeats(&mut d, |l| l.asset_tag.clone());
        assert!(d.iter().all(|r| r.problem.as_deref() == Some("No asset tag")));
    }
}
