---
loop_id: 25faba01-13e3-4216-906b-28800f821fb6
loop_name: rust-parity-proof-claude-cc3
repo_path: /Users/trmd/Code/oss--forge/repos/forge
created_at: 2026-02-10T12:04:29Z
---

# Loop Ledger: rust-parity-proof-claude-cc3

## 2026-02-10T12:12:45Z

- run_id: 52b49020-f12f-4084-befa-e608e2cb7512
- loop_name: rust-parity-proof-claude-cc3
- status: success
- profile: cc3
- harness: claude
- prompt_source: base
- prompt_path: /Users/trmd/Code/oss--forge/repos/forge/.forge/prompts/rust-swarm-dev-claude-single-task.md
- prompt_override: false
- started_at: 2026-02-10T12:04:29Z
- finished_at: 2026-02-10T12:12:45Z
- exit_code: 0

```
## Summary — PAR-045 `fmail` root help/usage/exit-code parity (forge-dr7) ✅ CLOSED

### Changes made

**Files modified:**
- `rust/crates/fmail-cli/src/lib.rs` — Added root-level help, version, robot-help, and fixed unknown command behavior
- `rust/crates/fmail-cli/src/bin/fmail-cli.rs` — Added `set_version()` call with `CARGO_PKG_VERSION`

### Parity achieved

| Behavior | Before | After (matches Go) |
|---|---|---|
| No args | `missing command` stderr, exit 2 | Help text on stdout, exit 0 |
| `--help` / `-h` / `help` | `unknown command` stderr, exit 2 | Help text on stdout, exit 0 |
| `--version` / `-v` | `unknown command` stderr, exit 2 | `fmail version X` on stdout, exit 0 |
| `--robot-help` | `unknown command` stderr, exit 2 | JSON help payload on stdout, exit 0 |
| Unknown command | `unknown command: X` exit 2 | `Error: unknown command "X" for "fmail"` exit 1 |

### New tests added (7)
- `no_args_shows_help_exit_0`
- `help_flag_shows_help_exit_0` (tests `--help`, `-h`, `help`)
- `version_flag_shows_version_exit_0` (tests `--version`, `-v`)
- `unknown_command_error_format_exit_1`
- `robot_help_returns_json_exit_0`
- `robot_help_anywhere_in_args`
- `normalize_robot_help_version_cases`

### Validation
- `cargo fmt --check` ✅
- `cargo clippy -p fmail-cli --all-targets -- -D warnings` ✅
- `cargo test -p fmail-cli` — 163 tests pass ✅

(Note: `fmail send` commands failed due to `forged: project not found` — this is an infrastructure issue unrelated to the task.)
```

