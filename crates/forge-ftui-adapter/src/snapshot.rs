//! Snapshot helpers for adapter-based render abstractions.

use crate::render::RenderFrame;

/// Assert a stable text snapshot for a render frame.
///
/// `expected` may include a trailing newline; it will be trimmed for comparison.
pub fn assert_render_frame_snapshot(label: &str, frame: &RenderFrame, expected: &str) {
    let expected = expected.trim_end_matches('\n');
    let got = frame.snapshot();
    assert_eq!(
        got, expected,
        "render frame snapshot mismatch ({label})\n--- expected\n{expected}\n--- got\n{got}",
    );
}
