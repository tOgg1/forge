# tui-next roadmap (2026-02-10)

Project:
- ID: `prj-v5pc07bf`
- Name: `tui-next`
- Goal: next-generation Forge TUI with premium logs, orchestration, analytics, and collaboration.

Program epic:
- `forge-k52` TUI-000 Epic: Forge Next-Gen TUI program

Domain epics:
- `forge-v67` TUI-100 Navigation, command palette, and workspace UX
- `forge-3t4` TUI-200 Logs intelligence and semantic rendering
- `forge-zad` TUI-300 Fleet control and safety rails
- `forge-gtx` TUI-400 Run and task analytics cockpit
- `forge-tf7` TUI-500 Swarm orchestration cockpit
- `forge-ty5` TUI-600 Collaboration and handoff flows
- `forge-er1` TUI-700 Reliability, replay, and performance
- `forge-vfd` TUI-800 Personalization and accessibility
- `forge-325` TUI-900 Plugin and extension platform

Included existing logs-highlighting epic:
- `forge-9m4` PAR-100 high-fidelity syntax highlighting (now in `tui-next`, under `forge-3t4`)

Counts snapshot:
- Open tasks in project: 81
- Ready tasks in project: 12

Priority model:
- P0: core interaction, logs core, fleet core, reliability/perf core
- P1: orchestration workflows, analytics core, collaboration core, log drill-down features
- P2: personalization and extension platform

Dependency spine:
- `forge-k52` blocks: `forge-v67`, `forge-3t4`, `forge-zad`, `forge-er1`
- `forge-v67` + `forge-3t4` feed analytics (`forge-gtx`)
- `forge-zad` + `forge-3t4` feed swarm cockpit (`forge-tf7`)
- `forge-er1` gates personalization (`forge-vfd`) and extensions (`forge-325`)

Suggested first spawn set:
- `forge-cey` TUI-101 IA
- `forge-xv4` TUI-201 multi-lane logs model
- `forge-exn` TUI-301 fleet selection engine
- `forge-qxw` TUI-701 incremental render engine
- `forge-5s3` PAR-101 logs corpus pack
