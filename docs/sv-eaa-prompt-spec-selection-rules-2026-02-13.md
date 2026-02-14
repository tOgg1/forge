# sv-eaa - Prompt spec + selection rules (2026-02-13)

## Scope mapping
- Prompt spec and deterministic resolution are already implemented across workflow + loop + CLI layers:
  - `docs/workflows.md`
    - defines step prompt fields: `prompt`, `prompt_path`, `prompt_name`
    - documents resolver precedence: inline `prompt` -> `prompt_path` -> `prompt_name`
  - `crates/forge-cli/src/workflow.rs`
    - validates prompt field presence and normalizes `prompt_name`/`prompt_path`
    - resolves prompt name/path in workflow execution path
  - `crates/forge-loop/src/prompt_composition.rs`
    - runtime prompt composition and precedence behavior
  - `crates/forge-cli/src/prompt_resolution.rs`
    - prompt name vs path resolver for `.forge/prompts/<name>.md`

## Validation run
- Executed:
```bash
cargo test -p forge-loop resolve_base_prompt_precedence_matches_go -- --nocapture
cargo test -p forge-loop resolve_override_prompt_path_and_inline -- --nocapture
```
- Result: both tests passed.

## Note on current forge-cli test surface
- `forge-cli` has additional prompt-resolution coverage (e.g. `prompt_resolution`, `up`, `scale`) but full crate test execution is currently affected by unrelated `workflow`/`workflow_bash_executor` compile drift in this shared workspace.

Given documented prompt schema + deterministic precedence + passing prompt composition tests, `sv-eaa` is treated as delivered baseline.
