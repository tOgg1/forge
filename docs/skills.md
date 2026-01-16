# Agent Skills

Forge keeps harness-agnostic skills in `.agent-skills/` following the Agent Skills
folder format. Harness-specific installs are handled by the installer script.

## Install to configured harnesses

```bash
scripts/install-skills.sh
```

The script reads `~/.config/forge/config.yaml` (or `--config`) and installs the
skills into the harness-specific locations based on the configured profiles.

Options:

- `--config PATH`: explicit Forge config path.
- `--source DIR`: override the skills source directory (default: `.agent-skills`).
- `--dry-run`: show the install plan without writing files.

## Harness mapping defaults

When a profile has `auth_home`, the installer writes to `<auth_home>/skills`.
If `auth_home` is empty, it uses the defaults below:

- `codex` -> `~/.codex/skills`
- `claude` / `claude_code` -> `~/.claude/skills`
- `opencode` -> `~/.config/opencode/skills`
- `pi` -> `~/.pi/skills`
