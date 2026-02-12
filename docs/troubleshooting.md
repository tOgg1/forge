# Forge Troubleshooting

This guide covers common setup and runtime issues with copy-paste fixes.
If you are in JSON mode, add `--no-color` to keep output plain.

## Quick triage

```bash
./build/forge --version
./build/forge doctor
./build/forge ps
./build/forge logs --all
```

If any command fails, see the matching section below.

## Delegated agent failure playbook (`forge agent`)

Quick checks:

```bash
./build/forge agent ps
./build/forge agent show <agent-id>
./build/forge agent run "health-check ping" --agent <agent-id> --wait idle
```

### Error: terminal state (`stopped`/`error`) on delegated run/send

Symptom:
- `agent '<id>' is in terminal state ...; use --revive to restart it`

Fix:
```bash
./build/forge agent run "continue previous task" --agent <agent-id> --revive --wait idle
```

### Error: harness mode / capability mismatch

Symptom:
- spawn fails with mode/capability mismatch for continuous agent flow

Cause:
- selected harness/profile is one-shot mode, not interactive/reusable

Fix:
```bash
# use interactive harness command/profile, then retry
./build/forge agent spawn <agent-id> --command codex
./build/forge agent send <agent-id> "resume delegated task"
```

### Error: wait timeout

Symptom:
- wait returns timeout while agent still working or stuck

Fix:
```bash
./build/forge agent show <agent-id>
./build/forge agent wait <agent-id> --until idle --timeout 900
# if stuck:
./build/forge agent interrupt <agent-id>
./build/forge agent run "continue from last stable point" --agent <agent-id> --wait idle
```

### Error: old command name `subagent` no longer exists

Symptom:
- `unknown command: subagent`

Fix (exact replacements):
```bash
# old
./build/forge subagent run "triage failures"
./build/forge subagent send reviewer-1 "follow up"

# new
./build/forge agent run "triage failures"
./build/forge agent send reviewer-1 "follow up"
```

## tmux not found

Symptoms:
- `tmux is required for this command`
- workspace/agent commands fail immediately

Fix (Linux):
```bash
sudo apt-get update && sudo apt-get install -y tmux
```

Fix (macOS):
```bash
brew install tmux
```

Verify:
```bash
tmux -V
```

## Config file missing

Symptoms:
- warning about missing config
- config-related errors during startup

Fix (manual setup):
```bash
mkdir -p ~/.config/forge
cp docs/config.example.yaml ~/.config/forge/config.yaml
```

If your build supports `forge init`, you can run:
```bash
./build/forge init
```

## Database not migrated

Symptoms:
- `database not migrated`
- `database has no migrations applied`

Fix:
```bash
./build/forge migrate up
```

If you customized the data directory, confirm the config:
```bash
rg -n "data_dir|database.path" ~/.config/forge/config.yaml
```

## SSH issues (remote nodes)

Symptoms:
- connection test fails
- `ssh binary not found for system backend`
- `permission denied` when connecting to a remote host

Fix (system ssh):
```bash
ssh -T user@host
```

If you rely on system SSH and do not have the binary installed, install it or
switch to the native backend in `config.yaml`:
```yaml
node_defaults:
  ssh_backend: native
```

## tmux session or pane missing

Symptoms:
- runner operations fail with pane/session errors
- `pane is dead or inaccessible`

Fix:
```bash
tmux ls
```

If a loop runner appears stale, recheck state:

```bash
./build/forge ps --json | jq '.[]? | {name,state,runner_pid_alive,runner_daemon_alive,reason}'
```

Then stop/kill and resume as needed:

```bash
./build/forge stop <loop-name>
./build/forge resume <loop-name>
```

## Daemon unavailable (`--spawn-owner daemon`)

Symptoms:
- `forged daemon unavailable: ...`
- `up`/`resume` fails when `--spawn-owner daemon` is set

Fix:
```bash
make build-rust-cli build-rust-daemon
./build/rforged --config ~/.config/forge/config.yaml --port 50061
export FORGE_DAEMON_TARGET=http://127.0.0.1:50061
./build/rforge up --name daemon-retry --profile <profile> --spawn-owner daemon
```

If daemon ownership is optional, use fallback mode:
```bash
./build/rforge up --name daemon-auto --profile <profile> --spawn-owner auto
```

## Daemon port already in use

Symptoms:
- `failed to listen on 127.0.0.1:50051: address already in use`
- `rforged failed` immediately on startup

Fix:
```bash
./build/rforged --port 50061
export FORGE_DAEMON_TARGET=http://127.0.0.1:50061
```

If you expect default port `50051`, stop the conflicting daemon/process first.

## Daemon-owned loop becomes stopped unexpectedly

Symptoms:
- loop was running, then `rforge ps` reports `state=stopped`
- JSON row shows `runner_owner=daemon` and `runner_daemon_alive=false`

Inspect and recover:
```bash
./build/rforge ps --json | jq '.[]? | {name,state,runs,runner_owner,runner_daemon_alive}'
./build/rforge logs <loop-name-or-short-id>
./build/rforge resume <loop-name-or-short-id> --spawn-owner daemon
```

Verify post-recovery:
```bash
./build/rforge status --json | jq '.alerts.items[]? | select(.message | test("runner health check failed"))'
```

## Loop stuck / no progress

Symptoms:
- no new output across iterations
- repeated failures with no progress

Fix:
```bash
./build/forge logs <loop-name> -f
```

## Need more detail

Run with verbose logs:
```bash
./build/forge --verbose <command>
```

If you are still stuck, collect:
- `forge --version`
- the exact command you ran
- the error output
