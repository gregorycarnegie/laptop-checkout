//! Every loan, open and returned, with filters and search.

use leptos::prelude::*;

use crate::components::{Chips, LoanLedger};
use crate::models::Loan;
use crate::repo;
use crate::state::use_app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Filter {
    Open,
    Late,
    Returned,
    All,
}

impl Filter {
    fn keep(self, l: &Loan, now: i64) -> bool {
        match self {
            Filter::Open => l.returned_at.is_none(),
            Filter::Late => l.returned_at.is_none() && l.due_at < now,
            Filter::Returned => l.returned_at.is_some(),
            Filter::All => true,
        }
    }
}

const FILTERS: [(Filter, &str); 4] = [
    (Filter::Open, "Out now"),
    (Filter::Late, "Overdue"),
    (Filter::Returned, "Returned"),
    (Filter::All, "All"),
];

#[component]
pub fn LoansPage() -> impl IntoView {
    let st = use_app();
    let filter = RwSignal::new(Filter::Open);
    let search = RwSignal::new(String::new());
    let all = Memo::new(move |_| {
        st.rev.track();
        repo::all_loans()
    });
    let counts = Signal::derive(move || {
        let now = st.clock.get();
        let list = all.get();
        FILTERS
            .iter()
            .map(|(f, _)| list.iter().filter(|l| f.keep(l, now)).count())
            .collect::<Vec<_>>()
    });
    let shown = Signal::derive(move || {
        let now = st.clock.get();
        let q = search.get().trim().to_lowercase();
        let f = filter.get();
        all.get()
            .into_iter()
            .filter(|l| f.keep(l, now))
            .filter(|l| {
                q.is_empty()
                    || format!("{} {} {} {} {}", l.asset_tag, l.model, l.borrower_name, l.borrower_email, l.department)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect::<Vec<_>>()
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"Loan history"</p>
                <h1>"Loans"</h1>
            </div>
        </header>
        <div class="toolbar">
            <Chips options=FILTERS.to_vec() value=filter counts=counts />
            <input
                id="loan-search"
                class="input search"
                type="search"
                placeholder="Search tag, name, email"
                bind:value=search
            />
        </div>
        <section class="panel">
            <LoanLedger loans=shown empty="No loans match." />
        </section>
    }
}
