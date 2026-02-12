# TUI-107 contextual onboarding and key hints

Task: `forge-45t`

## Scope delivered

- Added first-run contextual onboarding overlay in `crates/forge-tui/src/app.rs`.
- Overlay is tab-aware (`Overview`, `Logs`, `Runs`, `Multi Logs`, `Inbox`) and shows focused key hints + workflow guidance.
- Added dismiss/recall behavior:
  - `i`: dismiss onboarding hints for current tab.
  - `I`: recall onboarding hints for current tab.
- Added global help coverage for onboarding controls in help content.
- Added footer hint updates so dismiss/recall controls stay discoverable.

## Behavior notes

- Onboarding overlay is shown in `UiMode::Main` until dismissed for the active tab.
- Dismiss state is session-local (per app instance), tab-scoped.
- Status bar confirms dismiss/recall actions and no-op states.

## Regression tests

Added tests in `crates/forge-tui/src/app.rs`:

- `first_run_onboarding_overlay_renders_by_default`
- `dismiss_onboarding_hides_overlay_per_tab`
- `recall_onboarding_restores_overlay_for_tab`
