//! A CSV import in progress: parsed rows, their problems, and which ones
//! already exist in the database.

use crate::csv_import::{self, Draft};
use crate::models::{BorrowerInput, LaptopInput};
use crate::repo::{self, ImportResult};
use crate::time;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImportKind {
    Borrowers,
    Laptops,
}

impl ImportKind {
    pub fn noun(self) -> &'static str {
        match self {
            ImportKind::Borrowers => "borrower",
            ImportKind::Laptops => "laptop",
        }
    }

    pub fn columns(self) -> [&'static str; 4] {
        match self {
            ImportKind::Borrowers => ["Name", "Email", "Department", "ID"],
            ImportKind::Laptops => ["Asset tag", "Model", "Serial", "Notes"],
        }
    }

    pub fn headers(self) -> &'static str {
        match self {
            ImportKind::Borrowers => csv_import::BORROWER_HEADERS,
            ImportKind::Laptops => csv_import::LAPTOP_HEADERS,
        }
    }

    pub fn example(self) -> &'static str {
        match self {
            ImportKind::Borrowers => "name,email,department,id\nAmara Okafor,amara.okafor@school.org,Year 11,S20931\nGrace Whitfield,g.whitfield@school.org,English,T0388",
            ImportKind::Laptops => "asset_tag,model,serial\nLT-0201,Dell Latitude 3440,7HQ9ZK3\nLT-0202,Dell Latitude 3440,7HQ9ZK4",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Borrowers(Vec<Draft<BorrowerInput>>),
    Laptops(Vec<Draft<LaptopInput>>),
}

/// One preview row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreviewRow {
    pub line: usize,
    pub cells: Vec<String>,
    pub problem: Option<String>,
    /// Matches a record already in the database.
    pub exists: bool,
}

/// Rows that are new, already here, or have problems.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub new: usize,
    pub existing: usize,
    pub bad: usize,
}

impl Tally {
    pub fn of(rows: &[PreviewRow]) -> Tally {
        let bad = rows.iter().filter(|r| r.problem.is_some()).count();
        let existing = rows.iter().filter(|r| r.problem.is_none() && r.exists).count();
        Tally { new: rows.len() - bad - existing, existing, bad }
    }

    /// Whether the Import button does anything.
    pub fn can_import(self, update_existing: bool) -> bool {
        self.new > 0 || (update_existing && self.existing > 0)
    }

    pub fn button_label(self, update_existing: bool) -> String {
        if update_existing && self.existing > 0 {
            format!("Import {} and update {}", self.new, self.existing)
        } else {
            format!("Import {}", self.new)
        }
    }
}

impl Parsed {
    /// Parses pasted or uploaded text; nothing for an empty box.
    pub fn from_text(kind: ImportKind, text: &str) -> Option<Result<Parsed, String>> {
        (!text.trim().is_empty()).then(|| Parsed::build(kind, text))
    }

    pub fn build(kind: ImportKind, text: &str) -> Result<Parsed, String> {
        let rows = csv_import::parse(text)?;
        Ok(match kind {
            ImportKind::Borrowers => {
                let mut d = csv_import::borrowers(&rows);
                csv_import::mark_repeats(&mut d, |b| b.email.clone());
                Parsed::Borrowers(d)
            }
            ImportKind::Laptops => {
                let mut d = csv_import::laptops(&rows);
                csv_import::mark_repeats(&mut d, |l| l.asset_tag.clone());
                Parsed::Laptops(d)
            }
        })
    }

    pub fn preview(&self) -> Vec<PreviewRow> {
        match self {
            Parsed::Borrowers(d) => d
                .iter()
                .map(|r| {
                    let b = &r.record;
                    PreviewRow {
                        line: r.line,
                        cells: vec![b.name.clone(), b.email.clone(), b.department.clone(), b.external_id.clone()],
                        problem: r.problem.clone(),
                        exists: repo::find_borrower_by_email(&b.email).is_some(),
                    }
                })
                .collect(),
            Parsed::Laptops(d) => d
                .iter()
                .map(|r| {
                    let l = &r.record;
                    PreviewRow {
                        line: r.line,
                        cells: vec![l.asset_tag.clone(), l.model.clone(), l.serial.clone(), l.notes.clone()],
                        problem: r.problem.clone(),
                        exists: repo::find_laptop_by_tag(&l.asset_tag).is_some(),
                    }
                })
                .collect(),
        }
    }

    /// Imports the rows without problems.
    pub fn import(&self, update_existing: bool) -> Result<ImportResult, String> {
        fn good<T: Clone>(d: &[Draft<T>]) -> Vec<T> {
            d.iter().filter(|r| r.problem.is_none()).map(|r| r.record.clone()).collect()
        }
        match self {
            Parsed::Borrowers(d) => repo::import_borrowers(&good(d), update_existing),
            Parsed::Laptops(d) => repo::import_laptops(&good(d), update_existing),
        }
    }
}

/// "Added 3 borrowers, updated 1, skipped 2."
pub fn result_message(kind: ImportKind, r: ImportResult) -> String {
    let mut parts = vec![format!("Added {}", time::plural(r.added as i64, kind.noun()))];
    if r.updated > 0 {
        parts.push(format!("updated {}", r.updated));
    }
    if r.skipped > 0 {
        parts.push(format!("skipped {}", r.skipped));
    }
    format!("{}.", parts.join(", "))
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::test_support::seeded;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn row(problem: Option<&str>, exists: bool) -> PreviewRow {
        PreviewRow { line: 2, cells: vec![], problem: problem.map(String::from), exists }
    }

    #[test]
    fn the_tally_puts_each_row_in_one_bucket() {
        let rows = [row(None, false), row(None, false), row(None, true), row(Some("x"), true), row(Some("y"), false)];
        assert_eq!(Tally::of(&rows), Tally { new: 2, existing: 1, bad: 2 });
    }

    #[rstest]
    #[case::new_rows(Tally { new: 1, existing: 0, bad: 0 }, false, true, "Import 1")]
    #[case::nothing_new(Tally { new: 0, existing: 2, bad: 1 }, false, false, "Import 0")]
    #[case::updating(Tally { new: 0, existing: 2, bad: 0 }, true, true, "Import 0 and update 2")]
    #[case::update_ticked_but_nothing_to_update(Tally { new: 3, existing: 0, bad: 0 }, true, true, "Import 3")]
    #[case::empty(Tally::default(), true, false, "Import 0")]
    fn the_import_button_says_what_will_happen(
        #[case] t: Tally,
        #[case] update: bool,
        #[case] enabled: bool,
        #[case] label: &str,
    ) {
        assert_eq!((t.can_import(update), t.button_label(update).as_str()), (enabled, label));
    }

    #[rstest]
    fn borrower_previews_mark_people_already_here(_seeded: ()) {
        let p = Parsed::build(
            ImportKind::Borrowers,
            "name,email\nAmara Okafor,amara.okafor@example.org\nNew Person,new@example.org\nBad,nope",
        )
        .unwrap();
        let rows = p.preview();
        assert_eq!(
            rows[0],
            PreviewRow {
                line: 2,
                cells: vec!["Amara Okafor".into(), "amara.okafor@example.org".into(), "".into(), "".into()],
                problem: None,
                exists: true,
            }
        );
        assert_eq!(Tally::of(&rows), Tally { new: 1, existing: 1, bad: 1 });
    }

    #[rstest]
    fn laptop_previews_mark_tags_already_here(_seeded: ()) {
        let p = Parsed::build(
            ImportKind::Laptops,
            "asset tag,model,serial,notes\nlt-0101,Dell,X,n\nLT-0900,HP,Y,\nLT-0900,HP,Z,",
        )
        .unwrap();
        let rows = p.preview();
        assert_eq!(rows[0].cells, ["lt-0101", "Dell", "X", "n"]);
        assert_eq!(Tally::of(&rows), Tally { new: 1, existing: 1, bad: 1 });
    }

    #[rstest]
    fn importing_skips_rows_with_problems(_seeded: ()) {
        let p = Parsed::build(ImportKind::Borrowers, "name,email\nNew Person,new@example.org\nNo Email,").unwrap();
        let r = p.import(false).unwrap();
        assert_eq!(r, ImportResult { added: 1, updated: 0, skipped: 0 });
    }

    #[rstest]
    fn importing_laptops_can_update_existing_ones(_seeded: ()) {
        let p = Parsed::build(ImportKind::Laptops, "asset tag,model\nLT-0101,Dell Latitude 7450\nLT-0999,HP").unwrap();
        assert_eq!(p.import(true).unwrap(), ImportResult { added: 1, updated: 1, skipped: 0 });
        let model = repo::laptops().into_iter().find(|l| l.asset_tag == "LT-0101").unwrap().model;
        assert_eq!(model, "Dell Latitude 7450");
    }

    #[rstest]
    #[case("")]
    #[case("  \n ")]
    fn an_empty_box_parses_to_nothing(#[case] text: &str) {
        assert_eq!(Parsed::from_text(ImportKind::Borrowers, text), None);
    }

    #[test]
    fn text_in_the_box_is_parsed() {
        assert!(matches!(
            Parsed::from_text(ImportKind::Borrowers, "name,email\nA,a@b.org"),
            Some(Ok(Parsed::Borrowers(_)))
        ));
    }

    #[test]
    fn an_empty_file_is_an_error() {
        assert_eq!(Parsed::build(ImportKind::Laptops, " \n"), Err("The file is empty.".into()));
    }

    #[rstest]
    #[case(ImportResult { added: 1, updated: 0, skipped: 0 }, "Added 1 borrower.")]
    #[case(ImportResult { added: 3, updated: 2, skipped: 0 }, "Added 3 borrowers, updated 2.")]
    #[case(ImportResult { added: 0, updated: 0, skipped: 4 }, "Added 0 borrowers, skipped 4.")]
    #[case(ImportResult { added: 2, updated: 1, skipped: 1 }, "Added 2 borrowers, updated 1, skipped 1.")]
    fn result_messages_mention_only_what_happened(#[case] r: ImportResult, #[case] text: &str) {
        assert_eq!(result_message(ImportKind::Borrowers, r), text);
    }

    #[rstest]
    #[case(ImportKind::Borrowers, "borrower", "Name", "name,email,department,id,phone,notes")]
    #[case(ImportKind::Laptops, "laptop", "Asset tag", "asset_tag,model,serial,notes")]
    fn each_kind_describes_its_columns(
        #[case] k: ImportKind,
        #[case] noun: &str,
        #[case] first: &str,
        #[case] headers: &str,
    ) {
        assert_eq!((k.noun(), k.columns()[0], k.headers()), (noun, first, headers));
    }

    #[rstest]
    #[case(ImportKind::Borrowers)]
    #[case(ImportKind::Laptops)]
    fn each_example_imports_cleanly(_seeded: (), #[case] k: ImportKind) {
        let rows = Parsed::build(k, k.example()).unwrap().preview();
        assert_eq!(Tally::of(&rows), Tally { new: 2, existing: 0, bad: 0 });
    }
}
