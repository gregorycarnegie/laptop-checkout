//! App shell: boot, navigation, the compose dialog and toasts.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::email;
use crate::repo;
use crate::time;
use crate::ui::components::CopyButton;
use crate::ui::pages::{
    borrowers::BorrowersPage,
    desk::DeskPage,
    emails::EmailsPage,
    laptops::LaptopsPage,
    loans::LoansPage,
    settings::{SaveBadge, SettingsPage},
};
use crate::ui::state::{use_app, AppState};
use crate::view_model::{self as vm, Page, ToastAction};
use crate::web::{notify, persistence};

#[derive(Clone, PartialEq, Debug)]
enum Boot {
    Loading,
    Ready,
    Failed(String),
}

#[component]
pub fn App() -> impl IntoView {
    let st = AppState::new(notify::permission());
    provide_context(st);
    persistence::connect(st.rev, st.db_status);

    if let Some(w) = web_sys::window() {
        if let Ok(hash) = w.location().hash() {
            st.page.set(Page::from_hash(&hash));
        }
    }

    let boot = RwSignal::new(Boot::Loading);
    spawn_local(async move {
        let result = async {
            let fresh = persistence::init().await?;
            repo::migrate()?;
            if fresh {
                repo::seed_sample()?;
            }
            Ok::<_, String>(())
        }
        .await;
        match result {
            Ok(()) => {
                boot.set(Boot::Ready);
                notify::start(st);
            }
            Err(e) => boot.set(Boot::Failed(e)),
        }
    });

    view! {
        {move || match boot.get() {
            Boot::Loading => view! { <div class="boot">"Opening the loan ledger…"</div> }.into_any(),
            Boot::Failed(e) => view! {
                <div class="boot">
                    <div class="boot-error">
                        <h1>"The database couldn't be opened"</h1>
                        <p>{e}</p>
                        <p class="muted">"Reload the page to try again. If this keeps happening, try Chrome or Edge."</p>
                    </div>
                </div>
            }
            .into_any(),
            Boot::Ready => view! { <Shell /> }.into_any(),
        }}
    }
}

#[component]
fn Shell() -> impl IntoView {
    let st = use_app();
    let late = Memo::new(move |_| {
        st.rev.track();
        vm::late(&repo::open_loans(), st.clock.get()).len()
    });

    view! {
        <div class="shell">
            <aside class="rail">
                <div class="brand">
                    <span class="brand-mark" aria-hidden="true">"LT"</span>
                    <span class="brand-name">"Laptop Checkout"</span>
                </div>
                <nav class="nav" aria-label="Sections">
                    {Page::ALL
                        .into_iter()
                        .map(|p| {
                            view! {
                                <a
                                    href=format!("#{}", p.slug())
                                    class="nav-item"
                                    class:on=move || st.page.get() == p
                                    aria-current=move || (st.page.get() == p).then_some("page")
                                    on:click=move |ev| {
                                        ev.prevent_default();
                                        st.go(p);
                                    }
                                >
                                    <span>{p.label()}</span>
                                    {(p == Page::Desk).then(|| view! {
                                        <Show when=move || { late.get() > 0 }>
                                            <span class="nav-badge" title="Overdue laptops">{move || late.get()}</span>
                                        </Show>
                                    })}
                                </a>
                            }
                        })
                        .collect_view()}
                </nav>
                <div class="rail-foot">
                    <SaveBadge />
                    <StorageLine />
                </div>
            </aside>
            <main class="work" id="main">
                <SampleBanner />
                <ReconnectBanner />
                {move || match st.page.get() {
                    Page::Desk => view! { <DeskPage /> }.into_any(),
                    Page::Loans => view! { <LoansPage /> }.into_any(),
                    Page::Laptops => view! { <LaptopsPage /> }.into_any(),
                    Page::Borrowers => view! { <BorrowersPage /> }.into_any(),
                    Page::Emails => view! { <EmailsPage /> }.into_any(),
                    Page::Settings => view! { <SettingsPage /> }.into_any(),
                }}
            </main>
            <ComposeDialog />
            <Toasts />
        </div>
    }
}

#[component]
fn StorageLine() -> impl IntoView {
    let st = use_app();
    view! {
        <button type="button" class="storage-line" on:click=move |_| st.go(Page::Settings)>
            {move || {
                let s = st.db_status.get();
                match (s.file_connected, s.file_name) {
                    (true, Some(name)) => format!("File: {name}"),
                    _ => "Stored in this browser".to_string(),
                }
            }}
        </button>
    }
}

#[component]
fn SampleBanner() -> impl IntoView {
    let st = use_app();
    view! {
        <Show when=move || st.settings.get().sample_data>
            <div class="banner">
                <p>
                    <strong>"You're looking at example data."</strong>
                    " Try checking laptops in and out, then remove it when you're ready to add your own."
                </p>
                <button
                    type="button"
                    class="btn small"
                    on:click=move |_| { st.report(repo::clear_records(), "Example data removed. Add your laptops and borrowers to get started."); }
                >
                    "Remove example data"
                </button>
            </div>
        </Show>
    }
}

#[component]
fn ReconnectBanner() -> impl IntoView {
    let st = use_app();
    let reconnect = move |_| {
        spawn_local(async move {
            match persistence::reconnect_file().await {
                Ok(true) => match repo::migrate() {
                    Ok(()) => st.ok("Reconnected to the database file."),
                    Err(e) => st.error(e),
                },
                Ok(false) => st.warn("The browser didn't allow access to the file."),
                Err(e) => st.error(e),
            }
        });
    };
    view! {
        <Show when=move || st.db_status.get().file_needs_permission>
            <div class="banner warn">
                <p>
                    <strong>"Reconnect to your database file. "</strong>
                    {move || format!("The browser needs your permission again to use {}.", st.db_status.get().file_name.unwrap_or_default())}
                </p>
                <button type="button" class="btn primary small" on:click=reconnect>"Reconnect"</button>
            </div>
        </Show>
    }
}

#[component]
fn Toasts() -> impl IntoView {
    let st = use_app();
    view! {
        <div class="toasts" role="status" aria-live="polite">
            <For
                each=move || st.toasts.get()
                key=|t| t.id
                children=move |t| {
                    let id = t.id;
                    let class = t.kind.class();
                    let dismiss = move || st.toasts.update(|v| v.retain(|x| x.id != id));
                    view! {
                        <div class=class>
                            <p>{t.text.clone()}</p>
                            {t.action.clone().map(|(label, action)| view! {
                                <button
                                    type="button"
                                    class="btn small"
                                    on:click=move |_| {
                                        match action.clone() {
                                            ToastAction::EmailLoan { loan_id, purpose } => st.email_loans(vec![loan_id], purpose),
                                            ToastAction::EmailAllLate => {
                                                st.email_loans(vm::emailable(&vm::late(&repo::open_loans(), time::now())), "overdue");
                                            }
                                        }
                                        dismiss();
                                    }
                                >
                                    {label}
                                </button>
                            })}
                            <button type="button" class="toast-close" aria-label="Dismiss" on:click=move |_| dismiss()>"×"</button>
                        </div>
                    }
                }
            />
        </div>
    }
}

/// Writes one email at a time from a queue of loans.
#[component]
fn ComposeDialog() -> impl IntoView {
    let st = use_app();
    let templates = Memo::new(move |_| {
        st.rev.track();
        repo::templates()
    });
    let template_id = RwSignal::new(None::<i64>);
    let to = RwSignal::new(String::new());
    let subject = RwSignal::new(String::new());
    let body = RwSignal::new(String::new());
    let opened = RwSignal::new(Vec::<i64>::new());

    let current = Memo::new(move |_| {
        st.compose.get().and_then(|c| {
            let id = *c.loan_ids.get(c.index)?;
            repo::loan(id)
        })
    });

    // Choose a template whenever a new queue opens (not when stepping through one).
    let queue = Memo::new(move |_| st.compose.with(|c| c.as_ref().map(|c| (c.loan_ids.clone(), c.purpose))));
    Effect::new(move |_| {
        if let Some((_, purpose)) = queue.get() {
            template_id.set(vm::pick_template(
                &templates.get_untracked(),
                purpose,
                st.settings.get_untracked().late_template,
            ));
            opened.set(Vec::new());
        }
    });

    // Fill in the message whenever the loan or template changes.
    Effect::new(move |_| {
        let Some(loan) = current.get() else { return };
        let tid = template_id.get();
        let s = st.settings.get_untracked();
        let t = templates.with_untracked(|all| vm::find_template(all, tid));
        to.set(loan.borrower_email.clone());
        match t {
            Some(t) => {
                subject.set(email::render(&t.subject, &loan, &s));
                body.set(email::render(&t.body, &loan, &s));
            }
            None => {
                subject.set(String::new());
                body.set(String::new());
            }
        }
    });

    let close = move || st.compose.set(None);
    let step = move |delta: isize| {
        st.compose.update(|c| {
            if let Some(c) = c {
                c.index = vm::step_index(c.index, c.loan_ids.len(), delta);
            }
        })
    };
    let app = move || st.settings.get().email_app;
    let link_for = move |app_key: &str| email::compose_url(app_key, &to.get(), &subject.get(), &body.get());
    let mark_opened = move || {
        let Some(loan) = current.get_untracked() else { return };
        if opened.get_untracked().contains(&loan.id) {
            return;
        }
        let tname = templates
            .with_untracked(|all| vm::find_template(all, template_id.get_untracked()))
            .map(|t| t.name)
            .unwrap_or_default();
        if repo::log_email(&loan, &to.get_untracked(), &subject.get_untracked(), &tname).is_ok() {
            opened.update(|o| o.push(loan.id));
        }
    };

    view! {
        <Show when=move || st.compose.with(|c| c.is_some())>
            <div class="scrim" on:click=move |_| close()></div>
            <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="compose-title">
                <div class="dialog-head">
                    <div>
                        <p class="eyebrow">
                            {move || st.compose.with(|c| c.as_ref().map(|c| {
                                vm::queue_label(c.index, c.loan_ids.len())
                            }).unwrap_or_default())}
                        </p>
                        <h2 id="compose-title">
                            {move || current.get().map(|l| format!("{} · {}", l.borrower_name, l.asset_tag)).unwrap_or_default()}
                        </h2>
                    </div>
                    <button type="button" class="toast-close" aria-label="Close" on:click=move |_| close()>"×"</button>
                </div>

                <div class="field">
                    <label class="label" for="compose-template">"Template"</label>
                    <select
                        id="compose-template"
                        class="input"
                        prop:value=move || template_id.get().map(|v| v.to_string()).unwrap_or_default()
                        on:change=move |ev| template_id.set(event_target_value(&ev).parse().ok())
                    >
                        {move || templates.get().into_iter().map(|t| view! { <option value=t.id.to_string()>{t.name}</option> }).collect_view()}
                    </select>
                </div>
                <div class="field">
                    <label class="label" for="compose-to">"To"</label>
                    <div class="row">
                        <input id="compose-to" class="input" type="email" bind:value=to />
                        <CopyButton text=to label="Copy" />
                    </div>
                </div>
                <div class="field">
                    <label class="label" for="compose-subject">"Subject"</label>
                    <div class="row">
                        <input id="compose-subject" class="input" type="text" bind:value=subject />
                        <CopyButton text=subject label="Copy" />
                    </div>
                </div>
                <div class="field">
                    <div class="row between">
                        <label class="label" for="compose-body">"Message"</label>
                        <CopyButton text=body label="Copy message" />
                    </div>
                    <textarea id="compose-body" class="input body-text" rows="11" bind:value=body></textarea>
                </div>

                <div class="dialog-actions">
                    <a
                        class="btn primary"
                        href=move || link_for(&app())
                        target=move || vm::link_target(&app())
                        rel="noopener"
                        on:click=move |_| mark_opened()
                    >
                        {move || format!("Open in {}", vm::short_app(&app()))}
                    </a>
                    <div class="other-apps muted small">
                        "or "
                        {email::APPS
                            .iter()
                            .map(|(k, _)| {
                                let k = *k;
                                view! {
                                    <Show when=move || app() != k>
                                        <a
                                            href=move || link_for(k)
                                            target=vm::link_target(k)
                                            rel="noopener"
                                            on:click=move |_| mark_opened()
                                        >
                                            {vm::short_app(k)}
                                        </a>
                                        " "
                                    </Show>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
                <p class="hint">
                    {move || if current.get().map(|l| opened.get().contains(&l.id)).unwrap_or(false) {
                        "Logged. Press Send in your email app, then move on to the next one."
                    } else {
                        "Your email app opens with this message ready to send. If nothing opens, copy the address, subject and message instead."
                    }}
                </p>

                <Show when=move || st.compose.with(|c| c.as_ref().map(|c| c.loan_ids.len() > 1).unwrap_or(false))>
                    <div class="row between queue">
                        <button type="button" class="btn ghost" on:click=move |_| step(-1)
                            disabled=move || st.compose.with(|c| c.as_ref().map(|c| c.index == 0).unwrap_or(true))>
                            "Previous"
                        </button>
                        <span class="muted small">
                            {move || format!("{} of {} opened", opened.get().len(), st.compose.with(|c| c.as_ref().map(|c| c.loan_ids.len()).unwrap_or(0)))}
                        </span>
                        {move || {
                            let last = st.compose.with(|c| c.as_ref().map(|c| c.index + 1 >= c.loan_ids.len()).unwrap_or(true));
                            if last {
                                view! { <button type="button" class="btn" on:click=move |_| close()>"Done"</button> }.into_any()
                            } else {
                                view! { <button type="button" class="btn" on:click=move |_| step(1)>"Next borrower"</button> }.into_any()
                            }
                        }}
                    </div>
                </Show>
            </div>
        </Show>
    }
}
