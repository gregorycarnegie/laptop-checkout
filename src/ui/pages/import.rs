//! CSV import panel shared by the Borrowers and Laptops pages.

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen_futures::JsFuture;

use crate::import::{result_message, ImportKind, Parsed, Tally};
use crate::ui::state::use_app;

#[component]
pub fn CsvImport(kind: ImportKind, on_close: impl Fn() + Clone + Send + Sync + 'static) -> impl IntoView {
    let st = use_app();
    let text = RwSignal::new(String::new());
    let parsed = RwSignal::new(None::<Result<Parsed, String>>);
    let update_existing = RwSignal::new(false);

    let reparse = move || {
        parsed.set(Parsed::from_text(kind, &text.get_untracked()));
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
        parsed.with(|p| p.as_ref().and_then(|p| p.as_ref().ok()).map(Parsed::preview))
    });
    let tally = Memo::new(move |_| preview.with(|rows| rows.as_deref().map(Tally::of).unwrap_or_default()));

    let close = on_close.clone();
    let run = move |_| {
        let Some(Ok(p)) = parsed.get_untracked() else { return };
        match p.import(update_existing.get_untracked()) {
            Ok(r) => {
                st.ok(result_message(kind, r));
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
                <code>{kind.headers()}</code>
                ". Common alternatives like \"First name\" + \"Surname\" or \"Serial number\" are recognised."
            </p>
            <div class="row wrap">
                <label class="btn file-btn">
                    "Choose CSV file…"
                    <input id=format!("csv-file-{kind:?}") type="file" accept=".csv,.txt,text/csv" on:change=on_file />
                </label>
                <button type="button" class="btn ghost small" on:click=move |_| { text.set(kind.example().to_string()); reparse(); }>
                    "Show an example"
                </button>
            </div>
            <label class="label" for=format!("csv-text-{kind:?}")>"…or paste CSV here"</label>
            <textarea
                id=format!("csv-text-{kind:?}")
                class="input mono"
                rows="5"
                placeholder=kind.example()
                prop:value=move || text.get()
                on:input=move |ev| { text.set(event_target_value(&ev)); reparse(); }
            ></textarea>

            {move || match parsed.get() {
                Some(Err(e)) => view! { <p class="form-error">{e}</p> }.into_any(),
                None => ().into_any(),
                Some(Ok(_)) => view! {
                    <div class="import-summary">
                        <span class="pill ok">{move || format!("{} new", tally.get().new)}</span>
                        <span class="pill">{move || format!("{} already here", tally.get().existing)}</span>
                        <span class="pill late">{move || format!("{} with problems", tally.get().bad)}</span>
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
                                    {kind.columns().map(|c| view! { <th>{c}</th> }).collect_view()}
                                    <th>"Status"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {move || {
                                    preview.get().unwrap_or_default().into_iter().take(200).map(|row| {
                                        let status = match (&row.problem, row.exists) {
                                            (Some(p), _) => view! { <span class="pill late">{p.clone()}</span> }.into_any(),
                                            (None, true) => view! { <span class="pill">"Already here"</span> }.into_any(),
                                            (None, false) => view! { <span class="pill ok">"New"</span> }.into_any(),
                                        };
                                        view! {
                                            <tr class:bad=row.problem.is_some()>
                                                <td class="num">{row.line}</td>
                                                {row.cells.into_iter().map(|c| view! { <td>{c}</td> }).collect_view()}
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
                            disabled=move || !tally.get().can_import(update_existing.get())
                            on:click=run.clone()
                        >
                            {move || tally.get().button_label(update_existing.get())}
                        </button>
                    </div>
                }
                .into_any(),
            }}
        </section>
    }
}
