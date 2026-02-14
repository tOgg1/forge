# sv-xdr - Delegation rules engine (2026-02-13)

## Scope shipped
- Added deterministic delegation rules engine:
  - `crates/forge-db/src/team_delegation.rs`
- Rule schema supports matching on:
  - `type`
  - `tags_any`
  - `repo_prefix`
  - `priority_min`
  - `path_prefixes`
- Rule targets support:
  - `agent_id`
  - `prompt_name`
- Ordered rule evaluation:
  - first matching rule wins (stable/deterministic)
  - fallback to team defaults when no rules match
- Explain output:
  - per-rule skip/match trace
  - final decision summary line
- Team config integration:
  - `resolve_delegation_for_team(team, payload)` evaluates from `Team.delegation_rules_json`.

## Tests added
- `team_delegation::tests::matches_rule_on_type_tag_and_priority`
- `team_delegation::tests::first_matching_rule_wins_deterministically`
- `team_delegation::tests::falls_back_to_default_target_when_no_rule_matches`
- `team_delegation::tests::repo_and_path_prefix_matching_is_supported`
- `team_delegation::tests::invalid_rule_json_returns_validation_error`
- `team_delegation::tests::explain_text_is_deterministic`
- `team_delegation::tests::resolves_from_team_config_json`

## Validation
```bash
cargo fmt --package forge-db
cargo test -p forge-db team_delegation::tests:: -- --nocapture
cargo test -p forge-db --test team_task_repository_test -- --nocapture
```

Result: all listed tests passed.
