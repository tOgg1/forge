# Rust Final Switch Automation

Task: `forge-0xq` (`PAR-099`)

Goal: one-command cutover, one-command rollback, checklist hook support.

Scripts:
- `scripts/rust-final-switch.sh`
- `scripts/rust-final-switch-checklist-hook.sh`

## Cutover command template

```bash
scripts/rust-final-switch.sh cutover \
  --cutover-cmd '<switch-to-rust-command>' \
  --verify-cmd 'forge --version' \
  --verify-cmd 'forge doctor' \
  --verify-cmd 'forge ps' \
  --hook 'scripts/rust-final-switch-checklist-hook.sh docs/review/rust-final-switch-checklist-log.md' \
  --log-file docs/review/rust-final-switch-run.log
```

## Rollback command template

```bash
scripts/rust-final-switch.sh rollback \
  --rollback-cmd '<switch-back-to-go-command>' \
  --verify-cmd 'forge --version' \
  --verify-cmd 'forge doctor' \
  --verify-cmd 'forge ps' \
  --hook 'scripts/rust-final-switch-checklist-hook.sh docs/review/rust-final-switch-checklist-log.md' \
  --log-file docs/review/rust-final-switch-run.log
```

## Hook contract

`scripts/rust-final-switch.sh --hook <cmd>` exports:
- `FORGE_SWITCH_EVENT` (`pre_switch`, `post_switch`, `verify_start`, `verify_pass`, `verify_fail`)
- `FORGE_SWITCH_MODE` (`cutover` or `rollback`)
- `FORGE_SWITCH_STATUS` (`ok` or `failed`)
- `FORGE_SWITCH_COMMAND` (current command string)
- `FORGE_SWITCH_VERIFY_INDEX`, `FORGE_SWITCH_VERIFY_TOTAL`
- `FORGE_SWITCH_LOG_FILE`

Use this to integrate custom checklist trackers or incident systems.
