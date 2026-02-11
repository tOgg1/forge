# rforged Daemon E2E Report (2026-02-11)

## Goal

Validate daemon-owned loop execution with real binaries:

- start `rforged`
- run `rforge up --spawn-owner daemon`
- execute more than one loop iteration
- verify repo side effect, run counters, logs, and clean daemon stop

## Environment

- tmp root: `/tmp/rforged-daemon-e2e-BPXORq`
- repo: `/tmp/rforged-daemon-e2e-BPXORq/repo`
- config: `/tmp/rforged-daemon-e2e-BPXORq/config.yaml`
- daemon bind: `127.0.0.1:50061`
- profile name: `daemon-e2e-1770790978`
- loop name: `daemon-e2e-loop-1770790978`
- loop id: `b3e1baa1-f32d-4d53-a7e9-cca501483d04`
- short id: `wjcxirxs`

## Procedure

1. Launch `rforged` with explicit `--config` + `--port`.
2. In tmp repo, run:
   - `rforge migrate up`
   - `rforge profile add pi --name <profile> --command "./write_prompt.sh {prompt}"`
   - `rforge up --name <loop> --profile <profile> --prompt-msg "daemon-e2e-line" --max-iterations 2 --interval 1s --spawn-owner daemon --json`
3. Poll `rforge ps --json` until loop reaches `state=stopped` and `runs>=2`.
4. Assert side effect file contains prompt payload at least twice.
5. Assert `rforge logs <short-id>` returns output.
6. Send `SIGTERM` to daemon process and verify shutdown log lines.

## Results

- loop state: `stopped`
- loop runs: `2`
- side effect proof: `daemon_output.txt` contains `daemon-e2e-line` exactly 2 times
- logs proof: `rforge logs wjcxirxs` returned output (preview: `==> daemon-e2e-loop-1770790978 <==`)
- daemon shutdown proof in stderr:
  - `rforged shutdown signal received`
  - `rforged loop runners drained`

## Conclusion

Daemon-mode runtime path is working end-to-end for `up --spawn-owner daemon` with real process ownership, loop execution, and observability.

## Note

During this run, CLI behavior suggested `rforge --config <path>` may not fully isolate backend data sources yet; validation used unique names and explicit cleanup to avoid residue in shared state.
