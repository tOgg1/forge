# TUI-7a8 workflow benchmark gate (<60s first-time flow)

Date: 2026-02-13
Task: `forge-7a8`

## Gate command

```bash
/usr/bin/time -p scripts/rust-frankentui-bootstrap-smoke.sh
```

## Result

- Status: PASS
- Evidence capture: `build/rust-frankentui-bootstrap-smoke.txt`
- Wall clock (`real`): `5.82` seconds
- Threshold: `< 60` seconds
- Gate: PASS (`5.82s < 60s`)

## Notes

- Run executes scripted interactive bootstrap flow (startup, refresh keypath, resize, quit) and verifies runtime markers.
