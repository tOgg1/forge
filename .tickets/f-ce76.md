---
id: f-ce76
status: open
deps: [f-f710, f-e441, f-c44a]
links: []
created: 2026-01-10T20:12:51Z
type: task
priority: 3
assignee: Tormod Haugland
parent: f-c2d0
---
# Cross-host sync: relay messages between forged instances

Implement optional cross-host synchronization for Forge Mail.

Spec notes:
- Single-host is default
- Cross-host requires explicit relay configuration

Scope:
- Define relay configuration format (how hosts discover each other, auth/trust assumptions)
- Implement message replication between forged instances for the same project ID
- Ensure deduplication and sane ordering across hosts
- Include host field so consumers can identify origin

References:
- docs/forge-mail/SPEC.md (Cross-Host Sync)

## Acceptance Criteria

- With relay configured, sending on host A is delivered on host B for the same project ID
- Deduplication prevents double-delivery loops
- Basic integration test covers two server instances exchanging messages

