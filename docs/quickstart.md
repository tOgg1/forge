# Forge Quickstart

This guide walks through building Forge, configuring it, and the first steps to
create a workspace and spawn agents. The CLI is still early-stage; commands that
are not implemented yet are marked as planned.

## Prerequisites

- Go 1.25+ (see `go.mod`)
- Git
- tmux (required for workspace/agent orchestration)
- ssh (for remote nodes)

## Bootstrap a node (optional)

Use the bootstrap script to install dependencies on a fresh node.

```bash
# One-liner (downloads + verifies bootstrap.sh before running)
curl -fsSL https://raw.githubusercontent.com/tOgg1/forge/main/scripts/install.sh | bash -s -- --install-extras --install-claude

# Manual download + verify
curl -fsSL https://raw.githubusercontent.com/tOgg1/forge/main/scripts/bootstrap.sh -o bootstrap.sh
curl -fsSL https://raw.githubusercontent.com/tOgg1/forge/main/scripts/bootstrap.sh.sha256 -o bootstrap.sh.sha256
sha256sum -c bootstrap.sh.sha256
sudo bash bootstrap.sh --install-extras --install-claude
```

Notes:
- `--install-claude` is opt-in; omit it if you do not want Claude Code installed.
- `scripts/install.sh` verifies `bootstrap.sh` against `bootstrap.sh.sha256`.
- The checksum file in `scripts/bootstrap.sh.sha256` must be kept in sync with the script.

## Build

```bash
make build
```

Binaries are written to `./build/forge` and `./build/forged`.

## Configure

Copy the example config and adjust values as needed:

```bash
mkdir -p ~/.config/forge
cp docs/config.example.yaml ~/.config/forge/config.yaml
```

For a full reference, see `docs/config.md`.

## Initialize the database

```bash
./build/forge migrate up
```

This creates `~/.local/share/forge/forge.db` by default.

## Launch the TUI (preview)

```bash
./build/forge
```

The TUI is currently a stub that prints a placeholder message.

## First workspace

```bash
# Add a local node
./build/forge node add --name local --local

# Create a workspace
./build/forge ws create --node local --path /path/to/repo

# Spawn an agent
./build/forge agent spawn --workspace <workspace-id> --type opencode --count 1
```

## Basic commands

```bash
./build/forge node list
./build/forge ws list
./build/forge agent list
```

## Troubleshooting

See `docs/troubleshooting.md` for common fixes and copy-paste commands.
