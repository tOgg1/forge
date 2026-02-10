# rforge logs high-fidelity highlighting task breakdown (2026-02-10)

Project: `prj-vr0104gr` (`rust-rewrite`)
Epic: `forge-9m4` (`PAR-100`)

Goal:
- Bring `rforge logs` highlighting to harness-grade quality (Codex/Claude/Opencode/Pi), not basic line coloring.

Execution order (dependency-driven):
1. `forge-5s3` PAR-101 corpus pack (P0)
2. `forge-mrh` PAR-102 token spec (P0)
3. Parser core: `forge-yqd` PAR-103, `forge-1hy` PAR-104 (P0)
4. Renderer core: `forge-8a5` PAR-105, `forge-0b7` PAR-109 (P0)
5. Stream/perf: `forge-xk3` PAR-110, `forge-h6e` PAR-112 (P0)
6. Validation: `forge-23y` PAR-113, `forge-x4w` PAR-114 (P0)
7. Extensions: `forge-2k6` PAR-106, `forge-7m9` PAR-107, `forge-gnk` PAR-108, `forge-r0x` PAR-111 (P1)
8. Finish: `forge-jxn` PAR-115 docs (P1), `forge-8m7` PAR-116 TUI reuse (P2)

Acceptance gates:
- Corpus coverage >= 50 distinct patterns across target harnesses.
- Deterministic tokenization + rendering snapshots.
- Follow-mode stable under chunk boundaries (no flicker/repaint artifacts).
- Performance budget met on large replay and live tail.
- E2E `rforge logs` suite green for `logs`, `-f`, `--since`, `--lines`, and ID-prefix selection paths.
