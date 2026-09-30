//! The circulation desk: check laptops out and in, and see what's late.

use leptos::prelude::*;

use crate::repo;
use crate::time;
use crate::ui::components::{focus, DueStamp, LoanLedger, Picker};
use crate::ui::state::use_app;
use crate::view_model::{self as vm, Page, ToastKind};

#[component]
pub fn DeskPage() -> impl IntoView {
    let st = use_app();
    let open = Memo::new(move |_| {
        st.rev.track();
        repo::open_loans()
    });
    let late = Memo::new(move |_| open.with(|o| vm::late(o, st.clock.get())));
    let soon = Memo::new(move |_| open.with(|o| vm::due_soon(o, st.clock.get())));
    let shelf = Memo::new(move |_| {
        st.rev.track();
        vm::on_shelf_count(&repo::laptops())
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">{move || time::long(st.clock.get())}</p>
                <h1>"Circulation desk"</h1>
            </div>
        </header>

        <NotifyNudge />

        <section class="tally" aria-label="Today at a glance">
            <button type="button" class="tally-item" on:click=move |_| st.go(Page::Loans)>
                <span class="tally-n">{move || open.with(Vec::len)}</span>
                <span class="tally-label">"On loan"</span>
            </button>
            <a class="tally-item late" class:zero=move || late.with(Vec::is_empty) href="#overdue">
                <span class="tally-n">{move || late.with(Vec::len)}</span>
                <span class="tally-label">"Overdue"</span>
            </a>
            <a class="tally-item soon" class:zero=move || soon.with(Vec::is_empty) href="#due-soon">
                <span class="tally-n">{move || soon.with(Vec::len)}</span>
                <span class="tally-label">"Due today or tomorrow"</span>
            </a>
            <button type="button" class="tally-item" on:click=move |_| st.go(Page::Laptops)>
                <span class="tally-n">{move || shelf.get()}</span>
                <span class="tally-label">"On the shelf"</span>
            </button>
        </section>

        <div class="desk-grid">
            <CheckOut />
            <CheckIn />
        </div>

        <section class="panel" id="overdue">
            <div class="panel-head">
                <h2>"Overdue"</h2>
                <Show when=move || !late.with(Vec::is_empty)>
                    <button
                        type="button"
                        class="btn late-btn"
                        on:click=move |_| st.email_loans(late.with_untracked(|l| vm::emailable(l)), "overdue")
                    >
                        {move || format!("Email all {} late borrowers", late.with(Vec::len))}
                    </button>
                </Show>
            </div>
            <LoanLedger loans=late empty="Nothing is overdue. Every laptop out is still within its loan period." />
        </section>

        <section class="panel" id="due-soon">
            <div class="panel-head">
                <h2>"Due today or tomorrow"</h2>
                <Show when=move || !soon.with(Vec::is_empty)>
                    <button
                        type="button"
                        class="btn"
                        on:click=move |_| st.email_loans(soon.with_untracked(|l| vm::emailable(l)), "reminder")
                    >
                        "Send reminders"
                    </button>
                </Show>
            </div>
            <LoanLedger loans=soon empty="No laptops are due back today or tomorrow." />
        </section>
    }
}

#[component]
fn NotifyNudge() -> impl IntoView {
    let st = use_app();
    let show = move || vm::show_notify_nudge(st.settings.get().notify_enabled, &st.notify_permission.get());
    view! {
        <Show when=show>
            <div class="nudge">
                <p>
                    <strong>"Get a desktop alert when a laptop is overdue."</strong>
                    " Alerts appear while this page is open in a tab."
                </p>
                <button type="button" class="btn primary" on:click=move |_| crate::web::notify::request(st)>
                    "Turn on notifications"
                </button>
            </div>
        </Show>
    }
}

#[component]
fn CheckOut() -> impl IntoView {
    let st = use_app();
    let laptop = RwSignal::new(None::<i64>);
    let borrower = RwSignal::new(None::<i64>);
    let due = RwSignal::new(time::to_input(time::due_in_days(st.settings.get_untracked().loan_days)));
    let note = RwSignal::new(String::new());
    let error = RwSignal::new(None::<String>);

    let laptops = Memo::new(move |_| {
        st.rev.track();
        vm::available_laptop_items(&repo::laptops())
    });
    let people = Memo::new(move |_| {
        st.rev.track();
        vm::borrower_items(&repo::borrowers())
    });

    let set_days = move |days: i64| due.set(time::to_input(time::due_in_days(days)));

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        error.set(None);
        let (Some(lap), Some(who)) = (laptop.get_untracked(), borrower.get_untracked()) else {
            error.set(Some("Choose a laptop and a borrower.".into()));
            return;
        };
        let Some(due_at) = time::from_input(&due.get_untracked()) else {
            error.set(Some("Pick a due date.".into()));
            return;
        };
        match repo::check_out(lap, who, due_at, &note.get_untracked()).map(repo::loan) {
            Ok(Some(l)) => {
                st.toast_with(ToastKind::Ok, vm::checked_out_message(&l), vm::receipt_action(&l));
                laptop.set(None);
                borrower.set(None);
                note.set(String::new());
                request_animation_frame(|| focus("out-laptop"));
            }
            Ok(None) => {}
            Err(e) => error.set(Some(e)),
        }
    };

    view! {
        <form class="panel slip" on:submit=submit>
            <div class="panel-head">
                <h2>"Check out"</h2>
            </div>
            <div class="field">
                <label class="label" for="out-laptop">"Laptop"</label>
                <Picker
                    items=laptops
                    selected=laptop
                    placeholder="Scan or type an asset tag"
                    input_id="out-laptop"
                    empty="No available laptop matches. It may be on loan, in repair or retired."
                />
            </div>
            <div class="field">
                <label class="label" for="out-borrower">"Borrower"</label>
                <Picker
                    items=people
                    selected=borrower
                    placeholder="Name, email or ID number"
                    input_id="out-borrower"
                    empty="No borrower matches. Add them on the Borrowers page."
                />
            </div>
            <div class="field">
                <label class="label" for="out-due">"Due back"</label>
                <div class="row wrap">
                    <input id="out-due" class="input date" type="date" min=time::to_input(time::now()) bind:value=due />
                    <div class="quick">
                        <button type="button" class="btn ghost small" on:click=move |_| set_days(1)>"1 day"</button>
                        <button type="button" class="btn ghost small" on:click=move |_| set_days(7)>"1 week"</button>
                        <button type="button" class="btn ghost small" on:click=move |_| set_days(14)>"2 weeks"</button>
                        <button type="button" class="btn ghost small" on:click=move |_| set_days(28)>"4 weeks"</button>
                    </div>
                </div>
                <p class="hint">
                    {move || {
                        time::from_input(&due.get())
                            .map(|d| format!("Due by the end of {}.", time::long(d)))
                            .unwrap_or_else(|| "Pick a date.".into())
                    }}
                </p>
            </div>
            <div class="field">
                <label class="label" for="out-note">"Note " <span class="muted">"(optional)"</span></label>
                <input id="out-note" class="input" type="text" placeholder="e.g. with charger and case" bind:value=note />
            </div>
            {move || error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <button type="submit" class="btn primary wide">"Check out"</button>
        </form>
    }
}

#[component]
fn CheckIn() -> impl IntoView {
    let st = use_app();
    let loan = RwSignal::new(None::<i64>);
    let note = RwSignal::new(String::new());

    let open = Memo::new(move |_| {
        st.rev.track();
        repo::open_loans()
    });
    let items = Memo::new(move |_| open.with(|o| vm::return_items(o)));
    let chosen = Memo::new(move |_| loan.get().and_then(|id| open.with(|o| vm::find_loan(o, id))));

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let Some(id) = loan.get_untracked() else {
            st.warn("Scan or choose the laptop being returned.");
            return;
        };
        match repo::check_in(id, &note.get_untracked()) {
            Ok(l) => {
                st.ok(vm::returned_message(&l));
                loan.set(None);
                note.set(String::new());
                request_animation_frame(|| focus("in-laptop"));
            }
            Err(e) => st.error(e),
        }
    };

    view! {
        <form class="panel slip" on:submit=submit>
            <div class="panel-head">
                <h2>"Check in"</h2>
            </div>
            <div class="field">
                <label class="label" for="in-laptop">"Laptop being returned"</label>
                <Picker items=items selected=loan placeholder="Scan or type an asset tag" input_id="in-laptop" empty="No laptop on loan matches." />
            </div>
            {move || {
                chosen.get().map(|l| view! {
                    <div class="return-card">
                        <div>
                            <span class="who">{l.borrower_name.clone()}</span>
                            <span class="muted small">"Borrowed " {time::short(l.out_at)}</span>
                        </div>
                        <DueStamp due=l.due_at />
                    </div>
                })
            }}
            <div class="field">
                <label class="label" for="in-note">"Condition " <span class="muted">"(optional)"</span></label>
                <input id="in-note" class="input" type="text" placeholder="e.g. no charger, scratched lid" bind:value=note />
            </div>
            <button type="submit" class="btn primary wide">"Check in"</button>
            <p class="hint">"Returned laptops go straight back on the shelf. The loan stays in the history."</p>
        </form>
    }
}
