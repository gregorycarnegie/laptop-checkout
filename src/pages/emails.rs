//! Email templates with placeholders, a live preview, and the email log.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::ConfirmButton;
use crate::email::{self, PLACEHOLDERS};
use crate::models::Loan;
use crate::repo;
use crate::state::use_app;
use crate::time;

const PURPOSES: [(&str, &str); 4] = [
    ("overdue", "Overdue"),
    ("reminder", "Due-soon reminder"),
    ("receipt", "Check-out receipt"),
    ("general", "General"),
];

fn purpose_label(p: &str) -> &'static str {
    PURPOSES.iter().find(|(k, _)| *k == p).map(|(_, v)| *v).unwrap_or("General")
}

/// A believable loan for previews when there's no real one to use.
fn preview_loan() -> Loan {
    let now = time::now();
    repo::open_loans().into_iter().next().unwrap_or(Loan {
        id: 0,
        laptop_id: 0,
        borrower_id: 0,
        out_at: now - 10 * time::DAY,
        due_at: time::end_of_day(now - 3 * time::DAY),
        returned_at: None,
        note: String::new(),
        last_notified_at: None,
        last_emailed_at: None,
        emails_sent: 0,
        renewals: 0,
        asset_tag: "LT-0103".into(),
        model: "Lenovo ThinkPad E14 Gen 5".into(),
        serial: "PF4A9K2M".into(),
        borrower_name: "Amara Okafor".into(),
        borrower_email: "amara.okafor@example.org".into(),
        department: "Year 11".into(),
    })
}

#[component]
pub fn EmailsPage() -> impl IntoView {
    let st = use_app();
    let templates = Memo::new(move |_| {
        st.rev.track();
        repo::templates()
    });
    let selected = RwSignal::new(None::<i64>);
    // Select the first template once they've loaded.
    Effect::new(move |_| {
        if selected.get().is_none() {
            if let Some(t) = templates.get().first() {
                selected.set(Some(t.id));
            }
        }
    });
    let log = Memo::new(move |_| {
        st.rev.track();
        repo::email_log(50)
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"Time-saving messages"</p>
                <h1>"Email templates"</h1>
            </div>
            <div class="row wrap">
                <button
                    type="button"
                    class="btn primary"
                    on:click=move |_| {
                        match repo::save_template(None, "New template", "About your laptop {{asset_tag}}", "Hi {{first_name}},\n\n\n\n{{sender_name}}\n{{org_name}}", "general") {
                            Ok(id) => selected.set(Some(id)),
                            Err(e) => st.error(e),
                        }
                    }
                >
                    "New template"
                </button>
            </div>
        </header>

        <div class="templates">
            <nav class="template-list" aria-label="Templates">
                {move || {
                    templates
                        .get()
                        .into_iter()
                        .map(|t| {
                            let id = t.id;
                            view! {
                                <button
                                    type="button"
                                    class="template-item"
                                    class:on=move || selected.get() == Some(id)
                                    on:click=move |_| selected.set(Some(id))
                                >
                                    <span class="template-name">{t.name.clone()}</span>
                                    <span class="muted small">{purpose_label(&t.purpose)}</span>
                                </button>
                            }
                        })
                        .collect_view()
                }}
                <button
                    type="button"
                    class="btn ghost small"
                    on:click=move |_| match repo::restore_default_templates() {
                        Ok(0) => st.ok("All the standard templates are already here."),
                        Ok(n) => st.ok(format!("Added back {}.", time::plural(n as i64, "standard template"))),
                        Err(e) => st.error(e),
                    }
                >
                    "Restore standard templates"
                </button>
            </nav>
            {move || {
                let t = selected.get().and_then(|id| templates.get_untracked().into_iter().find(|t| t.id == id));
                match t {
                    Some(t) => view! { <TemplateEditor template=t selected=selected /> }.into_any(),
                    None => view! { <p class="empty">"Choose a template to edit it."</p> }.into_any(),
                }
            }}
        </div>

        <section class="panel">
            <div class="panel-head">
                <h2>"Email log"</h2>
                <span class="muted small">"Messages opened in your email app from here"</span>
            </div>
            <div class="table-wrap">
                <table class="table">
                    <thead>
                        <tr>
                            <th>"When"</th>
                            <th>"To"</th>
                            <th>"Laptop"</th>
                            <th>"Subject"</th>
                            <th>"Template"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let list = log.get();
                            if list.is_empty() {
                                return view! { <tr><td colspan="5" class="empty">"No emails yet. Use Email on any loan, or \"Email all late borrowers\" on the Desk."</td></tr> }.into_any();
                            }
                            list.into_iter()
                                .map(|e| view! {
                                    <tr>
                                        <td class="small num">{time::stamp(e.sent_at)}</td>
                                        <td>
                                            <span class="who">{e.borrower_name.clone().unwrap_or_default()}</span>
                                            <div class="muted small">{e.to_addr.clone()}</div>
                                        </td>
                                        <td><span class="tag">{e.asset_tag.clone().unwrap_or_default()}</span></td>
                                        <td>{e.subject.clone()}</td>
                                        <td class="small">{e.template.clone()}</td>
                                    </tr>
                                })
                                .collect_view()
                                .into_any()
                        }}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn TemplateEditor(template: crate::models::Template, selected: RwSignal<Option<i64>>) -> impl IntoView {
    let st = use_app();
    let id = template.id;
    let name = RwSignal::new(template.name.clone());
    let subject = RwSignal::new(template.subject.clone());
    let body = RwSignal::new(template.body.clone());
    let purpose = RwSignal::new(template.purpose.clone());
    let body_ref = NodeRef::<leptos::html::Textarea>::new();
    let sample = preview_loan();
    let sample_label = format!("{} · {}", sample.borrower_name, sample.asset_tag);
    let sample = StoredValue::new(sample);

    // What's saved, so the Save button only lights up for real edits.
    let saved = RwSignal::new(template);
    let changed = move || {
        saved.with(|t| {
            name.get() != t.name || subject.get() != t.subject || body.get() != t.body || purpose.get() != t.purpose
        })
    };

    let insert = move |key: &str| {
        let token = format!("{{{{{key}}}}}");
        let Some(ta) = body_ref.get_untracked() else { return };
        let ta: web_sys::HtmlTextAreaElement = ta.unchecked_into();
        let value = ta.value();
        let utf16: Vec<u16> = value.encode_utf16().collect();
        let start = ta.selection_start().ok().flatten().unwrap_or(utf16.len() as u32) as usize;
        let end = ta.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
        let (start, end) = (start.min(utf16.len()), end.min(utf16.len()).max(start.min(utf16.len())));
        let next = format!(
            "{}{}{}",
            String::from_utf16_lossy(&utf16[..start]),
            token,
            String::from_utf16_lossy(&utf16[end..])
        );
        body.set(next.clone());
        ta.set_value(&next);
        let caret = (start + token.encode_utf16().count()) as u32;
        let _ = ta.focus();
        let _ = ta.set_selection_range(caret, caret);
    };

    let save = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let (n, s, b, p) = (name.get_untracked(), subject.get_untracked(), body.get_untracked(), purpose.get_untracked());
        if st.report(repo::save_template(Some(id), &n, &s, &b, &p), "Template saved.").is_some() {
            saved.set(crate::models::Template { id, name: n, subject: s, body: b, purpose: p });
        }
    };

    view! {
        <form class="panel template-editor" on:submit=save>
            <div class="form-grid">
                <div class="field">
                    <label class="label" for="tpl-name">"Template name"</label>
                    <input id="tpl-name" class="input" type="text" bind:value=name />
                </div>
                <div class="field">
                    <label class="label" for="tpl-purpose">"Used for"</label>
                    <select
                        id="tpl-purpose"
                        class="input"
                        prop:value=move || purpose.get()
                        on:change=move |ev| purpose.set(event_target_value(&ev))
                    >
                        {PURPOSES.map(|(k, v)| view! { <option value=k>{v}</option> }).collect_view()}
                    </select>
                </div>
            </div>
            <div class="field">
                <label class="label" for="tpl-subject">"Subject"</label>
                <input id="tpl-subject" class="input" type="text" bind:value=subject />
            </div>
            <div class="field">
                <label class="label" for="tpl-body">"Message"</label>
                <textarea
                    id="tpl-body"
                    class="input body-text"
                    rows="14"
                    node_ref=body_ref
                    prop:value=move || body.get()
                    on:input=move |ev| body.set(event_target_value(&ev))
                ></textarea>
            </div>
            <div class="field">
                <span class="label">"Insert a detail"</span>
                <div class="placeholders">
                    {PLACEHOLDERS
                        .iter()
                        .map(|(key, desc)| {
                            let key = *key;
                            view! {
                                <button type="button" class="token" title=*desc on:click=move |_| insert(key)>
                                    {format!("{{{{{key}}}}}")}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
            <div class="row wrap">
                <button type="submit" class="btn primary" disabled=move || !changed()>"Save template"</button>
                <ConfirmButton
                    label="Delete template"
                    confirm="Delete it?"
                    class="ghost"
                    on_confirm=move || {
                        if st.report(repo::delete_template(id), "Template deleted.").is_some() {
                            selected.set(None);
                        }
                    }
                />
            </div>

            <div class="preview">
                <p class="eyebrow">"Preview with " {sample_label}</p>
                <div class="letter">
                    <p class="letter-subject">
                        {move || sample.with_value(|l| email::render(&subject.get(), l, &st.settings.get()))}
                    </p>
                    <pre class="letter-body">
                        {move || sample.with_value(|l| email::render(&body.get(), l, &st.settings.get()))}
                    </pre>
                </div>
            </div>
        </form>
    }
}
