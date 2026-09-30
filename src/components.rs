//! Reusable pieces: date stamps, pickers, the loan ledger and small buttons.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::db;
use crate::models::Loan;
use crate::repo;
use crate::state::use_app;
use crate::time::{self, DueState};

/// Library-card style due-date stamp.
#[component]
pub fn DueStamp(due: i64, #[prop(default = None)] returned: Option<i64>) -> impl IntoView {
    let st = use_app();
    view! {
        {move || {
            let now = st.clock.get();
            let state = DueState::of(due, returned, now);
            let (headline, detail) = match returned {
                Some(r) => {
                    let late = time::days_late(due, r);
                    let note = if late > 0 { format!("{} late", time::plural(late, "day")) } else { "on time".into() };
                    (format!("In {}", time::short(r)), note)
                }
                None => (time::describe_due(due, now), format!("due {}", time::short(due))),
            };
            view! {
                <span class=format!("stamp {}", state.class())>
                    <span class="stamp-main">{headline}</span>
                    <span class="stamp-sub">{detail}</span>
                </span>
            }
        }}
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickItem {
    pub id: i64,
    pub label: String,
    pub sub: String,
    /// Lower-case text searched when typing.
    pub search: String,
    /// Lower-case values that select this item outright on Enter (asset tag, email, ID).
    pub exact: Vec<String>,
    pub warning: Option<String>,
}

/// Type-to-search picker. Barcode scanners work too: they type the tag and press Enter.
#[component]
pub fn Picker(
    #[prop(into)] items: Signal<Vec<PickItem>>,
    selected: RwSignal<Option<i64>>,
    #[prop(into)] placeholder: String,
    input_id: &'static str,
    #[prop(into)] empty: String,
) -> impl IntoView {
    let placeholder = StoredValue::new(placeholder);
    let empty = StoredValue::new(empty);
    let text = RwSignal::new(String::new());
    let open = RwSignal::new(false);
    let cursor = RwSignal::new(0usize);

    let matches = Memo::new(move |_| {
        let q = text.get().trim().to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        items
            .get()
            .into_iter()
            .filter(|i| words.iter().all(|w| i.search.contains(w)))
            .take(8)
            .collect::<Vec<_>>()
    });
    let chosen = Memo::new(move |_| {
        selected
            .get()
            .and_then(|id| items.get().into_iter().find(|i| i.id == id))
    });

    let pick = move |id: i64| {
        selected.set(Some(id));
        text.set(String::new());
        open.set(false);
    };

    let on_key = move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
        "Enter" => {
            ev.prevent_default();
            let q = text.get_untracked().trim().to_lowercase();
            if q.is_empty() {
                return;
            }
            let all = items.get_untracked();
            if let Some(hit) = all.iter().find(|i| i.exact.iter().any(|e| *e == q)) {
                pick(hit.id);
            } else if let Some(hit) = matches.get_untracked().get(cursor.get_untracked()) {
                pick(hit.id);
            }
        }
        "ArrowDown" => {
            ev.prevent_default();
            open.set(true);
            let n = matches.get_untracked().len();
            if n > 0 {
                cursor.update(|c| *c = (*c + 1).min(n - 1));
            }
        }
        "ArrowUp" => {
            ev.prevent_default();
            cursor.update(|c| *c = c.saturating_sub(1));
        }
        "Escape" => open.set(false),
        _ => {}
    };

    view! {
        <div class="picker">
            {move || match chosen.get() {
                Some(item) => view! {
                    <div class="picked">
                        <div class="picked-text">
                            <span class="picked-label">{item.label.clone()}</span>
                            <span class="picked-sub">{item.sub.clone()}</span>
                            {item.warning.clone().map(|w| view! { <span class="picked-warn">{w}</span> })}
                        </div>
                        <button
                            type="button"
                            class="btn ghost small"
                            on:click=move |_| {
                                selected.set(None);
                                request_animation_frame(move || focus(input_id));
                            }
                        >
                            "Change"
                        </button>
                    </div>
                }
                .into_any(),
                None => view! {
                    <input
                        id=input_id
                        class="input"
                        type="text"
                        autocomplete="off"
                        spellcheck="false"
                        placeholder=placeholder.get_value()
                        prop:value=move || text.get()
                        on:input=move |ev| {
                            text.set(event_target_value(&ev));
                            cursor.set(0);
                            open.set(true);
                        }
                        on:focus=move |_| open.set(true)
                        on:blur=move |_| open.set(false)
                        on:keydown=on_key
                    />
                    <Show when=move || open.get()>
                        <ul class="picker-list" role="listbox">
                            {move || {
                                let list = matches.get();
                                if list.is_empty() {
                                    return view! { <li class="picker-empty">{empty.get_value()}</li> }.into_any();
                                }
                                list.into_iter()
                                    .enumerate()
                                    .map(|(i, m)| {
                                        let id = m.id;
                                        view! {
                                            <li>
                                                <button
                                                    type="button"
                                                    class="picker-option"
                                                    class:active=move || cursor.get() == i
                                                    on:mousedown=move |ev| {
                                                        ev.prevent_default();
                                                        pick(id);
                                                    }
                                                >
                                                    <span class="picked-label">{m.label}</span>
                                                    <span class="picked-sub">{m.sub}</span>
                                                </button>
                                            </li>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }}
                        </ul>
                    </Show>
                }
                .into_any(),
            }}
        </div>
    }
}

pub fn focus(id: &str) {
    use wasm_bindgen::JsCast;
    if let Some(el) = document()
        .get_element_by_id(id)
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// A button that asks "Sure?" in place before running a destructive action.
#[component]
pub fn ConfirmButton(
    #[prop(into)] label: String,
    #[prop(into)] confirm: String,
    #[prop(optional, into)] class: String,
    on_confirm: impl Fn() + 'static,
) -> impl IntoView {
    let armed = RwSignal::new(false);
    view! {
        <button
            type="button"
            class=move || format!("btn {} {}", class, if armed.get() { "danger" } else { "" })
            on:click=move |_| {
                if armed.get_untracked() {
                    armed.set(false);
                    on_confirm();
                } else {
                    armed.set(true);
                    set_timeout(move || armed.set(false), Duration::from_secs(4));
                }
            }
        >
            {move || if armed.get() { confirm.clone() } else { label.clone() }}
        </button>
    }
}

#[component]
pub fn CopyButton(#[prop(into)] text: Signal<String>, #[prop(into)] label: String) -> impl IntoView {
    let done = RwSignal::new(false);
    view! {
        <button
            type="button"
            class="btn ghost small"
            on:click=move |_| {
                let t = text.get_untracked();
                spawn_local(async move {
                    if db::copy_text(&t).await {
                        done.set(true);
                        set_timeout(move || done.set(false), Duration::from_millis(1600));
                    }
                });
            }
        >
            {move || if done.get() { "Copied".to_string() } else { label.clone() }}
        </button>
    }
}

/// Filter chips with counts.
#[component]
pub fn Chips<T>(options: Vec<(T, &'static str)>, value: RwSignal<T>, #[prop(into)] counts: Signal<Vec<usize>>) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div class="chips" role="tablist">
            {options
                .into_iter()
                .enumerate()
                .map(|(i, (v, label))| {
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class="chip"
                            class:on=move || value.get() == v
                            aria-selected=move || (value.get() == v).to_string()
                            on:click=move |_| value.set(v)
                        >
                            {label}
                            <span class="chip-count">{move || counts.get().get(i).copied().unwrap_or(0)}</span>
                        </button>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// The loan ledger: one row per loan with its stamp and actions.
#[component]
pub fn LoanLedger(#[prop(into)] loans: Signal<Vec<Loan>>, #[prop(into)] empty: String) -> impl IntoView {
    view! {
        <div class="ledger">
            {move || {
                let list = loans.get();
                if list.is_empty() {
                    return view! { <p class="empty">{empty.clone()}</p> }.into_any();
                }
                list.into_iter().map(|l| view! { <LoanRow loan=l /> }).collect_view().into_any()
            }}
        </div>
    }
}

#[component]
fn LoanRow(loan: Loan) -> impl IntoView {
    let st = use_app();
    let id = loan.id;
    let open = loan.returned_at.is_none();
    let emailed = (loan.emails_sent > 0).then(|| {
        format!(
            "Emailed {}× · last {}",
            loan.emails_sent,
            loan.last_emailed_at.map(time::short).unwrap_or_default()
        )
    });
    let renewed = (loan.renewals > 0).then(|| format!("Renewed {}", time::plural(loan.renewals, "time")));
    let tag = loan.asset_tag.clone();
    let who = loan.borrower_name.clone();
    let has_email = !loan.borrower_email.is_empty();
    let is_late = open && loan.due_at < time::now();
    view! {
        <article class="ledger-row" class:is-late=is_late>
            <div class="ledger-item">
                <span class="tag">{loan.asset_tag.clone()}</span>
                <span class="muted">{loan.model.clone()}</span>
            </div>
            <div class="ledger-who">
                <span class="who">{loan.borrower_name.clone()}</span>
                <span class="muted">
                    {if loan.borrower_email.is_empty() { "No email on file".to_string() } else { loan.borrower_email.clone() }}
                    {(!loan.department.is_empty()).then(|| format!(" · {}", loan.department))}
                </span>
            </div>
            <div class="ledger-dates">
                <DueStamp due=loan.due_at returned=loan.returned_at />
                <span class="muted small">"Out " {time::short(loan.out_at)}</span>
            </div>
            <div class="ledger-meta muted small">
                {emailed}
                {renewed.map(|r| view! { <span>{r}</span> })}
                {(!loan.note.is_empty()).then(|| view! { <span class="note">{loan.note.clone()}</span> })}
            </div>
            <div class="ledger-actions">
                {open
                    .then(|| {
                        let tag2 = tag.clone();
                        let who2 = who.clone();
                        view! {
                            <button
                                type="button"
                                class="btn small"
                                disabled=!has_email
                                title=if has_email { "Write an email to this borrower" } else { "Add an email address to this borrower first" }
                                on:click=move |_| st.email_loans(vec![id], if is_late { "overdue" } else { "reminder" })
                            >
                                "Email"
                            </button>
                            <button
                                type="button"
                                class="btn small ghost"
                                title="Extend the due date by the standard loan period"
                                on:click=move |_| {
                                    let days = st.settings.get_untracked().loan_days;
                                    match repo::renew(id, days) {
                                        Ok(due) => st.ok(format!("Renewed {}. Now due {}.", tag2, time::short(due))),
                                        Err(e) => st.error(e),
                                    }
                                }
                            >
                                "Renew"
                            </button>
                            <button
                                type="button"
                                class="btn small primary"
                                on:click=move |_| {
                                    match repo::check_in(id, "") {
                                        Ok(l) => st.ok(crate::pages::desk::returned_message(&l, &who2)),
                                        Err(e) => st.error(e),
                                    }
                                }
                            >
                                "Check in"
                            </button>
                        }
                    })}
            </div>
        </article>
    }
}
