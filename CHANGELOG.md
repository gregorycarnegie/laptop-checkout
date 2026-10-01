# Changelog

Notable changes to Laptop Library are recorded here.

## Unreleased

### Added

- README status badges, an app link and a contribution guide.

### Changed

- Foreign keys are now enforced, and checking out to a missing or inactive
  borrower is refused.
- Checking in or renewing a loan that's already returned is refused instead of
  silently succeeding.
- Writes that change nothing no longer trigger a save.
- A broken query panics in debug builds (and tests) instead of showing an empty list.
- Expanded the project description and setup documentation.
- Updated application and test dependencies, including Leptos, rusqlite,
  wasm-bindgen, fake, mockall and rstest.

Earlier development history is available in the
[commit log](https://github.com/gregorycarnegie/laptop-library/commits/main/).
