//! The laptop inventory.

use leptos::prelude::*;

use crate::components::{Chips, ConfirmButton, DueStamp};
use crate::models::{Laptop, LaptopInput};
use crate::pages::import::{CsvImport, ImportKind};
use crate::repo;
use crate::state::use_app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Filter {
    All,
    Shelf,
    Out,
    Late,
    Repair,
    Retired,
}

impl Filter {
    fn keep(self, l: &Laptop, now: i64) -> bool {
        match self {
            Filter::All => l.status != "retired",
            Filter::Shelf => l.status == "available" && !l.on_loan(),
            Filter::Out => l.on_loan(),
            Filter::Late => l.due_at.is_some_and(|d| d < now),
            Filter::Repair => l.status == "repair",
            Filter::Retired => l.status == "retired",
        }
    }
}

const FILTERS: [(Filter, &str); 6] = [
    (Filter::All, "In service"),
    (Filter::Shelf, "On the shelf"),
    (Filter::Out, "On loan"),
    (Filter::Late, "Overdue"),
    (Filter::Repair, "In repair"),
    (Filter::Retired, "Retired"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Panel {
    None,
    Add,
    Edit(i64),
    Import,
}

#[component]
pub fn LaptopsPage() -> impl IntoView {
    let st = use_app();
    let panel = RwSignal::new(Panel::None);
    let filter = RwSignal::new(Filter::All);
    let search = RwSignal::new(String::new());
    let all = Memo::new(move |_| {
        st.rev.track();
        repo::laptops()
    });
    let counts = Signal::derive(move || {
        let now = st.clock.get();
        let list = all.get();
        FILTERS.iter().map(|(f, _)| list.iter().filter(|l| f.keep(l, now)).count()).collect::<Vec<_>>()
    });
    let shown = Memo::new(move |_| {
        let now = st.clock.get();
        let q = search.get().trim().to_lowercase();
        let f = filter.get();
        all.get()
            .into_iter()
            .filter(|l| f.keep(l, now))
            .filter(|l| {
                q.is_empty()
                    || format!("{} {} {} {}", l.asset_tag, l.model, l.serial, l.borrower_name.clone().unwrap_or_default())
                        .to_lowercase()
                        .contains(&q)
            })
            .collect::<Vec<_>>()
    });

    view! {
        <header class="page-head">
            <div>
                <p class="eyebrow">"Inventory"</p>
                <h1>"Laptops"</h1>
            </div>
            <div class="row wrap">
                <button type="button" class="btn" on:click=move |_| panel.set(Panel::Import)>"Import CSV"</button>
                <button type="button" class="btn primary" on:click=move |_| panel.set(Panel::Add)>"Add laptop"</button>
            </div>
        </header>

        {move || match panel.get() {
            Panel::None => ().into_any(),
            Panel::Import => view! { <CsvImport kind=ImportKind::Laptops on_close=move || panel.set(Panel::None) /> }.into_any(),
            Panel::Add => view! { <LaptopForm id=None on_done=move || panel.set(Panel::None) /> }.into_any(),
            Panel::Edit(id) => view! { <LaptopForm id=Some(id) on_done=move || panel.set(Panel::None) /> }.into_any(),
        }}

        <div class="toolbar">
            <Chips options=FILTERS.to_vec() value=filter counts=counts />
            <input id="laptop-search" class="input search" type="search" placeholder="Search tag, model, serial" bind:value=search />
        </div>

        <section class="panel">
            <div class="table-wrap">
                <table class="table">
                    <thead>
                        <tr>
                            <th>"Asset tag"</th>
                            <th>"Model"</th>
                            <th>"Serial"</th>
                            <th>"Status"</th>
                            <th>"Loans"</th>
                            <th class="right">"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let list = shown.get();
                            if list.is_empty() {
                                return view! { <tr><td colspan="6" class="empty">"No laptops match. Add one, or import a CSV from your asset register."</td></tr> }.into_any();
                            }
                            list.into_iter().map(|l| view! { <LaptopRow laptop=l panel=panel /> }).collect_view().into_any()
                        }}
                    </tbody>
                </table>
            </div>
        </section>
    }
}

#[component]
fn LaptopRow(laptop: Laptop, panel: RwSignal<Panel>) -> impl IntoView {
    let st = use_app();
    let id = laptop.id;
    let status = match (laptop.on_loan(), laptop.status.as_str()) {
        (true, _) => view! {
            <div class="status-cell">
                <span class="who">{laptop.borrower_name.clone().unwrap_or_default()}</span>
                <DueStamp due=laptop.due_at.unwrap_or_default() />
            </div>
        }
        .into_any(),
        (false, "repair") => view! { <span class="pill soon">"In repair"</span> }.into_any(),
        (false, "retired") => view! { <span class="pill">"Retired"</span> }.into_any(),
        _ => view! { <span class="pill ok">"On the shelf"</span> }.into_any(),
    };
    let current = laptop.status.clone();
    let on_loan = laptop.on_loan();
    let has_history = laptop.total_loans > 0;
    let tag = laptop.asset_tag.clone();
    view! {
        <tr>
            <td><span class="tag">{laptop.asset_tag.clone()}</span></td>
            <td>
                {laptop.model.clone()}
                {(!laptop.notes.is_empty()).then(|| view! { <div class="muted small">{laptop.notes.clone()}</div> })}
            </td>
            <td class="mono small">{laptop.serial.clone()}</td>
            <td>{status}</td>
            <td class="num">{laptop.total_loans}</td>
            <td class="right">
                <div class="row end">
                    <button type="button" class="btn ghost small" on:click=move |_| panel.set(Panel::Edit(id))>"Edit"</button>
                    <select
                        id=format!("laptop-status-{id}")
                        class="input small"
                        aria-label="Change status"
                        disabled=on_loan
                        title=if on_loan { "Check the laptop in before changing its status" } else { "Change status" }
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            let label = match v.as_str() { "repair" => "in repair", "retired" => "retired", _ => "back in service" };
                            st.report(repo::set_laptop_status(id, &v), format!("{tag} is {label}."));
                        }
                    >
                        <option value="available" selected=current == "available">"In service"</option>
                        <option value="repair" selected=current == "repair">"In repair"</option>
                        <option value="retired" selected=current == "retired">"Retired"</option>
                    </select>
                    {(!has_history).then(|| view! {
                        <ConfirmButton
                            label="Delete"
                            confirm="Delete?"
                            class="ghost small"
                            on_confirm=move || { st.report(repo::delete_laptop(id), "Laptop deleted."); }
                        />
                    })}
                </div>
            </td>
        </tr>
    }
}

#[component]
fn LaptopForm(id: Option<i64>, on_done: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let st = use_app();
    let existing = id.and_then(|id| repo::laptops().into_iter().find(|l| l.id == id));
    let tag = RwSignal::new(existing.as_ref().map(|l| l.asset_tag.clone()).unwrap_or_default());
    let model = RwSignal::new(existing.as_ref().map(|l| l.model.clone()).unwrap_or_default());
    let serial = RwSignal::new(existing.as_ref().map(|l| l.serial.clone()).unwrap_or_default());
    let notes = RwSignal::new(existing.as_ref().map(|l| l.notes.clone()).unwrap_or_default());
    let error = RwSignal::new(None::<String>);
    let done = on_done.clone();
    let add_another = RwSignal::new(false);

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let input = LaptopInput {
            asset_tag: tag.get_untracked(),
            model: model.get_untracked(),
            serial: serial.get_untracked(),
            notes: notes.get_untracked(),
        };
        let r = match id {
            Some(id) => repo::update_laptop(id, &input),
            None => repo::add_laptop(&input).map(|_| ()),
        };
        match r {
            Ok(()) => {
                st.ok(format!("{} saved.", input.asset_tag.trim()));
                if id.is_none() && add_another.get_untracked() {
                    // Keep the model for the next one in a batch of identical laptops.
                    tag.set(String::new());
                    serial.set(String::new());
                    error.set(None);
                    request_animation_frame(|| crate::components::focus("laptop-tag"));
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
                <h2>{if id.is_some() { "Edit laptop" } else { "Add a laptop" }}</h2>
                <button type="button" class="btn ghost small" on:click=move |_| on_done()>"Cancel"</button>
            </div>
            <div class="form-grid">
                <div class="field">
                    <label class="label" for="laptop-tag">"Asset tag"</label>
                    <input id="laptop-tag" class="input mono" type="text" required placeholder="LT-0117" autofocus bind:value=tag />
                </div>
                <div class="field">
                    <label class="label" for="laptop-model">"Make and model"</label>
                    <input id="laptop-model" class="input" type="text" placeholder="Dell Latitude 3440" bind:value=model />
                </div>
                <div class="field">
                    <label class="label" for="laptop-serial">"Serial number"</label>
                    <input id="laptop-serial" class="input mono" type="text" bind:value=serial />
                </div>
                <div class="field">
                    <label class="label" for="laptop-notes">"Notes"</label>
                    <input id="laptop-notes" class="input" type="text" placeholder="e.g. includes USB-C dock" bind:value=notes />
                </div>
            </div>
            {move || error.get().map(|e| view! { <p class="form-error" role="alert">{e}</p> })}
            <div class="row wrap">
                <button type="submit" class="btn primary">{if id.is_some() { "Save changes" } else { "Add laptop" }}</button>
                {id.is_none().then(|| view! {
                    <label class="check">
                        <input id="laptop-another" type="checkbox" bind:checked=add_another />
                        " Add another after this one"
                    </label>
                })}
            </div>
        </form>
    }
}
