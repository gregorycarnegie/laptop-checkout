#!/usr/bin/env bash
# Mutation-tests the browser-only code (src/web and src/ui) against the
# wasm-bindgen-test suites, which run in headless Chrome.
#
# Needs wasm-bindgen-test-runner on PATH (cargo install wasm-bindgen-cli with
# the version in Cargo.lock) and a chromedriver matching the installed Chrome
# (on PATH, or set CHROMEDRIVER). Extra arguments go to cargo-mutants, e.g.
#   scripts/mutants-browser.sh -j 2
set -euo pipefail
cd "$(dirname "$0")/.."

tests=()
for f in tests/browser_*.rs; do
  tests+=("--cargo-test-arg=--test=$(basename "$f" .rs)")
done

# wasm-bindgen-test-runner stops chromedriver with SIGKILL, so Chrome never
# deletes its temporary profile (~200 MB per test binary). Over a mutation run
# that fills the disk, so clear out profiles older than ten minutes as we go.
(
  while sleep 60; do
    find "${TMPDIR:-/tmp}" -maxdepth 1 -name '.org.chromium.Chromium.*' -mmin +10 -exec rm -rf {} + 2>/dev/null
  done
) &
janitor=$!
trap 'kill $janitor 2>/dev/null' EXIT

cargo mutants \
  --file 'src/web/**' --file 'src/ui/**' \
  --test-tool cargo \
  --cargo-arg=--target=wasm32-unknown-unknown \
  "${tests[@]}" \
  --minimum-test-timeout 180 \
  "$@"
