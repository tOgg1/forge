# Stop Tool Runner (M3.1b) - 2026-02-13

Implemented reusable stop-tool execution in `crates/forge-loop/src/stop_rules.rs`.

## Behavior

- Runs `stop.tool` command by executable + args in provided workdir.
- Captures `stdout`, `stderr`, and `exit_code`.
- Maps to boolean stop decision:
  - output token match (`true|false|stop|continue|1|0`) => output decides.
  - otherwise fallback to exit status (`0 => true`, non-zero => false).
- Surfaces execution failures clearly:
  - spawn errors include command context.
  - timeout returns explicit timeout error.

## API

- `StopToolSpec`
- `StopToolRunResult`
- `StopToolDecisionSource`
- `parse_stop_tool_bool(...)`
- `run_stop_tool(...)`

## Verification

- `cargo test -p forge-loop stop_rules::tests:: -- --nocapture`
- `cargo check -p forge-loop`
