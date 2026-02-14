# forge-rd3: completion help probe side-effects (2026-02-13)

## Problem
`forge completion` help discovery probed nested command paths and could execute side-effectful subcommands (notably under `template` / `prompt`) while collecting snapshots.

## Fix
- Updated `crates/forge-cli/src/completion.rs`:
  - Added `should_skip_help_probe(path)`.
  - Skip nested help probes for `template/*` and `prompt/*` leaf paths.
  - For non-skipped nested paths, use `<path...> --help` probe.
- This preserves useful leaf help discovery where available while blocking known side-effectful families from execution during completion generation.

## Validation
- `cargo test -p forge-cli completion_generation_does_not_invoke_editor -- --nocapture` ✅
- `cargo test -p forge-cli completion::tests::bash_contains_start_function -- --nocapture` ✅

## Notes
- `cargo test -p forge-cli --test completion_command_test` currently fails in this workspace due broad command-surface golden drift (new command families/subcommands already present in tree vs checked-in completion goldens), not from this change set.
