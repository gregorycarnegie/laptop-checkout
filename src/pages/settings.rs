//! Where the data lives, notifications, loan rules and email details.

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::components::ConfirmButton;
use crate::db;
use crate::email;
use crate::repo;
use crate::state::use_app;
use crate::time;

#[component]
pub fn SettingsPage() -> impl IntoView {
    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"Set up the desk"</p>
                <h1>"Settings"</h1>
            </div>
        </header>
        <div class="settings">
            <Storage />
            <Notifications />
            <LoanRules />
            <EmailDetails />
            <DangerZone />
        </div>
    }
}

#[component]
fn Storage() -> impl IntoView {
    let st = use_app();
    let status = st.db_status;

    let after_swap = move |r: Result<(), String>, ok: &'static str| match r {
        Ok(()) => match repo::migrate() {
            Ok(()) => st.ok(ok),
            Err(e) => st.error(e),
        },
        Err(e) if db::is_cancel(&e) => {}
        Err(e) => st.error(e),
    };

    let create = move |_| {
        spawn_local(async move { after_swap(db::create_file().await, "Saving to the file on this PC.") });
    };
    let open = move |_| {
        spawn_local(async move { after_swap(db::open_file().await, "Opened the database file.") });
    };
    let reconnect = move |_| {
        spawn_local(async move {
            match db::reconnect_file().await {
                Ok(true) => after_swap(Ok(()), "Reconnected to the database file."),
                Ok(false) => st.warn("The browser didn't allow access to the file."),
                Err(e) => st.error(e),
            }
        });
    };
    let disconnect = move |_| {
        spawn_local(async move {
            if let Err(e) = db::disconnect_file().await {
                st.error(e);
            } else {
                st.ok("Stopped saving to the file. Your data is still saved in this browser.");
            }
        });
    };
    let download = move |_| {
        let name = format!("laptop-checkout-{}.sqlite", time::to_input(time::now()));
        if let Err(e) = db::download(&name) {
            st.error(e);
        }
    };
    let restore = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let Some(file) = input.files().and_then(|f| f.get(0)) else { return };
        input.set_value("");
        spawn_local(async move {
            let r = db::restore_upload(file).await;
            after_swap(r, "Backup restored.");
            db::save_now().await;
        });
    };

    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>"Where your data is saved"</h2>
                <SaveBadge />
            </div>
            <p>
                "Everything is stored in a SQLite database on this PC. Nothing is sent to a server. "
                "Every change is saved in this browser straight away."
            </p>
            {move || {
                let s = status.get();
                if s.file_connected {
                    view! {
                        <div class="callout ok">
                            <p><strong>"Also saving to " <span class="mono">{s.file_name.clone().unwrap_or_default()}</span></strong></p>
                            <p class="small">"Each change is written into that file too, so you can back it up with your other files."</p>
                            <div class="row wrap">
                                <button type="button" class="btn ghost small" on:click=disconnect>"Stop saving to this file"</button>
                            </div>
                        </div>
                    }
                    .into_any()
                } else if s.file_needs_permission {
                    view! {
                        <div class="callout warn">
                            <p><strong>"Reconnect to " <span class="mono">{s.file_name.clone().unwrap_or_default()}</span></strong></p>
                            <p class="small">"The browser asks again for permission after it restarts. Until you allow it, changes are only saved in this browser."</p>
                            <div class="row wrap">
                                <button type="button" class="btn primary" on:click=reconnect>"Reconnect"</button>
                                <button type="button" class="btn ghost small" on:click=disconnect>"Forget this file"</button>
                            </div>
                        </div>
                    }
                    .into_any()
                } else if s.fs_supported {
                    view! {
                        <div class="callout">
                            <p><strong>"Keep the database in a file you choose"</strong></p>
                            <p class="small">"Pick a folder such as Documents or a backed-up drive. The app then writes every change into that .sqlite file."</p>
                            <div class="row wrap">
                                <button type="button" class="btn primary" on:click=create>"Save to a new file…"</button>
                                <button type="button" class="btn" on:click=open>"Open an existing file…"</button>
                            </div>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="callout">
                            <p><strong>"This browser can't save straight to a file"</strong></p>
                            <p class="small">"Chrome and Edge can keep the database in a .sqlite file you choose. In this browser, your data is saved on this PC in the browser's storage. Download a backup copy regularly."</p>
                        </div>
                    }
                    .into_any()
                }
            }}
            <div class="row wrap">
                <button type="button" class="btn" on:click=download>"Download a backup copy"</button>
                <label class="btn file-btn">
                    "Restore from a backup…"
                    <input id="restore-file" type="file" accept=".sqlite,.sqlite3,.db" on:change=restore />
                </label>
            </div>
            <p class="hint">"Restoring replaces everything in the app with the contents of the backup file."</p>
        </section>
    }
}

#[component]
pub fn SaveBadge() -> impl IntoView {
    let st = use_app();
    view! {
        {move || {
            let s = st.db_status.get();
            let (class, text) = if s.error.is_some() {
                ("pill late", "Not saved".to_string())
            } else if s.saving || s.dirty {
                ("pill soon", "Saving…".to_string())
            } else if let Some(t) = s.last_saved {
                ("pill ok", format!("Saved {}", time::stamp(t)))
            } else {
                ("pill ok", "Saved".to_string())
            };
            view! { <span class=class title=s.error.clone().unwrap_or_default()>{text}</span> }
        }}
    }
}

#[component]
fn Notifications() -> impl IntoView {
    let st = use_app();
    let s = st.settings;
    let perm = st.notify_permission;
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>"Overdue alerts"</h2>
            </div>
            <label class="check">
                <input
                    id="notify-enabled"
                    type="checkbox"
                    prop:checked=move || s.get().notify_enabled
                    on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        st.report(repo::set_setting("notify_enabled", if on { "1" } else { "0" }), if on { "Overdue alerts are on." } else { "Overdue alerts are off." });
                    }
                />
                " Alert me when a laptop becomes overdue"
            </label>
            <div class="field">
                <label class="label" for="renotify">"Remind me again about the same laptop every"</label>
                <select
                    id="renotify"
                    class="input narrow"
                    prop:value=move || s.get().renotify_hours.to_string()
                    on:change=move |ev| { st.report(repo::set_setting("renotify_hours", &event_target_value(&ev)), "Saved."); }
                >
                    <option value="1">"hour"</option>
                    <option value="4">"4 hours"</option>
                    <option value="24">"day"</option>
                    <option value="72">"3 days"</option>
                    <option value="168">"week"</option>
                </select>
            </div>
            <div class="callout">
                {move || match perm.get().as_str() {
                    "granted" => view! {
                        <p><strong>"Desktop notifications are allowed."</strong></p>
                        <div class="row wrap">
                            <button type="button" class="btn small" on:click=move |_| crate::notify::show(st, "Test notification", "Overdue alerts will look like this.", "test")>"Send a test"</button>
                        </div>
                    }.into_any(),
                    "denied" => view! {
                        <p><strong>"Desktop notifications are blocked."</strong></p>
                        <p class="small">"Alerts will appear inside this page instead. To allow them, click the icon at the left of the address bar, then allow Notifications."</p>
                    }.into_any(),
                    "unsupported" => view! {
                        <p><strong>"This browser can't show desktop notifications here."</strong></p>
                        <p class="small">"Alerts will appear inside this page instead."</p>
                    }.into_any(),
                    _ => view! {
                        <p><strong>"Desktop notifications are not turned on yet."</strong></p>
                        <div class="row wrap">
                            <button type="button" class="btn primary" on:click=move |_| crate::notify::request(st)>"Turn on notifications"</button>
                        </div>
                    }.into_any(),
                }}
                <p class="small muted">"Checks run every minute while the app is open in a browser tab. Pin the tab to keep it running."</p>
            </div>
        </section>
    }
}

#[component]
fn LoanRules() -> impl IntoView {
    let st = use_app();
    let s = st.settings;
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>"Loans"</h2>
            </div>
            <div class="form-grid">
                <div class="field">
                    <label class="label" for="loan-days">"Standard loan period (days)"</label>
                    <input
                        id="loan-days"
                        class="input narrow"
                        type="number"
                        min="1"
                        max="365"
                        prop:value=move || s.get().loan_days.to_string()
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            match v.trim().parse::<i64>() {
                                Ok(n) if (1..=365).contains(&n) => { st.report(repo::set_setting("loan_days", &n.to_string()), "Loan period saved."); }
                                _ => st.error("Enter a number of days between 1 and 365."),
                            }
                        }
                    />
                    <p class="hint">"Used for new check-outs and the Renew button."</p>
                </div>
                <TextSetting id="return-location" key="return_location" label="Where laptops are returned" value=Signal::derive(move || s.get().return_location) hint="Used in emails: \"Please bring it back to …\"" />
            </div>
        </section>
    }
}

#[component]
fn TextSetting(
    id: &'static str,
    key: &'static str,
    label: &'static str,
    value: Signal<String>,
    #[prop(optional)] hint: &'static str,
) -> impl IntoView {
    let st = use_app();
    view! {
        <div class="field">
            <label class="label" for=id>{label}</label>
            <input
                id=id
                class="input"
                type="text"
                prop:value=move || value.get()
                on:change=move |ev| { st.report(repo::set_setting(key, event_target_value(&ev).trim()), "Saved."); }
            />
            {(!hint.is_empty()).then(|| view! { <p class="hint">{hint}</p> })}
        </div>
    }
}

#[component]
fn EmailDetails() -> impl IntoView {
    let st = use_app();
    let s = st.settings;
    let templates = Memo::new(move |_| {
        st.rev.track();
        repo::templates()
    });
    view! {
        <section class="panel">
            <div class="panel-head">
                <h2>"Emails"</h2>
            </div>
            <div class="form-grid">
                <TextSetting id="sender-name" key="sender_name" label="Sign emails as" value=Signal::derive(move || s.get().sender_name) />
                <TextSetting id="org-name" key="org_name" label="Organisation" value=Signal::derive(move || s.get().org_name) />
                <div class="field">
                    <label class="label" for="email-app">"Open emails in"</label>
                    <select
                        id="email-app"
                        class="input"
                        prop:value=move || s.get().email_app
                        on:change=move |ev| { st.report(repo::set_setting("email_app", &event_target_value(&ev)), "Saved."); }
                    >
                        {email::APPS.iter().map(|(k, v)| view! { <option value=*k>{*v}</option> }).collect_view()}
                    </select>
                    <p class="hint">"The message opens ready to send. Nothing is sent until you press Send in your email app."</p>
                </div>
                <div class="field">
                    <label class="label" for="late-template">"Template for late emails"</label>
                    <select
                        id="late-template"
                        class="input"
                        prop:value=move || s.get().late_template.map(|v| v.to_string()).unwrap_or_default()
                        on:change=move |ev| { st.report(repo::set_setting("late_template", &event_target_value(&ev)), "Saved."); }
                    >
                        <option value="">"First overdue template"</option>
                        {move || templates.get().into_iter().map(|t| view! { <option value=t.id.to_string()>{t.name}</option> }).collect_view()}
                    </select>
                </div>
            </div>
        </section>
    }
}

#[component]
fn DangerZone() -> impl IntoView {
    let st = use_app();
    let sample = move || st.settings.get().sample_data;
    view! {
        <section class="panel danger-zone">
            <div class="panel-head">
                <h2>"Start fresh"</h2>
            </div>
            <Show when=sample>
                <p>"The app is showing example borrowers, laptops and loans so you can try it out."</p>
            </Show>
            <p class="small muted">"Removes every borrower, laptop, loan and email log entry. Your templates and settings stay. Download a backup first if you might need the records."</p>
            <ConfirmButton
                label=if st.settings.get_untracked().sample_data { "Remove the example data" } else { "Erase all records" }
                confirm="Click again to erase"
                on_confirm=move || { st.report(repo::clear_records(), "All records removed."); }
            />
        </section>
    }
}
