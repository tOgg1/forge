# rforge E2E Test Report (2026-02-10)

## Scope

Real `rforge` binary. Isolated tmp repo + isolated tmp sqlite DB.

- tmp root: `/tmp/rforge-e2e-full-v3-gVJP5P`
- repo: `/tmp/rforge-e2e-full-v3-gVJP5P/repo`
- db: `/tmp/rforge-e2e-full-v3-gVJP5P/forge.db`

## Result

- pass: `43`
- fail: `0`
- skip: `1` (`run-ambiguous-prefix` in base matrix; covered separately below)

## Covered command families

- root/version/help
- `migrate up`
- `up` (single + multi + invalid arg combo)
- `ps` + `ls` alias
- `run` + hidden `loop run`
- selector/id-prefix targeting (unique prefix)
- `msg`
- `queue ls/move/rm/clear`
- `stop` (enqueue contract)
- `kill` (state transition to stopped)
- `resume` (from stopped back to running/sleeping/waiting)
- `mem set/get/ls/rm` + missing-key error path
- `audit`
- `export status/events`
- `status`
- `context` + `use --show`
- `wait` error path (idle requires agent/context)
- `rm --all --force`

## Separate ambiguity check

Explicit ambiguity test run in a separate isolated env:

- tmp root: `/tmp/rforge-ambig-pddf`
- created 14 loops
- verified `rforge run <ambiguous-prefix>` returns:
  - `"loop '<prefix>' is ambiguous; matches: ..."`

## Real Loop Execution Proof (live runtime path)

Second isolated run with real binary + real sqlite + writable temp repo:

- tmp root: `/tmp/rforge-real-e2e-1t1LUJ`
- repo: `/tmp/rforge-real-e2e-1t1LUJ/repo`
- db: `/tmp/rforge-real-e2e-1t1LUJ/forge.db`
- data dir: `/tmp/rforge-real-e2e-1t1LUJ/data`

Flow:

1. `rforge migrate up`
2. `rforge profile add pi --name local-e2e --command "cat {prompt} > loop_prompt.txt"`
3. `rforge up --name e2e-loop --profile local-e2e --prompt-msg "hello-rust-loop" --max-iterations 1 --interval 1s`
4. Poll `rforge ps --json` until `state=stopped`
5. Verify side-effect file in repo: `loop_prompt.txt`
6. Verify prefix targeting on logs: `rforge logs <short-id-prefix>`

Observed:

- loop id: `da2ef223-9ce2-4cc7-b613-4411950cad7e`
- short id: `vw4qvb6j`
- final state: `stopped`
- runs: `1`
- file content: `hello-rust-loop`
- logs prefix lookup: pass (`rforge logs vw4`)

## Multi-Loop Execution Proof (real runtime path)

Two isolated multi-loop runs with `--count 3`, `--max-iterations 2`, real sqlite, real repo side-effects:

1. **Default profile concurrency (`1`)**
   - tmp root: `/tmp/rforge-multi-e2e-sKVG9Y`
   - result:
     - loops: `3`
     - runs total: `2`
     - states: `error,stopped`
   - finding: when multiple loops pin to one profile with `max_concurrency=1`, only one loop runs; others error with profile unavailable.

2. **Raised profile concurrency (`3`)**
   - tmp root: `/tmp/rforge-multi-e2e-AL98Pg`
   - profile created with `--max-concurrency 3`
   - result:
     - loops: `3`
     - runs total: `6`
     - per-loop runs: min `2`, max `2`
     - states: `stopped`
     - side-effect lines in repo file: `6`

Conclusion:

- multi-loop runtime execution is working end-to-end.
- operational requirement: pool/profile concurrency must be sized for desired parallel loop count.

## `up` Arg-Matrix E2E (broad coverage)

Isolated run:

- tmp root: `/tmp/rforge-up-matrix-bAXUJ5`
- pass: `32`
- fail: `0`

Covered with real `rforge up` execution and DB assertions:

- `--name`
- `--count`
- `--name-prefix`
- `--pool`
- `--profile`
- `--prompt`
- `--prompt-msg`
- `--interval`
- `--initial-wait`
- `--max-runtime`
- `--max-iterations` (values `1`, `2`, `3`, `0`)
- `--tags`
- `--spawn-owner` (`local`, `daemon`)
- quantitative stop flags: all fields populated and persisted in `metadata_json`
- qualitative stop flags: prompt msg and prompt path variants persisted in `metadata_json`

Additional stop-flag value coverage:

- tmp root: `/tmp/rforge-up-stopvals-lRcOdX`
- pass: `17`
- fail: `0`
- explicit value coverage:
  - quant `when`: `before|after|both`
  - quant `decision`: `stop|continue`
  - quant `stdout_mode`: `any|empty|nonempty`
  - quant `stderr_mode`: `any|empty|nonempty`
  - qual `on_invalid`: `stop` (existing matrix already covered `continue`)

Behavior observed:

- `--initial-wait` with `--spawn-owner daemon` correctly enqueues pending `pause` queue item and records `runner_owner=daemon`.
- `--max-runtime 2s` loop stopped with `last_error` containing `max runtime reached`.
- quantitative config currently persists correctly, but runtime behavior still followed `max-iterations` in smoke case (`c10` ran 3/3), indicating quant rule execution is not yet active in `run_exec` path.
- qualitative config persists and local execution remains stable (`qlocal` reached `state=stopped`, `runs=2`, metadata contained `qual` config).

## Behavioral contracts observed

- `up --help` / `ps --help` print correct help text, but non-zero exit.
- `stop` enqueues `stop_graceful`; does not immediately flip loop state to stopped.
- `resume` only valid from stopped/errored state.
- `context --json` / `use --show --json` use PascalCase keys (`WorkspaceID`, `AgentID`, ...).
