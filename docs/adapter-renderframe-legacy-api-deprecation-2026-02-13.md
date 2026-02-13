# RenderFrame Legacy API Deprecation Plan (2026-02-13)

Task: `forge-1cf`

## Scope

Legacy `RenderFrame` convenience APIs are still used by some migration-stage callers.
Keep temporary aliases, route to current APIs, and mark deprecated.

## Deprecated Aliases

- `RenderFrame::width()` -> `RenderFrame::size().width`
- `RenderFrame::height()` -> `RenderFrame::size().height`
- `RenderFrame::to_text()` -> `RenderFrame::snapshot()`

## Deletion Gate

- Gate constant: `render::LEGACY_RENDER_FRAME_API_DELETE_GATE = "forge-brp"`
- Removal task: `forge-brp` (Remove dead legacy rendering code)
- Removal condition: no in-repo call sites and migration bake period complete.

## Regression Coverage

`render_frame_legacy_aliases_map_to_current_apis` verifies alias behavior remains a strict remap to current APIs.
