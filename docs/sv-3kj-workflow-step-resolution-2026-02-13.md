# sv-3kj: Workflow step resolution (agent+prompt registry) - 2026-02-13

## Scope
- Added workflow step id aliases for registry-style references:
  - `prompt_id` (alias of `prompt_name`)
  - `agent_id` (alias of `profile`)
- Added deterministic prompt fallback when prompt fields are omitted:
  - autodetect `.forge/prompts/default.md` from workflow repo root
- Added validation for id references with explicit error fields:
  - missing `prompt_id` target file -> `field=prompt_id`
  - missing `agent_id` profile id -> `field=agent_id`
  - conflicting `prompt_id`/`prompt_name` and `agent_id`/`profile`

## Implementation
- Workflow model updates:
  - `crates/forge-cli/src/workflow.rs`
    - `WorkflowStep.prompt_id`
    - `WorkflowStep.agent_id`
- Prompt resolution updates:
  - `resolve_step_prompt` now resolves:
    - `prompt` -> `prompt_path` -> `prompt_id` -> `prompt_name` -> autodetected `default.md`
- Validation updates:
  - canonicalizes id aliases during `validate_workflow`
  - applies default prompt path when prompt fields are absent
  - validates `prompt_id` file existence from repo-local prompt registry
  - validates `agent_id` via profile-id lookup in forge DB

## Tests added
- `run_human_step_accepts_prompt_id_reference`
- `validate_prompt_id_not_found`
- `validate_uses_default_prompt_file_when_missing_prompt_fields`
- `validate_agent_id_not_found`
- `resolve_prompt_id`

## Validation
- `cargo fmt -p forge-cli`
- `cargo test -p forge-cli --lib workflow::tests:: -- --nocapture`
- `cargo build -p forge-cli`

## Note
- Full crate test command (`cargo test -p forge-cli`) is currently blocked by unrelated integration-test drift in `crates/forge-cli/tests/prompt_command_test.rs` (trait mismatch, outside this task scope).
