# forge-3zw - workflow list/show JSON unwrap slice

Date: 2026-02-14
Task: `forge-3zw`
Scope: `crates/forge-cli/src/workflow.rs`

## Change

- Added local test helpers:
  - `parse_json_or_panic(raw, context)`
  - `array_or_panic(value, context)`
- Replaced `unwrap` callsites in scoped tests:
  - `list_json_output`
  - `list_jsonl_output`
  - `show_workflow_json`

## Validation

```bash
cargo test -p forge-cli --lib list_json_output
cargo test -p forge-cli --lib list_jsonl_output
cargo test -p forge-cli --lib show_workflow_json
```

Result:
- Scoped workflow tests passed.
- Slice converted to explicit handling without changing broader workflow test module behavior.
