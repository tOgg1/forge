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
