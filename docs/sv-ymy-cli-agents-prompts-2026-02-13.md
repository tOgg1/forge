# sv-ymy M8.4: CLI for agents/prompts

Date: 2026-02-13
Task: `sv-ymy`
Epic: `sv-rp2`

## Outcome
Delivered baseline confirmed in current `forge-cli` implementation.

## Scope verification
- `forge agent ls/show/validate` implemented in `crates/forge-cli/src/agent.rs`.
- `forge prompt ls/show/validate` implemented in `crates/forge-cli/src/prompt.rs`.
- JSON output + error handling covered in unit/integration tests.

## Validation run
- `cargo test -p forge-cli --lib agent_show_json -- --nocapture`
- `cargo test -p forge-cli --lib agent_ps_with_agents_json -- --nocapture`
- `cargo test -p forge-cli --lib agent_validate_json_success -- --nocapture`
- `cargo test -p forge-cli --lib agent_validate_returns_nonzero_for_invalid_snapshot -- --nocapture`
- `cargo test -p forge-cli --lib prompt::tests::show_json_outputs_prompt_definition -- --nocapture`
- `cargo test -p forge-cli --lib prompt::tests::validate_returns_nonzero_for_empty_prompt -- --nocapture`
- `cargo test -p forge-cli --lib prompt::tests::validate_json_reports_errors -- --nocapture`
- `cargo test -p forge-cli --lib prompt::tests::validate_succeeds_for_non_empty_prompts -- --nocapture`
- `cargo test -p forge-cli --test prompt_command_test -- --nocapture`

All commands passed.
