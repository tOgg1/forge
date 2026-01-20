# Workflows

Workflows are TOML files stored in `.forge/workflows/<name>.toml` (committed). File name should match the `name` field.

## Top-level fields

- `name` (string, required)
- `version` (string, optional)
- `description` (string, optional)
- `inputs` (table, optional)
- `outputs` (table, optional)
- `steps` (array of tables, required)
- `hooks` (table, optional)

## Step fields (common)

- `id` (string, required, unique)
- `type` (string, required): `agent|loop|bash|logic|job|workflow|human`
- `name` (string, optional)
- `depends_on` (array of step ids, optional)
- `when` (expr string, optional)
- `inputs` (table, optional)
- `outputs` (table, optional)
- `stop` (table, optional)
- `hooks` (table, optional)

## Step types

- `agent`
  - `prompt` (string, required)
  - `profile` or `pool` (optional)
  - `max_runtime` (duration, optional)
- `loop`
  - `prompt` (string, required)
  - `profile` or `pool` (optional)
  - `interval` (duration, optional)
  - `max_iterations` (int, optional)
- `bash`
  - `cmd` (string, required)
  - `workdir` (string, optional)
- `logic`
  - `if` (expr string, required)
  - `then` (array of step ids, optional)
  - `else` (array of step ids, optional)
- `job`
  - `job_name` (string, required)
  - `params` (table, optional)
- `workflow`
  - `workflow_name` (string, required)
  - `params` (table, optional)
- `human`
  - `prompt` (string, required)
  - `timeout` (duration, optional)

## Stop conditions

Use `stop` on loop steps (or anywhere it makes sense later). Supported fields:

- `stop.expr` (string) — expression like `count(tasks.open) == 0`
- `stop.tool` (table) — `{ name = "tk", args = ["ready"] }`
- `stop.llm` (table) — `{ rubric = "coverage", pass_if = "good" }`

## Hooks

Use `hooks.pre` and `hooks.post` arrays for steps or workflow:

- `hooks.pre = ["bash:./scripts/preflight.sh"]`
- `hooks.post = ["bash:./scripts/collect-logs.sh"]`

## Example

```toml
name = "basic"
version = "0.1"
description = "Plan then build"

[inputs]
repo = "."

[[steps]]
id = "plan"
type = "agent"
prompt = "prompts/plan.md"

[[steps]]
id = "build"
type = "bash"
cmd = "echo build"
depends_on = ["plan"]
```

## CLI

```bash
forge workflow ls
forge workflow show <name>
forge workflow validate <name>
```
