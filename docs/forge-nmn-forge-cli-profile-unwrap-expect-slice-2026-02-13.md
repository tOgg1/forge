# forge-nmn: forge-cli profile unwrap/expect test slice (2026-02-13)

## Scope
Remove clippy `unwrap_used` / `expect_used` callsites reported in `crates/forge-cli/src/profile.rs` test helpers/tests.

## Changes
Updated six callsites:

- `env_test_lock` mutex acquire now uses `match` + panic on poisoned lock
- profile instantiation test uses `match` instead of `unwrap`
- profile init JSON parse uses `match` instead of `unwrap`
- harness fixture setup (`create_dir_all`, `write` x2) now uses explicit error checks

## Validation
Commands run:

```bash
cargo clippy -p forge-cli --all-targets -- -D warnings
cargo clippy -p forge-cli --all-targets -- -D warnings -A clippy::unwrap_used -A clippy::expect_used
cargo test -p forge-cli --lib profile_init_uses_alias_fixture_file
cargo test -p forge-cli --lib detect_installed_harnesses_from_path_fixture
```

Results:

- full clippy still fails elsewhere, but no remaining `profile.rs` diagnostics
- clippy slice with unwrap/expect allowed passed
- focused profile tests passed
