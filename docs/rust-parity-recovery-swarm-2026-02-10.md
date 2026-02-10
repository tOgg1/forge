# Rust Parity Recovery Swarm (2026-02-10)

Project: `prj-vr0104gr` (`rust-rewrite`)

## Backlog reset summary

- Created `PAR-001` .. `PAR-104` as open tasks in project.
- Priority distribution:
  - `P0`: 60
  - `P1`: 33
  - `P2`: 9
  - `P3`: 2
- Policy: P0 first, then P1, then P2/P3.

## Active prompt set

- `rust-swarm-dev-codex-continuous`
  - Multi-task loop; continue until stop/no work.
- `rust-swarm-dev-claude-single-task`
  - Exactly one task per loop run.
- `rust-swarm-committer`
  - Commit only coherent validated chunks.
- `rust-swarm-stale-auditor`
  - Reopen stale in-progress tasks.

## Spawn command set (staged)

### Stage 1: proof

```bash
go run ./cmd/forge up --name rust-parity-proof-codex --profile codex2 --prompt rust-swarm-dev-codex-continuous --max-iterations 1 --interval 45s --tags rust-rewrite,parity,swarm,proof,codex

go run ./cmd/forge up --name rust-parity-proof-claude --profile cc2 --prompt rust-swarm-dev-claude-single-task --max-iterations 1 --interval 60s --tags rust-rewrite,parity,swarm,proof,claude
```

### Stage 2: ramp

```bash
# codex continuous workers
go run ./cmd/forge up --name rust-parity-dev-codex2-a --profile codex2 --prompt rust-swarm-dev-codex-continuous --max-iterations 0 --interval 45s --quantitative-stop-cmd 'scripts/swarm-quant-stop.sh --project prj-vr0104gr --open-max 0 --in-progress-max 0 --quiet' --quantitative-stop-exit-codes 0 --quantitative-stop-decision stop --quantitative-stop-when before --quantitative-stop-every 1 --tags rust-rewrite,parity,swarm,dev,codex

go run ./cmd/forge up --name rust-parity-dev-codex3-a --profile codex3 --prompt rust-swarm-dev-codex-continuous --max-iterations 0 --interval 45s --quantitative-stop-cmd 'scripts/swarm-quant-stop.sh --project prj-vr0104gr --open-max 0 --in-progress-max 0 --quiet' --quantitative-stop-exit-codes 0 --quantitative-stop-decision stop --quantitative-stop-when before --quantitative-stop-every 1 --tags rust-rewrite,parity,swarm,dev,codex

# claude single-task workers
go run ./cmd/forge up --name rust-parity-dev-claude1-a --profile cc1 --prompt rust-swarm-dev-claude-single-task --max-iterations 1 --interval 75s --tags rust-rewrite,parity,swarm,dev,claude

go run ./cmd/forge up --name rust-parity-dev-claude3-a --profile cc3 --prompt rust-swarm-dev-claude-single-task --max-iterations 1 --interval 75s --tags rust-rewrite,parity,swarm,dev,claude

# stale auditor
go run ./cmd/forge up --name rust-parity-stale-auditor --profile codex3 --prompt rust-swarm-stale-auditor --max-iterations 0 --interval 15m --tags rust-rewrite,parity,swarm,stale,ops

# committer (claude)
go run ./cmd/forge up --name rust-parity-committer-cc2 --profile cc2 --prompt rust-swarm-committer --max-iterations 0 --interval 6m --tags rust-rewrite,parity,swarm,committer
```

## Health-check command set

```bash
go run ./cmd/forge ps | rg 'rust-parity|NAME|STATE'
go run ./cmd/forge logs --name rust-parity-proof-codex --tail 120
go run ./cmd/forge logs --name rust-parity-proof-claude --tail 120

sv task count --project prj-vr0104gr --status open
sv task count --project prj-vr0104gr --status in_progress
sv task list --project prj-vr0104gr --status in_progress --json

fmail log task -n 200
```

## Stop/wind-down command set

```bash
go run ./cmd/forge stop --tag rust-rewrite
go run ./cmd/forge stop --tag parity
go run ./cmd/forge ps
sv task sync
```

## Explicit stop criteria

- `open == 0` for `prj-vr0104gr`.
- `in_progress == 0` or only explicitly approved blocked tasks remain.
- No stale task older than 45m without owner update.
- Required checks passing for merged work:
  - `./scripts/rust-quality-check.sh`
  - `./scripts/rust-boundary-check.sh`
  - parity harness checks.

## Launch status snapshot (2026-02-10)

Active loops after Stage 2 launch and cleanup:

- `rust-parity-proof-codex` (`codex2`) running
- `rust-parity-dev-codex3-v2` (`codex3`) running
- `rust-parity-proof-claude-cc3` (`cc3`) running
- `rust-parity-dev-claude1-v2` (`cc1`) running
- `rust-parity-stale-v2` (`codex1`) sleeping between audits

Claimed tasks currently in progress:

- `forge-800` `PAR-060` by `rust-parity-proof-codex`
- `forge-j1d` `PAR-059` by `rust-parity-dev-codex3-v2`
- `forge-dr7` `PAR-045` by `rust-parity-proof-claude-cc3`
- `forge-3jg` `PAR-005` by `rust-parity-dev-claude1-v2`

Profile availability notes:

- Some spawn attempts failed with `pinned profile <name> unavailable` (observed on `codex2`, `cc1`, `cc2`, `cc3` depending on timing).
- Retry spawn on a free profile when one task-loop completes (Claude loops are single-task and should free profile on completion).
