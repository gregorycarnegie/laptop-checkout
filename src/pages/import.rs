//! CSV import panel shared by the Borrowers and Laptops pages.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;

use crate::csv_import::{self, Draft};
use crate::models::{BorrowerInput, LaptopInput};
use crate::repo::{self, ImportResult};
use crate::state::use_app;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImportKind {
    Borrowers,
    Laptops,
}

#[derive(Clone, Debug, PartialEq)]
enum Parsed {
    Borrowers(Vec<Draft<BorrowerInput>>),
    Laptops(Vec<Draft<LaptopInput>>),
}

/// One preview row: line, the cells to show, problem, and whether it matches an existing record.
type PreviewRow = (usize, Vec<String>, Option<String>, bool);

impl Parsed {
    fn build(kind: ImportKind, text: &str) -> Result<Parsed, String> {
        let rows = csv_import::parse(text)?;
        Ok(match kind {
            ImportKind::Borrowers => {
                let mut d = csv_import::borrowers(&rows);
                csv_import::mark_repeats(&mut d, |b| b.email.clone());
                Parsed::Borrowers(d)
            }
            ImportKind::Laptops => {
                let mut d = csv_import::laptops(&rows);
                csv_import::mark_repeats(&mut d, |l| l.asset_tag.clone());
                Parsed::Laptops(d)
            }
        })
    }

    fn preview(&self) -> Vec<PreviewRow> {
        match self {
            Parsed::Borrowers(d) => d
                .iter()
                .map(|r| {
                    let b = &r.record;
                    (
                        r.line,
                        vec![b.name.clone(), b.email.clone(), b.department.clone(), b.external_id.clone()],
                        r.problem.clone(),
                        repo::find_borrower_by_email(&b.email).is_some(),
                    )
                })
                .collect(),
            Parsed::Laptops(d) => d
                .iter()
                .map(|r| {
                    let l = &r.record;
                    (
                        r.line,
                        vec![l.asset_tag.clone(), l.model.clone(), l.serial.clone(), l.notes.clone()],
                        r.problem.clone(),
                        repo::find_laptop_by_tag(&l.asset_tag).is_some(),
                    )
                })
                .collect(),
        }
    }

    fn import(&self, update: bool) -> Result<ImportResult, String> {
        match self {
            Parsed::Borrowers(d) => {
                let ok: Vec<_> = d.iter().filter(|r| r.problem.is_none()).map(|r| r.record.clone()).collect();
                repo::import_borrowers(&ok, update)
            }
            Parsed::Laptops(d) => {
                let ok: Vec<_> = d.iter().filter(|r| r.problem.is_none()).map(|r| r.record.clone()).collect();
                repo::import_laptops(&ok, update)
            }
        }
    }
}

#[component]
pub fn CsvImport(kind: ImportKind, on_close: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let st = use_app();
    let text = RwSignal::new(String::new());
    let parsed = RwSignal::new(None::<Result<Parsed, String>>);
    let update_existing = RwSignal::new(false);

    let (headers, example, columns) = match kind {
        ImportKind::Borrowers => (
            csv_import::BORROWER_HEADERS,
            "name,email,department,id\nAmara Okafor,amara.okafor@school.org,Year 11,S20931\nGrace Whitfield,g.whitfield@school.org,English,T0388",
            ["Name", "Email", "Department", "ID"],
        ),
        ImportKind::Laptops => (
            csv_import::LAPTOP_HEADERS,
            "asset_tag,model,serial\nLT-0201,Dell Latitude 3440,7HQ9ZK3\nLT-0202,Dell Latitude 3440,7HQ9ZK4",
            ["Asset tag", "Model", "Serial", "Notes"],
        ),
    };

    let reparse = move || {
        let t = text.get_untracked();
        parsed.set((!t.trim().is_empty()).then(|| Parsed::build(kind, &t)));
    };

    let on_file = move |ev: leptos::ev::Event| {
        let input: web_sys::HtmlInputElement = event_target(&ev);
        let Some(file) = input.files().and_then(|f| f.get(0)) else { return };
        input.set_value("");
        spawn_local(async move {
            match JsFuture::from(file.text()).await {
                Ok(v) => {
                    text.set(v.as_string().unwrap_or_default());
                    reparse();
                }
                Err(_) => st.error("Couldn't read that file. Save it as CSV (comma separated) and try again."),
            }
        });
    };

    let preview = Memo::new(move |_| {
        st.rev.track();
        parsed.get().and_then(|p| p.ok()).map(|p| p.preview())
    });
    let counts = Memo::new(move |_| {
        preview
            .get()
            .map(|rows| {
                let bad = rows.iter().filter(|r| r.2.is_some()).count();
                let existing = rows.iter().filter(|r| r.2.is_none() && r.3).count();
                (rows.len() - bad - existing, existing, bad)
            })
            .unwrap_or_default()
    });

    let close = on_close.clone();
    let run = move |_| {
        let Some(Ok(p)) = parsed.get_untracked() else { return };
        match p.import(update_existing.get_untracked()) {
            Ok(r) => {
                let what = if kind == ImportKind::Borrowers { "borrower" } else { "laptop" };
                let mut parts = vec![format!("Added {}", crate::time::plural(r.added as i64, what))];
                if r.updated > 0 {
                    parts.push(format!("updated {}", r.updated));
                }
                if r.skipped > 0 {
                    parts.push(format!("skipped {}", r.skipped));
                }
                st.ok(format!("{}.", parts.join(", ")));
                text.set(String::new());
                parsed.set(None);
                close();
            }
            Err(e) => st.error(format!("Nothing was imported: {e}")),
        }
    };

    view! {
        <section class="panel import">
            <div class="panel-head">
                <h2>{if kind == ImportKind::Borrowers { "Import borrowers from CSV" } else { "Import laptops from CSV" }}</h2>
                <button type="button" class="btn ghost small" on:click=move |_| on_close()>"Close"</button>
            </div>
            <p class="hint">
                "Export a spreadsheet as CSV, then choose the file or paste its contents. The first row should be column headings such as "
                <code>{headers}</code>
                ". Common alternatives like \"First name\" + \"Surname\" or \"Serial number\" are recognised."
            </p>
            <div class="row wrap">
                <label class="btn file-btn">
                    "Choose CSV file…"
                    <input id=format!("csv-file-{kind:?}") type="file" accept=".csv,.txt,text/csv" on:change=on_file />
                </label>
                <button type="button" class="btn ghost small" on:click=move |_| { text.set(example.to_string()); reparse(); }>
                    "Show an example"
                </button>
            </div>
            <label class="label" for=format!("csv-text-{kind:?}")>"…or paste CSV here"</label>
            <textarea
                id=format!("csv-text-{kind:?}")
                class="input mono"
                rows="5"
                placeholder=example
                prop:value=move || text.get()
                on:input=move |ev| { text.set(event_target_value(&ev)); reparse(); }
            ></textarea>

            {move || match parsed.get() {
                Some(Err(e)) => view! { <p class="form-error">{e}</p> }.into_any(),
                None => ().into_any(),
                Some(Ok(_)) => view! {
                    <div class="import-summary">
                        <span class="pill ok">{move || format!("{} new", counts.get().0)}</span>
                        <span class="pill">{move || format!("{} already here", counts.get().1)}</span>
                        <span class="pill late">{move || format!("{} with problems", counts.get().2)}</span>
                        <label class="check">
                            <input id=format!("csv-update-{kind:?}") type="checkbox" bind:checked=update_existing />
                            {if kind == ImportKind::Borrowers { " Update borrowers already here (matched by email)" } else { " Update laptops already here (matched by asset tag)" }}
                        </label>
                    </div>
                    <div class="table-wrap">
                        <table class="table">
                            <thead>
                                <tr>
                                    <th>"Line"</th>
                                    {columns.map(|c| view! { <th>{c}</th> }).collect_view()}
                                    <th>"Status"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    preview
                                        .get()
                                        .unwrap_or_default()
                                        .into_iter()
                                        .take(200)
                                        .map(|(line, cells, problem, exists)| {
                                            let status = match (&problem, exists) {
                                                (Some(p), _) => view! { <span class="pill late">{p.clone()}</span> }.into_any(),
                                                (None, true) => view! { <span class="pill">"Already here"</span> }.into_any(),
                                                (None, false) => view! { <span class="pill ok">"New"</span> }.into_any(),
                                            };
                                            view! {
                                                <tr class:bad=problem.is_some()>
                                                    <td class="num">{line}</td>
                                                    {cells.into_iter().map(|c| view! { <td>{c}</td> }).collect_view()}
                                                    <td>{status}</td>
                                                </tr>
                                            }
                                        })
                                        .collect_view()
                                }}
                            </tbody>
                        </table>
                    </div>
                    <div class="row">
                        <button
                            type="button"
                            class="btn primary"
                            disabled=move || { let (n, e, _) = counts.get(); n == 0 && !(update_existing.get() && e > 0) }
                            on:click=run.clone()
                        >
                            {move || {
                                let (n, e, _) = counts.get();
                                if update_existing.get() && e > 0 { format!("Import {n} and update {e}") } else { format!("Import {n}") }
                            }}
                        </button>
                    </div>
                }
                .into_any(),
            }}
        </section>
    }
}
