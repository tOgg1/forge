# forge-rd3 - completion help discovery side-effect guard (2026-02-13)

## Summary

Fixed `forge completion` snapshot generation so nested help discovery does not execute side-effectful subcommands (for example `prompt edit --help`, which can invoke `$EDITOR`).

## Root Cause

- Completion snapshot traversal used `<path> --help` for nested command paths.
- Some subcommands treat `--help` as positional input (or otherwise do not short-circuit), which can execute command logic.
- In this environment, that path launched `vim` during `cargo test -p forge-cli --lib` at `tests::completion_module_is_accessible`.

## Changes

- `crates/forge-cli/src/completion.rs`
  - `render_help` now uses:
    - root: `--help`
    - nested: `help <path...>`
  - Added regression test `completion_generation_does_not_invoke_editor`:
    - wires `EDITOR`/`VISUAL` to a probe script
    - asserts completion generation does not invoke editor

## Validation

```bash
cargo test -p forge-cli --lib completion::tests::bash_contains_start_function -- --nocapture
cargo test -p forge-cli --lib completion::tests::completion_generation_does_not_invoke_editor -- --nocapture
cargo test -p forge-cli --lib tests::completion_module_is_accessible -- --nocapture
cargo test -p forge-cli --lib
```

Result: pass (`1468 passed; 0 failed`).
