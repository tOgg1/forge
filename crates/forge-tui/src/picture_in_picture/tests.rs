use super::{
    focus_next_pip_window, move_pip_window_to_anchor, pin_pip_window, render_pip_windows,
    resize_pip_window, set_pip_opacity, toggle_pip_collapsed, unpin_pip_window, PiPAnchor,
    PiPSource, PiPState,
};

fn sample_lines(prefix: &str) -> Vec<String> {
    vec![
        format!("{prefix}: line-1"),
        format!("{prefix}: line-2"),
        format!("{prefix}: line-3"),
    ]
}

#[test]
fn pin_same_source_updates_existing_window() {
    let mut state = PiPState::default();
    let first = pin_pip_window(
        &mut state,
        PiPSource::RunsSummary,
        "runs",
        &sample_lines("runs"),
        PiPAnchor::TopRight,
        10,
    );
    let second = pin_pip_window(
        &mut state,
        PiPSource::RunsSummary,
        "runs updated",
        &sample_lines("updated"),
        PiPAnchor::BottomRight,
        20,
    );

    assert_eq!(first, second);
    assert_eq!(state.windows.len(), 1);
    assert_eq!(state.windows[0].title, "runs updated");
    assert_eq!(state.windows[0].anchor, PiPAnchor::BottomRight);
}

#[test]
fn pin_enforces_max_windows_by_dropping_oldest() {
    let mut state = PiPState {
        max_windows: 2,
        ..PiPState::default()
    };

    pin_pip_window(
        &mut state,
        PiPSource::Panel("one".to_owned()),
        "one",
        &sample_lines("one"),
        PiPAnchor::TopLeft,
        1,
    );
    pin_pip_window(
        &mut state,
        PiPSource::Panel("two".to_owned()),
        "two",
        &sample_lines("two"),
        PiPAnchor::TopLeft,
        2,
    );
    pin_pip_window(
        &mut state,
        PiPSource::Panel("three".to_owned()),
        "three",
        &sample_lines("three"),
        PiPAnchor::TopLeft,
        3,
    );

    assert_eq!(state.windows.len(), 2);
    assert_eq!(state.windows[0].title, "two");
    assert_eq!(state.windows[1].title, "three");
}

#[test]
fn opacity_and_resize_clamp_bounds() {
    let mut state = PiPState::default();
    let id = pin_pip_window(
        &mut state,
        PiPSource::Panel("x".to_owned()),
        "x",
        &sample_lines("x"),
        PiPAnchor::TopRight,
        1,
    );

    assert!(set_pip_opacity(&mut state, &id, 5));
    assert!(resize_pip_window(&mut state, &id, 5, 1));
    assert_eq!(state.windows[0].opacity_percent, 20);
    assert_eq!(state.windows[0].width, 20);
    assert_eq!(state.windows[0].height, 4);

    assert!(set_pip_opacity(&mut state, &id, 150));
    assert!(resize_pip_window(&mut state, &id, 999, 999));
    assert_eq!(state.windows[0].opacity_percent, 100);
    assert_eq!(state.windows[0].width, 120);
    assert_eq!(state.windows[0].height, 30);
}

#[test]
fn render_places_windows_in_each_corner() {
    let mut state = PiPState::default();
    let tl = pin_pip_window(
        &mut state,
        PiPSource::Panel("tl".to_owned()),
        "tl",
        &sample_lines("tl"),
        PiPAnchor::TopLeft,
        1,
    );
    let tr = pin_pip_window(
        &mut state,
        PiPSource::Panel("tr".to_owned()),
        "tr",
        &sample_lines("tr"),
        PiPAnchor::TopRight,
        2,
    );
    let bl = pin_pip_window(
        &mut state,
        PiPSource::Panel("bl".to_owned()),
        "bl",
        &sample_lines("bl"),
        PiPAnchor::BottomLeft,
        3,
    );
    let br = pin_pip_window(
        &mut state,
        PiPSource::Panel("br".to_owned()),
        "br",
        &sample_lines("br"),
        PiPAnchor::BottomRight,
        4,
    );

    let windows = render_pip_windows(&state, 160, 50);
    assert_eq!(windows.len(), 4);

    let find = |id: &str| {
        windows
            .iter()
            .find(|window| window.id == id)
            .unwrap_or_else(|| panic!("window not found: {id}"))
    };

    let tl_window = find(&tl);
    let tr_window = find(&tr);
    let bl_window = find(&bl);
    let br_window = find(&br);

    assert!(tl_window.x < tr_window.x);
    assert!(tl_window.y < bl_window.y);
    assert!(br_window.x > bl_window.x);
    assert!(br_window.y >= tr_window.y);
}

#[test]
fn collapsed_window_renders_compact_lines_and_focus_cycle() {
    let mut state = PiPState::default();
    let id_a = pin_pip_window(
        &mut state,
        PiPSource::Panel("a".to_owned()),
        "A",
        &sample_lines("a"),
        PiPAnchor::TopLeft,
        1,
    );
    let id_b = pin_pip_window(
        &mut state,
        PiPSource::Panel("b".to_owned()),
        "B",
        &sample_lines("b"),
        PiPAnchor::TopLeft,
        2,
    );

    assert!(toggle_pip_collapsed(&mut state, &id_a));
    assert!(move_pip_window_to_anchor(
        &mut state,
        &id_a,
        PiPAnchor::BottomRight,
        3,
        2
    ));

    let first_focus = match focus_next_pip_window(&mut state) {
        Some(window_id) => window_id,
        None => panic!("focus should return a window"),
    };
    let second_focus = match focus_next_pip_window(&mut state) {
        Some(window_id) => window_id,
        None => panic!("focus should return a window"),
    };
    assert_ne!(first_focus, second_focus);

    let windows = render_pip_windows(&state, 120, 40);
    let collapsed = windows
        .iter()
        .find(|window| window.id == id_a)
        .unwrap_or_else(|| panic!("missing collapsed window"));
    assert_eq!(collapsed.height, 3);
    assert!(collapsed
        .lines
        .iter()
        .any(|line| line.contains("collapsed")));

    assert!(unpin_pip_window(&mut state, &id_b));
    assert_eq!(state.windows.len(), 1);
}
