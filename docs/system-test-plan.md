# Forge System Test Plan (Loop-First, Non-Legacy)

This plan tests the current **loop-first** workflow and non-legacy CLI/TUI.

Notes:
- Legacy command groups wired via `addLegacyCommand(...)` are out of scope and
  not required for Rust rewrite parity. See: `docs/rust-legacy-drop-list.md`.
- Use this plan to validate post-cutover behavior at a human level (smoke +
  basic regressions).

## Table of Contents

1. Prerequisites
2. Critical Path Tests
3. Core Workflow Tests
4. TUI Tests
5. Multi-Loop Orchestration Tests
6. Secondary Tests
7. Results Template

## 1. Prerequisites

### 1.1 Build + doctor

```bash
make build
./build/forge --version
./build/forge doctor
```

Expected (high level):
- tmux present
- git present
- database accessible + migrations applied
- at least one harness reachable (if configured)

### 1.2 Config + profiles + pools

```bash
./build/forge config path
./build/forge profile init

./build/forge pool create default
./build/forge pool add default <profile>
./build/forge pool set-default default
```

Reference: `docs/config.md`, `docs/quickstart.md`, `docs/cli.md`.

## 2. Critical Path Tests

Goal: repo -> start loop -> message -> logs -> stop.

### 2.1 Single loop happy path (MUST PASS)

```bash
cd /path/to/your/repo
git status

./build/forge init
./build/forge migrate up

./build/forge up --name happy --count 1 --max-iterations 1
./build/forge ps

./build/forge msg happy "List the files in this repo. Print only file paths."
./build/forge logs happy -f

./build/forge stop happy
./build/forge rm happy
```

Expected results:
- loop starts and completes at least one iteration
- message is delivered and appears in logs
- stop + rm work without leaving stale state

### 2.2 TUI happy path (MUST PASS)

```bash
cd /path/to/your/repo
./build/forge up --name tui-happy --count 1
./build/forge
```

In TUI, verify:
- loops render in list
- navigation works (j/k or arrows)
- logs view shows output
- stop/kill/remove confirmations work

Cleanup:

```bash
./build/forge stop tui-happy
./build/forge rm tui-happy
```

## 3. Core Workflow Tests

### 3.1 Multiple loops + pool selection

```bash
./build/forge up --pool default --count 2
./build/forge ps
./build/forge stop --pool default
```

Expected:
- both loops start
- stop selector stops all targeted loops

### 3.2 Resume behavior

```bash
./build/forge up --name resume-test --count 1
./build/forge stop resume-test
./build/forge resume resume-test
./build/forge stop resume-test
./build/forge rm resume-test
```

Expected:
- resume restarts runner cleanly

### 3.3 Smart stop (smoke)

Quantitative stop example:

```bash
./build/forge up --name qstop \
  --quantitative-stop-cmd 'echo 0' \
  --quantitative-stop-exit-codes 0
./build/forge ps
./build/forge rm qstop --force
```

Qualitative stop (requires prompt + agent support):
- validate wiring only; do not expect deterministic stop without a real harness

## 4. TUI Tests

### 4.1 Tabs + keybindings (smoke)

```bash
./build/forge
```

Verify (see `docs/cli.md` for key list):
- switch tabs
- filter mode
- pin/unpin and multi-log layout changes (if present)

## 5. Multi-Loop Orchestration Tests

### 5.1 Scale up/down

```bash
./build/forge scale --count 3 --pool default
./build/forge ps
./build/forge scale --count 0 --pool default --kill
./build/forge ps
```

Expected:
- scale creates/stops expected number of loops
- kill leaves loops stopped/removed as expected

## 6. Secondary Tests

### 6.1 JSON output (smoke)

```bash
./build/forge ps --json | jq '.[]? | {name,state}'
```

### 6.2 Database migration idempotency

```bash
./build/forge migrate up
./build/forge migrate status
./build/forge migrate version
```

## 7. Results Template

Fill after running:

| Section | Result | Notes | Evidence |
|---|---|---|---|
| Prereqs | _TBD_ | _TBD_ | _TBD_ |
| Critical path | _TBD_ | _TBD_ | _TBD_ |
| Core workflows | _TBD_ | _TBD_ | _TBD_ |
| TUI | _TBD_ | _TBD_ | _TBD_ |
| Multi-loop | _TBD_ | _TBD_ | _TBD_ |
| Secondary | _TBD_ | _TBD_ | _TBD_ |

