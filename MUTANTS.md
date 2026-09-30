# Mutation testing

[cargo-mutants](https://mutants.rs) makes small changes to the code (a `<`
becomes `<=`, a `-` is deleted, a function returns a default) and runs the
tests against each one. A mutant is **caught** when a test fails, **missed**
when every test still passes, and **unviable** when the change doesn't compile.

## Latest results

| Pass | Mutants | Caught | Unviable | Missed |
| --- | ---: | ---: | ---: | ---: |
| Core logic, native tests (`time`, `email`, `csv_import`, `db`, `repo`, `persist`, `alerts`, `view_model`, `import`, `models`) | 843 | 761 → **all viable** | 54 | 28 → **0** |
| Browser code, headless Chrome (`web/`, `ui/`) | 116 | 97 | 19 | **0** |

**No missed mutants remain, so there are no equivalent survivors to explain.**
The 28 core-logic survivors from the first full run were all real gaps; each was
closed by a new test (and three by fixing code), then the affected functions
were re-run until everything was caught.

## What the survivors found

| Survivor | What it showed | Fix |
| --- | --- | --- |
| `csv_import::borrowers`: `\|\|` → `&&` (2) | No test had a header row with a name column but no email column | Tests for each single known column |
| `repo::seed_sample`: 20 arithmetic and comparison mutants | Nothing pinned the example data's dates or email history | `insta` snapshot of the whole example loan history, and an invariant test that loans have a last-emailed date exactly when they've been emailed |
| `time::from_wall`: `-` → `+` | **Bug**: in a zone whose clocks go forward at midnight, "start of day" was 23:00 the previous day | Rewrote `from_wall` to handle clock changes (skipped times move forward, repeated times use the first, as JavaScript's `Date` does), with tests for each case |
| `time::from_wall` (after the rewrite): `-` → `/` | The test clock could only model one clock change | Test clock now models a zone's history of changes |
| `time::describe_due`, `view_model` `LoanFilter::keep`, `LaptopFilter::keep`, `email_purpose`: `<` → `<=` | Nothing tested the exact due instant | Boundary tests: due now is not late, one millisecond later is |
| `time::clock::offset`: `<` → `<=` | The test clock's switch instant wasn't tested | Boundary test |
| `view_model::emailable` → `vec![1]` | The test's only expected answer was `[1]` | More cases |

Other bugs found while building the suite (by fuzzing, property tests and
browser tests rather than mutants): an arithmetic overflow for huge years in
date inputs, IndexedDB connections that were never closed, a CSV delimiter
tie-break, a misaligned email template and an email queue that kept the
previous queue's template.

## Skipped on purpose

`#[mutants::skip]` marks the few functions no test can meaningfully judge:

* `main` — the entry point only; `ui::start`, which it calls, is tested in Chrome.
* `db::log` — writes a diagnostic to the console.
* `time::platform::{now, offset}` (browser build) — read the browser's clock and
  timezone. They don't exist in the native build that runs the core pass; the
  browser test `the_browser_clock_and_timezone_are_used` checks them.

## Running it

```sh
# Core logic (about 70 minutes with -j 4):
cargo mutants --exclude 'src/ui/**' --exclude 'src/web/**' --exclude src/main.rs -j 4
# Browser code (about 2 hours with -j 2; needs chromedriver and wasm-bindgen-test-runner):
scripts/mutants-browser.sh -j 2
```

Each job keeps a full build copy, so allow roughly 5 GB of disk per job for the
browser pass.
