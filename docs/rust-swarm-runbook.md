# Rust Rewrite Swarm Runbook

Project: `prj-vr0104gr`
Repo: `~/Code/oss--forge/repos/forge`

## Prompt set

- `.forge/prompts/rust-swarm-dev.md`
- `.forge/prompts/rust-swarm-design-parity.md`
- `.forge/prompts/rust-swarm-review.md`
- `.forge/prompts/rust-swarm-stale-auditor.md`
- `.forge/prompts/rust-swarm-committer.md`
- `.forge/prompts/rust-swarm-orchestrator.md`

## Stage 0: preflight

```bash
forge --robot-help
sv --robot-help
sv task --robot-help
sv task count --project prj-vr0104gr --status open
sv task count --project prj-vr0104gr --status in_progress
```

## Stage 1: single-loop proof

```bash
forge up \
  --name rust-dev-proof \
  --profile <DEV_PROFILE> \
  --prompt rust-swarm-dev \
  --max-iterations 1 \
  --tags rust-rewrite,swarm,proof,dev
```

Pass condition before scale:
- loop claims real task
- reads/edits files
- runs validation command
- posts fmail progress

## Stage 2: controlled ramp

```bash
# 2 dev loops first
forge up --name rust-dev-1 --profile <DEV_PROFILE_1> --prompt rust-swarm-dev --max-iterations 0 --tags rust-rewrite,swarm,dev
forge up --name rust-dev-2 --profile <DEV_PROFILE_2> --prompt rust-swarm-dev --max-iterations 0 --tags rust-rewrite,swarm,dev

# add parity/design + review
forge up --name rust-design-1 --profile <DESIGN_PROFILE> --prompt rust-swarm-design-parity --max-iterations 0 --tags rust-rewrite,swarm,design
forge up --name rust-review-1 --profile <REVIEW_PROFILE> --prompt rust-swarm-review --max-iterations 0 --tags rust-rewrite,swarm,review

# add stale auditor + committer
forge up --name rust-stale-1 --profile <OPS_PROFILE> --prompt rust-swarm-stale-auditor --max-iterations 0 --tags rust-rewrite,swarm,stale
forge up --name rust-committer-1 --profile <COMMIT_PROFILE> --prompt rust-swarm-committer --max-iterations 0 --tags rust-rewrite,swarm,committer

# optional orchestrator loop
forge up --name rust-orchestrator-1 --profile <ORCH_PROFILE> --prompt rust-swarm-orchestrator --max-iterations 0 --tags rust-rewrite,swarm,orchestrator
```

## Health check command set

```bash
forge ps
forge ps --json | jq '.[]?'
forge logs --all --since 20m
sv task count --project prj-vr0104gr --status open
sv task count --project prj-vr0104gr --status in_progress
sv task list --project prj-vr0104gr --status in_progress --json
fmail log task -n 200
```

Dogpile signal:
- many loops posting on same task
- high `open` count unchanged

Correction broadcast:

```bash
forge msg --all "Pick OPEN/READY first. IN_PROGRESS only if self-owned or stale takeover >=45m."
```

## Stop/wind-down command set

```bash
forge stop --tag swarm
forge stop --tag rust-rewrite
forge ps
sv task sync
sv project sync
sv task count --project prj-vr0104gr --status open
sv task count --project prj-vr0104gr --status in_progress
```

## Explicit stop criteria

- manual operator hold, or
- `open` task count reaches target and stays stable, and
- no critical parity blockers remain open, and
- no stale `in_progress` tasks without owner response.
