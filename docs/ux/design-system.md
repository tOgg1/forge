# Swarm UX Audit and Design System Foundation

This document ties together Swarm's UX audits and the shared design system
artifacts. It is the single starting point for UX consistency work.

## Purpose

- Provide a stable UX foundation for CLI, TUI, and docs.
- Prevent ad-hoc changes that drift from the product's premium feel.
- Give contributors a clear checklist before shipping UX changes.

## Sources of truth

Use these documents as canonical references:

- `docs/ux/terminology.md` for naming and state language.
- `docs/ux/cli-style.md` for CLI output structure and error envelopes.
- `docs/ux/tui-theme.md` for TUI color roles, spacing, and state mapping.
- `docs/ux/journeys.md` for the user journey map and pain points.

## Audit snapshot (current)

Top gaps identified across CLI/TUI/docs:

- First run lacks a guided sequence and clear "next step" summaries.
- CLI outputs are inconsistent across commands and do not always hint recovery.
- TUI selection and focus are not always obvious.
- Destructive actions lack consistent confirmations.
- "Why" and "what next" are missing from several error flows.

This list should be kept short and updated when a gap is resolved.

## Design system foundation

### Language

- Use canonical terms from `docs/ux/terminology.md`.
- Prefer short, direct sentences.
- Include a one-line next step in user-facing errors.

### CLI output

- Human mode: stable tables and labeled blocks; no hidden data.
- JSON/JSONL: machine-safe output, no truncation, no color.
- Errors: follow the JSON error envelope in `docs/ux/cli-style.md`.

### TUI layout

- Use semantic color roles (not raw colors).
- Make focus and selection visible without color alone.
- Keep cards compact and consistent; avoid inconsistent spacing.

### Safety and guardrails

- Confirm destructive operations with explicit prompts.
- Prefer local-only binds for remote services, then forward via SSH.
- Default to the safest path, but allow opt-in power flags.

## UX consistency checklist

Before shipping a UX change, confirm:

- Terminology matches `docs/ux/terminology.md`.
- CLI output follows `docs/ux/cli-style.md` rules.
- TUI visuals respect `docs/ux/tui-theme.md` tokens.
- Errors include a recovery hint.
- The change maps to at least one journey in `docs/ux/journeys.md`.

## Future updates

Add a short section here when a major UX gap is closed. Keep this file
lightweight so it remains a quick reference.
