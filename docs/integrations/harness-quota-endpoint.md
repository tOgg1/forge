# Harness Quota Endpoint

CLI endpoint for quick retrieval of remaining quota across Claude and Codex profiles.

## Script

`scripts/harness-quota-endpoint.sh`

## Examples

```bash
# JSON endpoint output (default)
scripts/harness-quota-endpoint.sh \
  --profiles cc1,cc2,cc3,codex1,codex2,codex3 \
  --out .forge/quota/latest.json

# Human summary
scripts/harness-quota-endpoint.sh --text
```

## Returned fields

- Claude (`cc*`)
  - `session.remaining_pct` (this is the rolling 5h session window)
  - `weekly_all_models.remaining_pct`
  - `weekly_sonnet.remaining_pct`
- Codex (`codex*`)
  - `five_hour.remaining_pct`
  - `weekly.remaining_pct`

## Caveat

This endpoint scrapes interactive slash views (`/usage`, `/status`) via tmux. It is practical and fast, but still best-effort until providers expose stable machine-readable quota APIs.
