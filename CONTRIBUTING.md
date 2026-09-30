# Contributing

Bug reports, documentation improvements and pull requests are welcome.

## Run locally

Install Rust 1.88 or newer, a C/C++ compiler for native SQLite builds, and
LLVM/Clang on `PATH` for WebAssembly builds (see [setup](README.md#development)), then:

```sh
git clone https://github.com/gregorycarnegie/laptop-library.git
cd laptop-library
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
trunk serve --open
```

Use sample data or a disposable database while developing. Back up real loan data
before testing changes to storage or imports.

## Before opening a pull request

Keep changes focused and add or update tests when behaviour changes. Run:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --target wasm32-unknown-unknown
trunk build --release
```

Browser tests need Chrome, a matching ChromeDriver, and `wasm-bindgen-cli` at the
same version as `wasm-bindgen` in `Cargo.lock`. See the
[testing guide](README.md#testing) for setup and optional deeper checks.
Commit `Cargo.lock` alongside dependency changes.

Describe the problem, your change and the checks you ran in the pull request.
For user-visible changes, add an entry under **Unreleased** in [CHANGELOG.md](CHANGELOG.md).

## Report a problem

[Open an issue](https://github.com/gregorycarnegie/laptop-library/issues) with
reproduction steps, expected and actual behaviour, and your browser and operating
system. Use fictional borrowers and asset tags; do not attach real personal data
or an unredacted database backup.
