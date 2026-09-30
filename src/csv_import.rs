//! Turns spreadsheet exports (CSV, semicolon- or tab-separated) into borrower
//! and laptop records. Column headers are matched loosely, so exports from
//! MIS/HR systems usually work without editing.

use crate::email::looks_like_email;
use crate::models::{BorrowerInput, LaptopInput};

#[derive(Clone, Debug, PartialEq)]
pub struct Draft<T> {
    /// 1-based line in the file, for error messages.
    pub line: usize,
    pub record: T,
    pub problem: Option<String>,
}

fn normalise(h: &str) -> String {
    h.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    let count = |c: char| first.matches(c).count();
    [(b',', count(',')), (b';', count(';')), (b'\t', count('\t'))]
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map(|(d, _)| d)
        .unwrap_or(b',')
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
    names
        .iter()
        .find_map(|n| headers.iter().position(|h| h == n))
}

fn cell(row: &[String], idx: Option<usize>) -> String {
    idx.and_then(|i| row.get(i)).cloned().unwrap_or_default()
}

pub const BORROWER_HEADERS: &str = "name,email,department,id,phone,notes";

pub fn borrowers(rows: &[Vec<String>]) -> Vec<Draft<BorrowerInput>> {
    let headers: Vec<String> = rows[0].iter().map(|h| normalise(h)).collect();
    let name = find(&headers, &["name", "fullname", "borrower", "displayname", "studentname", "staffname", "pupilname"]);
    let first = find(&headers, &["firstname", "first", "givenname", "forename", "preferredname"]);
    let last = find(&headers, &["lastname", "last", "surname", "familyname"]);
    let email = find(&headers, &["email", "emailaddress", "mail", "workemail", "schoolemail", "upn", "userprincipalname"]);
    let has_header = name.is_some() || first.is_some() || email.is_some();

    let (name, email, dept, ext, phone, notes, skip) = if has_header {
        (
            name,
            email,
            find(&headers, &["department", "dept", "group", "class", "form", "team", "year", "yeargroup", "tutorgroup", "homeroom", "course"]),
            find(&headers, &["id", "studentid", "staffid", "employeeid", "idnumber", "cardnumber", "librarycard", "barcode", "admissionnumber", "upn2"]),
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
    let tag = find(&headers, &["assettag", "asset", "tag", "assetno", "assetnumber", "assetid", "devicename", "hostname", "computername", "name"]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowers_with_headers() {
        let rows = parse("First Name,Surname,Email Address,Form\nAmara,Okafor,amara@example.org,11B\n,,,\nLiam,Chen,bad,10A\n").unwrap();
        let d = borrowers(&rows);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].record.name, "Amara Okafor");
        assert_eq!(d[0].record.department, "11B");
        assert!(d[0].problem.is_none());
        assert!(d[1].problem.is_some());
    }

    #[test]
    fn borrowers_without_headers_semicolons() {
        let rows = parse("Grace Whitfield;g.whitfield@example.org;English").unwrap();
        let d = borrowers(&rows);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].record.email, "g.whitfield@example.org");
    }

    #[test]
    fn laptops_and_repeats() {
        let rows = parse("Asset Tag,Model,Serial Number\nLT-1,Dell,A\nlt-1,Dell,B\n").unwrap();
        let mut d = laptops(&rows);
        mark_repeats(&mut d, |l| l.asset_tag.clone());
        assert!(d[0].problem.is_none());
        assert!(d[1].problem.is_some());
    }
}
