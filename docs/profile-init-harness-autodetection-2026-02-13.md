# sv-hfy: profile init harness autodetection + alias discovery (2026-02-13)

## Scope
- `forge profile init` now performs real detection + profile stub creation.
- Detect installed harness binaries:
  - `amp`, `claude`, `codex`, `droid` (`factory` alias), `opencode`, `pi`.
- Parse shell aliases from:
  - `~/.zsh_aliases`
  - optional live `zsh -ic 'alias'` output
- Map aliases to harness/auth-home hints.
- Instantiate profile stubs from detection output.

## Behavior
- Deterministic outputs:
  - harness list sorted
  - alias map sorted by alias name
  - duplicate alias names: first occurrence wins
- `profile init --json|--jsonl` emits:
  - `imported`
  - `profiles`
  - `harnesses`
  - `aliases` (`name`, `harness`, `command`, optional `auth_home`)
- Human output:
  - prints imported profile count + names
  - clear no-op message when nothing detected

## Env controls (for tests/operators)
- `FORGE_PROFILE_INIT_ALIAS_FILE`: override alias file path
- `FORGE_PROFILE_INIT_SKIP_ZSH_ALIAS=1`: skip `zsh -ic alias` probe

## Tests added
- alias-line parser forms
- deterministic alias parsing + auth-home extraction
- profile instantiation from mixed alias/harness detection
- `profile init` fixture alias-file flow
- harness detection from PATH fixture (`codex` + `factory` -> `codex`,`droid`)

## Notes
- Profile stubs created with `max_concurrency=1`, no credentials.
- Alias-derived profiles include `command_template` + inferred `auth_home` when present.
