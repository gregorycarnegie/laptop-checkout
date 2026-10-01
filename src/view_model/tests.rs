use super::*;
use crate::{
    test_support::{at, now},
    time::{clock, end_of_day, HOUR},
};
use pretty_assertions::assert_eq;
use proptest::prelude::*;
use rstest::{fixture, rstest};

#[fixture]
fn loan() -> Loan {
    clock::set_now(now());
    let mut l = sample_loan(now());
    l.id = 1;
    l
}

fn with_due(mut l: Loan, id: i64, due: i64) -> Loan {
    l.id = id;
    l.due_at = due;
    l
}

fn laptop(id: i64, tag: &str, status: &str, due: Option<i64>) -> Laptop {
    Laptop {
        id,
        asset_tag: tag.into(),
        model: "Dell Latitude".into(),
        serial: format!("SN{id}"),
        notes: String::new(),
        status: ServiceStatus::parse(status).unwrap(),
        loan_id: due.map(|_| id * 10),
        due_at: due,
        borrower_name: due.map(|_| "Amara Okafor".into()),
        total_loans: 0,
    }
}

fn borrower(id: i64, name: &str, active: bool, open: i64, late: i64) -> Borrower {
    Borrower {
        id,
        name: name.into(),
        email: format!("{}@example.org", name.to_lowercase().replace(' ', ".")),
        department: "Year 11".into(),
        external_id: format!("S{id}"),
        phone: String::new(),
        notes: String::new(),
        active: i64::from(active),
        open_loans: open,
        late_loans: late,
        total_loans: open,
    }
}

fn template(id: i64, purpose: &str) -> Template {
    Template { id, name: format!("T{id}"), subject: String::new(), body: String::new(), purpose: purpose.into() }
}

// ------------------------------------------------------------ navigation

#[test]
fn every_page_round_trips_through_its_hash() {
    for p in Page::ALL {
        assert_eq!(Page::from_hash(&format!("#{}", p.slug())), p);
    }
}

#[rstest]
#[case("")]
#[case("#")]
#[case("#overdue")]
#[case("#Loans")]
fn unknown_hashes_open_the_desk(#[case] hash: &str) {
    assert_eq!(Page::from_hash(hash), Page::Desk);
}

#[test]
fn pages_have_distinct_labels() {
    let labels: Vec<&str> = Page::ALL.iter().map(|p| p.label()).collect();
    assert_eq!(labels, ["Desk", "Loans", "Laptops", "Borrowers", "Email templates", "Settings"]);
}

// ------------------------------------------------------------ toasts

fn toast(id: u64) -> Toast {
    Toast { id, kind: ToastKind::Ok, text: format!("t{id}"), action: None }
}

#[test]
fn toasts_are_capped_dropping_the_oldest() {
    let mut list = Vec::new();
    for id in 1..=6 {
        push_toast(&mut list, toast(id));
    }
    assert_eq!(list.iter().map(|t| t.id).collect::<Vec<_>>(), [3, 4, 5, 6]);
}

#[test]
fn a_full_list_still_accepts_the_last_allowed_toast() {
    let mut list = Vec::new();
    for id in 1..=MAX_TOASTS as u64 {
        push_toast(&mut list, toast(id));
    }
    assert_eq!(list.len(), MAX_TOASTS);
    assert_eq!(list[0].id, 1);
}

#[rstest]
#[case(ToastKind::Ok, "toast ok", 6)]
#[case(ToastKind::Warn, "toast warn", 6)]
#[case(ToastKind::Error, "toast error", 9)]
fn toast_kinds_have_a_style_and_duration(#[case] kind: ToastKind, #[case] class: &str, #[case] secs: u64) {
    assert_eq!((kind.class(), kind.seconds()), (class, secs));
}

// ------------------------------------------------------------ loans

#[rstest]
fn late_and_due_soon_split_open_loans(loan: Loan) {
    let today = end_of_day(now());
    let open = vec![
        with_due(loan.clone(), 1, today - 3 * DAY),
        with_due(loan.clone(), 2, today),
        with_due(loan.clone(), 3, today + DAY),
        with_due(loan.clone(), 4, today + 2 * DAY),
    ];
    let ids = |v: Vec<Loan>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(ids(late(&open, now())), [1]);
    assert_eq!(ids(due_soon(&open, now())), [2, 3]);
}

#[rstest]
fn a_loan_due_this_instant_is_due_soon_not_late(loan: Loan) {
    let open = vec![with_due(loan, 1, now())];
    assert!(late(&open, now()).is_empty());
    assert_eq!(due_soon(&open, now()).len(), 1);
}

#[rstest]
fn emailable_skips_borrowers_without_an_address(loan: Loan) {
    let mut no_email = with_due(loan.clone(), 2, 0);
    no_email.borrower_email.clear();
    assert_eq!(emailable(&[loan.clone(), no_email]), [1]);
    assert_eq!(emailable(&[with_due(loan.clone(), 5, 0), with_due(loan, 6, 0)]), [5, 6]);
    assert!(emailable(&[]).is_empty());
}

#[rstest]
#[case(LoanFilter::Open, [true, true, false])]
#[case(LoanFilter::Late, [true, false, false])]
#[case(LoanFilter::Returned, [false, false, true])]
#[case(LoanFilter::All, [true, true, true])]
fn loan_filters_pick_the_right_loans(loan: Loan, #[case] f: LoanFilter, #[case] expected: [bool; 3]) {
    let mut returned = with_due(loan.clone(), 3, now() - DAY);
    returned.returned_at = Some(now() - 2 * DAY);
    let loans = [with_due(loan.clone(), 1, now() - DAY), with_due(loan, 2, now() + DAY), returned];
    assert_eq!(loans.map(|l| f.keep(&l, now())), expected);
}

#[rstest]
fn a_loan_due_this_instant_is_not_in_the_overdue_filter(loan: Loan) {
    let l = with_due(loan, 1, now());
    assert!(!LoanFilter::Late.keep(&l, now()));
    assert!(LoanFilter::Late.keep(&l, now() + 1));
}

#[rstest]
#[case("amara", true)]
#[case("AMARA lt-0103", true)]
#[case("lenovo year", true)]
#[case("amara.okafor@example", true)]
#[case("amara grace", false)]
#[case("", true)]
fn loan_search_needs_every_word(loan: Loan, #[case] query: &str, #[case] hit: bool) {
    assert_eq!(loan_matches(&loan, query), hit);
}

#[test]
fn counts_tally_each_filter() {
    let n = [1, 2, 3, 4, 5];
    assert_eq!(counts(&n, &[(2, "a"), (4, "b")], |f, x| x % f == 0), [2, 1]);
}

#[rstest]
fn stamps_describe_open_loans(loan: Loan) {
    assert_eq!(stamp_text(loan.due_at, None, now()), ("3 days late".to_string(), "due Sat 3 Oct".to_string()));
    assert_eq!(stamp_class(loan.due_at, None, now()), "late");
}

#[rstest]
#[case::late(4, "In Wed 7 Oct", "4 days late")]
#[case::on_time(-1, "In Fri 2 Oct", "on time")]
fn stamps_describe_returned_loans(loan: Loan, #[case] days_after: i64, #[case] head: &str, #[case] detail: &str) {
    let returned = loan.due_at + days_after * DAY;
    assert_eq!(stamp_text(loan.due_at, Some(returned), now()), (head.to_string(), detail.to_string()));
    assert_eq!(stamp_class(loan.due_at, Some(returned), now()), "returned");
}

#[rstest]
fn emailed_text_shows_count_and_last_date(mut loan: Loan) {
    assert_eq!(emailed_text(&loan), None);
    loan.emails_sent = 2;
    loan.last_emailed_at = Some(at(2026, 10, 5, 9, 0));
    assert_eq!(emailed_text(&loan).as_deref(), Some("Emailed 2× · last Mon 5 Oct"));
}

#[rstest]
fn renewed_text_counts_renewals(mut loan: Loan) {
    assert_eq!(renewed_text(&loan), None);
    loan.renewals = 1;
    assert_eq!(renewed_text(&loan).as_deref(), Some("Renewed 1 time"));
}

#[rstest]
fn email_purpose_matches_the_loan(mut loan: Loan) {
    assert_eq!(email_purpose(&loan, now()), "overdue");
    loan.due_at = now();
    assert_eq!(email_purpose(&loan, now()), "reminder", "not late at the due instant");
    loan.due_at = now() + DAY;
    assert_eq!(email_purpose(&loan, now()), "reminder");
    loan.due_at = now() - DAY;
    loan.returned_at = Some(now());
    assert_eq!(email_purpose(&loan, now()), "reminder");
}

#[rstest]
fn returned_message_mentions_lateness(mut loan: Loan) {
    loan.returned_at = Some(now());
    assert_eq!(returned_message(&loan), "LT-0103 returned by Amara Okafor, 3 days late.");
    loan.returned_at = Some(loan.due_at - HOUR);
    assert_eq!(returned_message(&loan), "LT-0103 returned by Amara Okafor. Thanks!");
}

#[rstest]
fn returned_message_uses_now_for_a_loan_still_out(loan: Loan) {
    assert!(returned_message(&loan).ends_with("3 days late."));
}

#[rstest]
fn check_out_and_renew_messages_name_the_due_date(loan: Loan) {
    assert_eq!(checked_out_message(&loan), "LT-0103 checked out to Amara Okafor. Due Sat 3 Oct.");
    assert_eq!(renewed_message("LT-0103", at(2026, 10, 13, 22, 0)), "Renewed LT-0103. Now due Tue 13 Oct.");
}

// ------------------------------------------------------------ picker

fn items() -> Vec<PickItem> {
    available_laptop_items(&(1..=12).map(|i| laptop(i, &format!("LT-{i:02}"), "available", None)).collect::<Vec<_>>())
}

#[test]
fn find_item_looks_up_by_id() {
    assert_eq!(find_item(&items(), 3).map(|i| i.label), Some("LT-03".into()));
    assert_eq!(find_item(&items(), 99), None);
}

#[test]
fn pick_matches_is_limited() {
    assert_eq!(pick_matches(&items(), "").len(), PICKER_LIMIT);
}

#[test]
fn pick_matches_needs_every_word() {
    let hits = pick_matches(&items(), "lt-1 DELL");
    assert_eq!(hits.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["LT-10", "LT-11", "LT-12"]);
}

#[test]
fn enter_prefers_an_exact_tag_over_the_highlighted_item() {
    assert_eq!(pick_on_enter(&items(), " lt-01 ", 0), Some(1));
}

#[test]
fn enter_accepts_a_scanned_serial_number() {
    assert_eq!(pick_on_enter(&items(), "sn7", 0), Some(7));
}

#[test]
fn enter_otherwise_picks_the_highlighted_suggestion() {
    assert_eq!(pick_on_enter(&items(), "lt-1", 2), Some(12));
}

#[rstest]
#[case::empty("   ", 0)]
#[case::no_match("xyz", 0)]
#[case::cursor_past_the_end("lt-1", 9)]
fn enter_picks_nothing_without_a_match(#[case] q: &str, #[case] cursor: usize) {
    assert_eq!(pick_on_enter(&items(), q, cursor), None);
}

#[rstest]
#[case::down(0, true, 3, 1)]
#[case::down_stops_at_the_end(2, true, 3, 2)]
#[case::down_with_no_items(0, true, 0, 0)]
#[case::up(2, false, 3, 1)]
#[case::up_stops_at_the_top(0, false, 3, 0)]
fn the_cursor_stays_in_range(#[case] from: usize, #[case] down: bool, #[case] len: usize, #[case] to: usize) {
    assert_eq!(move_cursor(from, down, len), to);
}

#[test]
fn only_shelved_laptops_can_be_checked_out() {
    let all = [
        laptop(1, "LT-1", "available", None),
        laptop(2, "LT-2", "available", Some(0)),
        laptop(3, "LT-3", "repair", None),
        laptop(4, "LT-4", "retired", None),
    ];
    let got = available_laptop_items(&all);
    assert_eq!(got.iter().map(|i| i.id).collect::<Vec<_>>(), [1]);
    assert_eq!(
        got[0],
        PickItem {
            id: 1,
            label: "LT-1".into(),
            sub: "Dell Latitude".into(),
            search: "lt-1 dell latitude sn1".into(),
            exact: vec!["lt-1".into(), "sn1".into()],
            warning: None,
        }
    );
}

#[rstest]
#[case(0, 0, None)]
#[case(0, 3, None)]
#[case(1, 0, Some("Already has 1 laptop out"))]
#[case(2, 0, Some("Already has 2 laptops out"))]
#[case(2, 1, Some("Has 2 laptops out, 1 overdue"))]
fn borrower_warnings_flag_laptops_already_out(#[case] open: i64, #[case] late: i64, #[case] text: Option<&str>) {
    assert_eq!(borrower_warning(open, late).as_deref(), text);
}

#[test]
fn borrower_items_skip_inactive_people() {
    let got = borrower_items(&[borrower(1, "Amara Okafor", true, 1, 0), borrower(2, "Liam Chen", false, 0, 0)]);
    assert_eq!(
        got,
        vec![PickItem {
            id: 1,
            label: "Amara Okafor".into(),
            sub: "amara.okafor@example.org · Year 11".into(),
            search: "amara okafor amara.okafor@example.org year 11 s1".into(),
            exact: vec!["amara.okafor@example.org".into(), "s1".into()],
            warning: Some("Already has 1 laptop out".into()),
        }]
    );
}

#[test]
fn borrower_items_leave_out_blank_details() {
    let mut b = borrower(1, "Cher", true, 0, 0);
    b.email.clear();
    b.external_id.clear();
    let got = borrower_items(&[b]);
    assert_eq!((got[0].sub.as_str(), got[0].exact.len()), ("Year 11", 0));
}

#[rstest]
fn return_items_describe_who_has_the_laptop(loan: Loan) {
    let got = return_items(&[loan]);
    assert_eq!(got[0].label, "LT-0103");
    assert_eq!(got[0].sub, "Amara Okafor · due Sat 3 Oct");
    assert_eq!(got[0].exact, vec!["lt-0103".to_string(), "pf4a9k2m".to_string()]);
    assert!(got[0].search.contains("amara.okafor@example.org"));
}

// ------------------------------------------------------------ laptops

#[rstest]
#[case(LaptopFilter::InService, [true, true, true, false])]
#[case(LaptopFilter::Shelf, [true, false, false, false])]
#[case(LaptopFilter::Out, [false, true, false, false])]
#[case(LaptopFilter::Late, [false, true, false, false])]
#[case(LaptopFilter::Repair, [false, false, true, false])]
#[case(LaptopFilter::Retired, [false, false, false, true])]
fn laptop_filters_pick_the_right_laptops(#[case] f: LaptopFilter, #[case] expected: [bool; 4]) {
    let all = [
        laptop(1, "LT-1", "available", None),
        laptop(2, "LT-2", "available", Some(now() - 1)),
        laptop(3, "LT-3", "repair", None),
        laptop(4, "LT-4", "retired", None),
    ];
    assert_eq!(all.map(|l| f.keep(&l, now())), expected);
}

#[test]
fn a_laptop_due_now_or_later_is_not_overdue() {
    assert!(!LaptopFilter::Late.keep(&laptop(1, "LT-1", "available", Some(now() + 1)), now()));
    assert!(!LaptopFilter::Late.keep(&laptop(1, "LT-1", "available", Some(now())), now()));
}

#[test]
fn laptop_search_covers_tag_model_serial_and_borrower() {
    let l = laptop(5, "LT-5", "available", Some(0));
    assert!(laptop_matches(&l, "lt-5 latitude sn5 okafor"));
    assert!(!laptop_matches(&l, "hp"));
    assert!(laptop_matches(&laptop(6, "LT-6", "available", None), "lt-6"));
}

#[test]
fn the_shelf_count_ignores_loaned_and_broken_laptops() {
    let all = [
        laptop(1, "LT-1", "available", None),
        laptop(2, "LT-2", "available", None),
        laptop(3, "LT-3", "available", Some(0)),
        laptop(4, "LT-4", "repair", None),
    ];
    assert_eq!(on_shelf_count(&all), 2);
}

#[rstest]
#[case("repair", "LT-1 is in repair.")]
#[case("retired", "LT-1 is retired.")]
#[case("available", "LT-1 is back in service.")]
fn status_messages_read_naturally(#[case] status: &str, #[case] text: &str) {
    assert_eq!(status_message("LT-1", ServiceStatus::parse(status).unwrap()), text);
}

// ------------------------------------------------------------ borrowers

#[rstest]
#[case(BorrowerFilter::Active, [true, true, true, false])]
#[case(BorrowerFilter::WithLaptop, [false, true, true, false])]
#[case(BorrowerFilter::Late, [false, false, true, false])]
#[case(BorrowerFilter::Inactive, [false, false, false, true])]
fn borrower_filters_pick_the_right_people(#[case] f: BorrowerFilter, #[case] expected: [bool; 4]) {
    let all = [
        borrower(1, "A", true, 0, 0),
        borrower(2, "B", true, 1, 0),
        borrower(3, "C", true, 1, 1),
        borrower(4, "D", false, 0, 0),
    ];
    assert_eq!(all.map(|b| f.keep(&b)), expected);
}

#[test]
fn borrower_search_covers_name_email_department_and_id() {
    let b = borrower(9, "Amara Okafor", true, 0, 0);
    assert!(borrower_matches(&b, "okafor year s9 example.org"));
    assert!(!borrower_matches(&b, "liam"));
}

// ------------------------------------------------------------ email

#[rstest]
#[case("overdue", "Overdue")]
#[case("reminder", "Due-soon reminder")]
#[case("receipt", "Check-out receipt")]
#[case("general", "General")]
#[case("something-new", "General")]
fn purposes_have_labels(#[case] purpose: &str, #[case] label: &str) {
    assert_eq!(purpose_label(purpose), label);
}

#[test]
fn overdue_emails_use_the_template_chosen_in_settings() {
    let all = [template(1, "overdue"), template(2, "overdue")];
    assert_eq!(pick_template(&all, "overdue", Some(2)), Some(2));
}

#[test]
fn a_deleted_preferred_template_falls_back_to_the_oldest() {
    let all = [template(4, "overdue"), template(2, "overdue")];
    assert_eq!(pick_template(&all, "overdue", Some(99)), Some(2));
}

#[test]
fn the_preferred_template_only_applies_to_overdue_emails() {
    let all = [template(1, "overdue"), template(3, "receipt")];
    assert_eq!(pick_template(&all, "receipt", Some(1)), Some(3));
}

#[test]
fn with_no_template_for_the_purpose_the_first_is_used() {
    assert_eq!(pick_template(&[template(5, "general"), template(1, "overdue")], "receipt", None), Some(5));
    assert_eq!(pick_template(&[], "receipt", None), None);
}

#[rstest]
#[case("outlook", "Outlook (work)", "_blank")]
#[case("outlook_live", "Outlook.com", "_blank")]
#[case("gmail", "Gmail", "_blank")]
#[case("mailto", "mail app", "_self")]
fn email_apps_have_names_and_open_in_the_right_place(#[case] app: &str, #[case] name: &str, #[case] target: &str) {
    assert_eq!((short_app(app), link_target(app)), (name, target));
}

#[rstest]
#[case::next(0, 3, 1, 1)]
#[case::previous(2, 3, -1, 1)]
#[case::not_past_the_end(2, 3, 1, 2)]
#[case::not_before_the_start(0, 3, -1, 0)]
fn the_email_queue_stays_in_range(#[case] i: usize, #[case] len: usize, #[case] d: isize, #[case] to: usize) {
    assert_eq!(step_index(i, len, d), to);
}

#[rstest]
#[case(0, 1, "Email")]
#[case(0, 3, "Email 1 of 3")]
#[case(2, 3, "Email 3 of 3")]
fn the_queue_label_counts_from_one(#[case] i: usize, #[case] len: usize, #[case] label: &str) {
    assert_eq!(queue_label(i, len), label);
}

#[rstest]
#[case::at_the_caret("Hi ,", 3, 3, ("Hi {{first_name}},", 17))]
#[case::over_a_selection("Hi NAME,", 3, 7, ("Hi {{first_name}},", 17))]
#[case::caret_past_the_end("Hi", 50, 50, ("Hi{{first_name}}", 16))]
#[case::end_before_start("Hi there", 3, 1, ("Hi {{first_name}}there", 17))]
#[case::after_an_emoji("👋 ", 3, 3, ("👋 {{first_name}}", 17))]
fn tokens_are_inserted_at_the_cursor(
    #[case] text: &str,
    #[case] start: usize,
    #[case] end: usize,
    #[case] expected: (&str, usize),
) {
    let (out, caret) = insert_token(text, start, end, "first_name");
    assert_eq!((out.as_str(), caret), expected);
}

proptest! {
    #[test]
    fn inserting_a_token_keeps_the_rest_of_the_text(text in "\\PC{0,40}", a in 0usize..60, b in 0usize..60) {
        let (out, caret) = insert_token(&text, a, b, "x");
        prop_assert!(out.contains("{{x}}"));
        prop_assert_eq!(out.encode_utf16().count(), text.encode_utf16().count() + 5 - (b.min(text.encode_utf16().count()).saturating_sub(a.min(text.encode_utf16().count()))));
        prop_assert!(caret <= out.encode_utf16().count());
    }
}

#[test]
fn the_sample_loan_is_three_days_late() {
    let l = sample_loan(now());
    assert_eq!(time::days_late(l.due_at, now()), 3);
    assert_eq!(time::calendar_days(l.out_at, now()), 10);
}

// ------------------------------------------------------------ extracted from the views

#[rstest]
fn filter_loans_applies_the_filter_and_the_search(loan: Loan) {
    let mut other = with_due(loan.clone(), 2, now() + DAY);
    other.borrower_name = "Liam Chen".into();
    let loans = [with_due(loan, 1, now() - DAY), other];
    let ids = |v: Vec<Loan>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(ids(filter_loans(&loans, LoanFilter::Open, "", now())), [1, 2]);
    assert_eq!(ids(filter_loans(&loans, LoanFilter::Open, "liam", now())), [2]);
    assert_eq!(ids(filter_loans(&loans, LoanFilter::Late, "liam", now())), Vec::<i64>::new());
}

#[rstest]
fn find_loan_looks_up_by_id(loan: Loan) {
    let loans = [with_due(loan.clone(), 1, 0), with_due(loan, 2, 5)];
    assert_eq!(find_loan(&loans, 2).map(|l| l.due_at), Some(5));
    assert_eq!(find_loan(&loans, 3), None);
}

#[rstest]
fn a_receipt_is_offered_only_with_an_email_address(mut loan: Loan) {
    assert_eq!(
        receipt_action(&loan),
        Some(("Email receipt".to_string(), ToastAction::EmailLoan { loan_id: 1, purpose: "receipt" }))
    );
    loan.borrower_email.clear();
    assert_eq!(receipt_action(&loan), None);
}

#[test]
fn filter_laptops_applies_the_filter_and_the_search() {
    let all = [
        laptop(1, "LT-1", "available", None),
        laptop(2, "LT-2", "repair", None),
        laptop(3, "LT-30", "available", None),
    ];
    let ids = |v: Vec<Laptop>| v.into_iter().map(|l| l.id).collect::<Vec<_>>();
    assert_eq!(ids(filter_laptops(&all, LaptopFilter::Shelf, "", now())), [1, 3]);
    assert_eq!(ids(filter_laptops(&all, LaptopFilter::Shelf, "lt-3", now())), [3]);
    assert_eq!(ids(filter_laptops(&all, LaptopFilter::Repair, "lt-3", now())), Vec::<i64>::new());
}

#[rstest]
#[case("available", Some(1), LaptopStatus::OnLoan)]
#[case("repair", None, LaptopStatus::Repair)]
#[case("retired", None, LaptopStatus::Retired)]
#[case("available", None, LaptopStatus::Shelf)]
fn each_laptop_shows_one_status(#[case] status: &str, #[case] due: Option<i64>, #[case] shown: LaptopStatus) {
    assert_eq!(laptop_status(&laptop(1, "LT-1", status, due)), shown);
}

#[test]
fn filter_borrowers_applies_the_filter_and_the_search() {
    let all = [
        borrower(1, "Amara Okafor", true, 0, 0),
        borrower(2, "Liam Chen", true, 1, 0),
        borrower(3, "Old Leaver", false, 0, 0),
    ];
    let ids = |v: Vec<Borrower>| v.into_iter().map(|b| b.id).collect::<Vec<_>>();
    assert_eq!(ids(filter_borrowers(&all, BorrowerFilter::Active, "")), [1, 2]);
    assert_eq!(ids(filter_borrowers(&all, BorrowerFilter::Active, "chen")), [2]);
    assert_eq!(ids(filter_borrowers(&all, BorrowerFilter::Inactive, "chen")), Vec::<i64>::new());
}

#[rstest]
#[case(0, 0, "muted", "None")]
#[case(0, 2, "muted", "None")]
#[case(2, 0, "pill", "2 out")]
#[case(2, 1, "pill late", "2 out · 1 late")]
fn the_laptops_out_cell_highlights_late_ones(
    #[case] open: i64,
    #[case] late: i64,
    #[case] class: &str,
    #[case] text: &str,
) {
    assert_eq!(loans_out_label(open, late), (class, text.to_string()));
}

#[rstest]
#[case(true, true, true)]
#[case(true, false, false)]
#[case(false, true, false)]
#[case(false, false, false)]
fn the_form_stays_open_only_when_adding_another(#[case] adding: bool, #[case] another: bool, #[case] open: bool) {
    assert_eq!(keep_form_open(adding, another), open);
}

#[test]
fn find_template_looks_up_by_id() {
    let all = [template(1, "overdue"), template(2, "receipt")];
    assert_eq!(find_template(&all, Some(2)).map(|t| t.purpose), Some("receipt".into()));
    assert_eq!(find_template(&all, Some(3)), None);
    assert_eq!(find_template(&all, None), None);
}

#[rstest]
#[case::unchanged("T1", "", "", "overdue", false)]
#[case::name("Renamed", "", "", "overdue", true)]
#[case::subject("T1", "New subject", "", "overdue", true)]
#[case::body("T1", "", "New body", "overdue", true)]
#[case::purpose("T1", "", "", "general", true)]
fn any_edit_counts_as_a_change(
    #[case] name: &str,
    #[case] subject: &str,
    #[case] body: &str,
    #[case] purpose: &str,
    #[case] changed: bool,
) {
    assert_eq!(template_changed(&template(1, "overdue"), name, subject, body, purpose), changed);
}

#[rstest]
#[case(true, "default", true)]
#[case(false, "default", false)]
#[case(true, "granted", false)]
#[case(true, "denied", false)]
#[case(true, "unsupported", false)]
fn the_notification_nudge_shows_until_the_browser_has_asked(
    #[case] enabled: bool,
    #[case] permission: &str,
    #[case] show: bool,
) {
    assert_eq!(show_notify_nudge(enabled, permission), show);
}

// ------------------------------------------------------------ settings

#[rstest]
#[case("7", Ok(7))]
#[case(" 14 ", Ok(14))]
#[case("1", Ok(1))]
#[case("365", Ok(365))]
#[case("0", Err(()))]
#[case("366", Err(()))]
#[case("-3", Err(()))]
#[case("a week", Err(()))]
fn loan_days_must_be_between_one_and_365(#[case] input: &str, #[case] expected: Result<i64, ()>) {
    assert_eq!(parse_loan_days(input).map_err(|_| ()), expected);
}

#[test]
fn a_bad_loan_period_explains_the_limits() {
    assert_eq!(parse_loan_days("0"), Err("Enter a number of days between 1 and 365.".into()));
}

#[rstest]
#[case("AbortError: The user aborted a request.", true)]
#[case("The operation was aborted.", true)]
#[case("User cancelled", true)]
#[case("NotAllowedError: permission denied", false)]
fn dismissing_a_file_picker_is_not_an_error(#[case] err: &str, #[case] cancel: bool) {
    assert_eq!(is_cancel(err), cancel);
}

#[test]
fn backups_are_named_by_date() {
    assert_eq!(backup_name(now()), "laptop-library-2026-10-06.sqlite");
}
