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

exec cargo mutants \
  --file 'src/web/**' --file 'src/ui/**' \
  --test-tool cargo \
  --cargo-arg=--target=wasm32-unknown-unknown \
  "${tests[@]}" \
  --minimum-test-timeout 180 \
  "$@"
