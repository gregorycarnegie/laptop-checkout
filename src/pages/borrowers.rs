//! People who can borrow laptops.

use leptos::prelude::*;

use crate::components::{Chips, ConfirmButton};
use crate::models::{Borrower, BorrowerInput};
use crate::pages::import::{CsvImport, ImportKind};
use crate::repo;
use crate::state::use_app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Filter {
    Active,
    WithLaptop,
    Late,
    Inactive,
}

impl Filter {
    fn keep(self, b: &Borrower) -> bool {
        match self {
            Filter::Active => b.is_active(),
            Filter::WithLaptop => b.open_loans > 0,
            Filter::Late => b.late_loans > 0,
            Filter::Inactive => !b.is_active(),
        }
    }
}

const FILTERS: [(Filter, &str); 4] = [
    (Filter::Active, "Active"),
    (Filter::WithLaptop, "Has a laptop"),
    (Filter::Late, "Overdue"),
    (Filter::Inactive, "Inactive"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Panel {
    None,
    Add,
    Edit(i64),
    Import,
}

#[component]
pub fn BorrowersPage() -> impl IntoView {
    let st = use_app();
    let panel = RwSignal::new(Panel::None);
    let filter = RwSignal::new(Filter::Active);
    let search = RwSignal::new(String::new());
    let all = Memo::new(move |_| {
        st.rev.track();
        st.clock.track();
        repo::borrowers()
    });
    let counts = Signal::derive(move || {
        let list = all.get();
        FILTERS.iter().map(|(f, _)| list.iter().filter(|b| f.keep(b)).count()).collect::<Vec<_>>()
    });
    let shown = Memo::new(move |_| {
        let q = search.get().trim().to_lowercase();
        let f = filter.get();
        all.get()
            .into_iter()
            .filter(|b| f.keep(b))
            .filter(|b| {
                q.is_empty()
                    || format!("{} {} {} {}", b.name, b.email, b.department, b.external_id)
                        .to_lowercase()
                        .contains(&q)
            })
            .collect::<Vec<_>>()
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"People"</p>
                <h1>"Borrowers"</h1>
            </div>
            <div class="row wrap">
                <button type="button" class="btn" on:click=move |_| panel.set(Panel::Import)>"Import CSV"</button>
                <button type="button" class="btn primary" on:click=move |_| panel.set(Panel::Add)>"Add borrower"</button>
            </div>
        </header>

        {move || match panel.get() {
            Panel::None => ().into_any(),
            Panel::Import => view! { <CsvImport kind=ImportKind::Borrowers on_close=move || panel.set(Panel::None) /> }.into_any(),
            Panel::Add => view! { <BorrowerForm id=None on_done=move || panel.set(Panel::None) /> }.into_any(),
            Panel::Edit(id) => view! { <BorrowerForm id=Some(id) on_done=move || panel.set(Panel::None) /> }.into_any(),
        }}

        <div class="toolbar">
            <Chips options=FILTERS.to_vec() value=filter counts=counts />
            <input id="borrower-search" class="input search" type="search" placeholder="Search name, email, ID" bind:value=search />
        </div>

        <section class="panel">
            <div class="table-wrap">
                <table class="table">
                    <thead>
                        <tr>
                            <th>"Name"</th>
                            <th>"Email"</th>
                            <th>"Department"</th>
                            <th>"ID"</th>
                            <th>"Laptops out"</th>
                            <th class="right">"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let list = shown.get();
                            if list.is_empty() {
                                return view! { <tr><td colspan="6" class="empty">"No borrowers match. Add someone, or import a CSV exported from your school or HR system."</td></tr> }.into_any();
                            }
                            list.into_iter().map(|b| view! { <BorrowerRow borrower=b panel=panel /> }).collect_view().into_any()
                        }}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn BorrowerRow(borrower: Borrower, panel: RwSignal<Panel>) -> impl IntoView {
    let st = use_app();
    let id = borrower.id;
    let active = borrower.is_active();
    let has_history = borrower.total_loans > 0;
    let late = borrower.late_loans;
    let out = match (borrower.open_loans, late) {
        (0, _) => view! { <span class="muted">"None"</span> }.into_any(),
        (n, 0) => view! { <span class="pill">{n}" out"</span> }.into_any(),
        (n, l) => view! { <span class="pill late">{n}" out · "{l}" late"</span> }.into_any(),
    };
    view! {
        <tr class:dim=!active>
            <td>
                <span class="who">{borrower.name.clone()}</span>
                {(!borrower.notes.is_empty()).then(|| view! { <div class="muted small">{borrower.notes.clone()}</div> })}
            </td>
            <td class="small">{if borrower.email.is_empty() { "—".to_string() } else { borrower.email.clone() }}</td>
            <td>{borrower.department.clone()}</td>
            <td class="mono small">{borrower.external_id.clone()}</td>
            <td>{out}</td>
            <td class="right">
                <div class="row end">
                    {(late > 0).then(|| view! {
                        <button
                            type="button"
                            class="btn small late-btn"
                            on:click=move |_| {
                                let ids = repo::open_loans().into_iter()
                                    .filter(|l| l.borrower_id == id && l.due_at < crate::time::now())
                                    .map(|l| l.id).collect();
                                st.email_loans(ids, "overdue");
                            }
                        >
                            "Email"
                        </button>
                    })}
                    <button type="button" class="btn ghost small" on:click=move |_| panel.set(Panel::Edit(id))>"Edit"</button>
                    <button
                        type="button"
                        class="btn ghost small"
                        on:click=move |_| {
                            st.report(
                                repo::set_borrower_active(id, !active),
                                if active { "Borrower deactivated. They won't appear when checking out." } else { "Borrower reactivated." },
                            );
                        }
                    >
                        {if active { "Deactivate" } else { "Reactivate" }}
                    </button>
                    {(!has_history).then(|| view! {
                        <ConfirmButton
                            label="Delete"
                            confirm="Delete?"
                            class="ghost small"
                            on_confirm=move || { st.report(repo::delete_borrower(id), "Borrower deleted."); }
                        />
                    })}
                </div>
            </td>
        </tr>
    }
}

#[component]
fn BorrowerForm(id: Option<i64>, on_done: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let st = use_app();
    let existing = id.and_then(|id| repo::borrowers().into_iter().find(|b| b.id == id));
    let pick = |f: fn(&Borrower) -> String| existing.as_ref().map(f).unwrap_or_default();
    let name = RwSignal::new(pick(|b| b.name.clone()));
    let email = RwSignal::new(pick(|b| b.email.clone()));
    let department = RwSignal::new(pick(|b| b.department.clone()));
    let external_id = RwSignal::new(pick(|b| b.external_id.clone()));
    let phone = RwSignal::new(pick(|b| b.phone.clone()));
    let notes = RwSignal::new(pick(|b| b.notes.clone()));
    let error = RwSignal::new(None::<String>);
    let add_another = RwSignal::new(false);
    let done = on_done.clone();

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let input = BorrowerInput {
            name: name.get_untracked(),
            email: email.get_untracked(),
            department: department.get_untracked(),
            external_id: external_id.get_untracked(),
            phone: phone.get_untracked(),
            notes: notes.get_untracked(),
        };
        let r = match id {
            Some(id) => repo::update_borrower(id, &input),
            None => repo::add_borrower(&input).map(|_| ()),
        };
        match r {
            Ok(()) => {
                st.ok(format!("{} saved.", input.name.trim()));
                if id.is_none() && add_another.get_untracked() {
                    for s in [name, email, external_id, phone, notes] {
                        s.set(String::new());
                    }
                    error.set(None);
                    request_animation_frame(|| crate::components::focus("borrower-name"));
                } else {
                    done();
                }
            }
            Err(e) => error.set(Some(e)),
        }
    };

    view! {
        <form class="panel form-panel" on:submit=submit>
            <div class="panel-head">
                <h2>{if id.is_some() { "Edit borrower" } else { "Add a borrower" }}</h2>
                <button type="button" class="btn ghost small" on:click=move |_| on_done()>"Cancel"</button>
            </div>
            <div class="form-grid">
                <div class="field">
                    <label class="label" for="borrower-name">"Full name"</label>
                    <input id="borrower-name" class="input" type="text" required autofocus bind:value=name />
                </div>
                <div class="field">
                    <label class="label" for="borrower-email">"Email"</label>
                    <input id="borrower-email" class="input" type="email" placeholder="Used for reminders" bind:value=email />
                </div>
                <div class="field">
                    <label class="label" for="borrower-dept">"Department, class or team"</label>
                    <input id="borrower-dept" class="input" type="text" bind:value=department />
                </div>
                <div class="field">
                    <label class="label" for="borrower-ext">"ID or card number"</label>
                    <input id="borrower-ext" class="input mono" type="text" placeholder="Scan a card to fill" bind:value=external_id />
                </div>
                <div class="field">
                    <label class="label" for="borrower-phone">"Phone"</label>
                    <input id="borrower-phone" class="input" type="tel" bind:value=phone />
                </div>
                <div class="field">
                    <label class="label" for="borrower-notes">"Notes"</label>
                    <input id="borrower-notes" class="input" type="text" bind:value=notes />
                </div>
            </div>
            {move || error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <div class="row wrap">
                <button type="submit" class="btn primary">{if id.is_some() { "Save changes" } else { "Add borrower" }}</button>
                {id.is_none().then(|| view! {
                    <label class="check">
                        <input id="borrower-another" type="checkbox" bind:checked=add_another />
                        " Add another after this one"
                    </label>
                })}
            </div>
        </form>
    }
}
