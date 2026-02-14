# CLI Prompt Name Resolution Before Path Fallback

Task: `forge-5xm`  
Date: `2026-02-13`  
Status: delivered

## Scope

- Fix `up` and `scale` prompt handling so `--prompt <name>` resolves to registered prompt files under `.forge/prompts/<name>.md` before path fallback.
- Keep explicit path values as passthrough.
- Add regression tests for both `up` and `scale` sqlite backends.

## Implementation anchors

- Shared resolver:
  - `crates/forge-cli/src/prompt_resolution.rs:7`
- Wire resolver in `up` backends:
  - `crates/forge-cli/src/up.rs:127`
  - `crates/forge-cli/src/up.rs:235`
  - `crates/forge-cli/src/up.rs:243`
- Wire resolver in `scale` backends:
  - `crates/forge-cli/src/scale.rs:179`
  - `crates/forge-cli/src/scale.rs:365`
  - `crates/forge-cli/src/scale.rs:373`
- Module registration:
  - `crates/forge-cli/src/lib.rs`

## Regression tests

- Resolver unit tests:
  - `crates/forge-cli/src/prompt_resolution.rs:63`
  - `crates/forge-cli/src/prompt_resolution.rs:73`
  - `crates/forge-cli/src/prompt_resolution.rs:82`
- `up` sqlite regression:
  - `crates/forge-cli/src/up.rs:1536`
  - `crates/forge-cli/src/up.rs:1567`
- `scale` sqlite regression:
  - `crates/forge-cli/src/scale.rs:1567`
  - `crates/forge-cli/src/scale.rs:1616`

## Validation

- `cargo test -p forge-cli --lib prompt_resolution::tests::resolves_registered_prompt_name_to_repo_prompts_path -- --nocapture`
- `cargo test -p forge-cli --lib up::tests::up_sqlite_backend_resolves_registered_prompt_name_before_path_fallback -- --nocapture`
- `cargo test -p forge-cli --lib scale::tests::scale_sqlite_up_resolves_registered_prompt_name_before_path_fallback -- --nocapture`
- `cargo build -p forge-cli`
