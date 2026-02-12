# M10.12 Agent Flow Test Matrix

Coverage anchors for persistent-agent flow and regressions.

## Core flow

- spawn: `crates/forge-cli/src/agent.rs` test `agent_spawn_json`
- send: `crates/forge-cli/src/agent.rs` test `agent_send_json`
- wait: `crates/forge-cli/src/agent.rs` test `agent_wait_already_in_state`
- interrupt: `crates/forge-cli/src/agent.rs` test `agent_interrupt_json`
- kill: `crates/forge-cli/src/agent.rs` test `agent_kill_json`
- revive: `crates/forge-cli/src/agent.rs` tests `agent_revive_terminal_success_json`, `agent_run_missing_process_auto_policy_revives_with_audit_events`

## Failure and policy paths

- capability mismatch (one-shot vs continuous): `agent_spawn_capability_mismatch_is_actionable`
- daemon unavailable fallback error: `agent_spawn_transport_unavailable_surfaces_fallback_error`
- terminal/missing revive policy gate: `agent_run_terminal_without_auto_revive_policy_errors`
- unknown revive target remediation: `agent_revive_unknown_agent_returns_actionable_error`

## Regression targets

- stale pane/process after restart -> revive by persistent record: `agent_run_missing_process_auto_policy_revives_with_audit_events`
- multi-agent stability under targeted send: `agent_send_with_multiple_agents_keeps_peer_visible_in_ps`
- legacy `subagent` command policy: root unknown-command guard in `crates/forge-cli/tests/root_command_test.rs` (`dropped_legacy_commands_are_unknown`)

## CI gate

- workspace gate command: `cargo test --workspace`
- command is required for task close in loop protocol.
