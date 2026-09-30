//! Every loan, open and returned, with filters and search.

use leptos::prelude::*;

use crate::repo;
use crate::ui::components::{Chips, LoanLedger};
use crate::ui::state::use_app;
use crate::view_model::{self as vm, LoanFilter};

#[component]
pub fn LoansPage() -> impl IntoView {
    let st = use_app();
    let filter = RwSignal::new(LoanFilter::Open);
    let search = RwSignal::new(String::new());
    let all = Memo::new(move |_| {
        st.rev.track();
        repo::all_loans()
    });
    let counts = Signal::derive(move || {
        let now = st.clock.get();
        all.with(|list| vm::counts(list, &LoanFilter::OPTIONS, |f, l| f.keep(l, now)))
    });
    let shown = Signal::derive(move || {
        let now = st.clock.get();
        let (q, f) = (search.get(), filter.get());
        all.with(|list| vm::filter_loans(list, f, &q, now))
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"Loan history"</p>
                <h1>"Loans"</h1>
            </div>
        </header>
        <div class="toolbar">
            <Chips options=LoanFilter::OPTIONS.to_vec() value=filter counts=counts />
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
