# TUI-101 Information Architecture (forge-cey)

Date: 2026-02-12
Scope: canonical view graph, pane transitions, deterministic focus behavior.

## Canonical views

- `overview`
- `fleet`
- `logs`
- `tasks`
- `analytics`
- `swarm`
- `inbox`
- `incidents`

## View graph (directed)

- `overview` -> `fleet`, `logs`, `tasks`, `analytics`, `swarm`, `inbox`, `incidents`
- `fleet` -> `swarm`, `incidents`
- `logs` -> `tasks`, `analytics`, `incidents`
- `tasks` -> `logs`, `swarm`, `inbox`, `incidents`
- `analytics` -> `logs`, `swarm`, `incidents`
- `swarm` -> `fleet`, `tasks`, `analytics`, `inbox`, `incidents`
- `inbox` -> `tasks`, `swarm`, `incidents`
- `incidents` -> `logs`, `tasks`, `swarm`

## Pane model

Pane ids:

- `nav`
- `main`
- `aux`
- `detail`

Views using four-pane layout:

- `overview`, `fleet`, `tasks`, `analytics`, `swarm`, `incidents`

Views using three-pane layout:

- `logs`, `inbox` (`aux` removed)

## Deterministic focus matrix

Movement keys:

- `L` left
- `R` right
- `U` up
- `D` down
- `N` next focus cycle
- `P` previous focus cycle

Four-pane matrix:

- `nav`: `L=nav R=main U=nav D=nav N=main P=detail`
- `main`: `L=nav R=aux U=main D=detail N=aux P=nav`
- `aux`: `L=main R=aux U=aux D=detail N=detail P=main`
- `detail`: `L=nav R=aux U=main D=detail N=nav P=aux`

Three-pane matrix:

- `nav`: `L=nav R=main U=nav D=nav N=main P=detail`
- `main`: `L=nav R=detail U=main D=detail N=detail P=nav`
- `detail`: `L=main R=detail U=main D=detail N=nav P=main`

## Source of truth

- Code: `crates/forge-tui/src/navigation_graph.rs`
- Tests:
  - view graph adjacency snapshot
  - reachability from `overview`
  - per-layout focus matrix snapshots
  - invalid-pane fallback behavior
