//! Canonical navigation IA for next-gen Forge TUI surfaces.
//!
//! Tracks two deterministic contracts:
//! - view-to-view transitions between major operator workspaces
//! - pane focus transitions inside each view

/// Canonical major views for next-gen operator workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuiView {
    Overview,
    Fleet,
    Logs,
    Tasks,
    Analytics,
    Swarm,
    Inbox,
    Incidents,
}

impl TuiView {
    pub const ORDER: [TuiView; 8] = [
        TuiView::Overview,
        TuiView::Fleet,
        TuiView::Logs,
        TuiView::Tasks,
        TuiView::Analytics,
        TuiView::Swarm,
        TuiView::Inbox,
        TuiView::Incidents,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Fleet => "fleet",
            Self::Logs => "logs",
            Self::Tasks => "tasks",
            Self::Analytics => "analytics",
            Self::Swarm => "swarm",
            Self::Inbox => "inbox",
            Self::Incidents => "incidents",
        }
    }
}

/// A directed edge in the view graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewRoute {
    pub from: TuiView,
    pub to: TuiView,
    pub reason: &'static str,
}

pub const VIEW_ROUTES: [ViewRoute; 30] = [
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Fleet,
        reason: "drill into loop ownership and states",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Logs,
        reason: "investigate recent execution output",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Tasks,
        reason: "inspect queued and running task work",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Analytics,
        reason: "check aggregate throughput and error trends",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Swarm,
        reason: "jump to topology and spawn controls",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Inbox,
        reason: "review team coordination updates",
    },
    ViewRoute {
        from: TuiView::Overview,
        to: TuiView::Incidents,
        reason: "review active incidents and alerts",
    },
    ViewRoute {
        from: TuiView::Fleet,
        to: TuiView::Swarm,
        reason: "shift from single-loop to topology-level operations",
    },
    ViewRoute {
        from: TuiView::Fleet,
        to: TuiView::Incidents,
        reason: "triage unhealthy loops",
    },
    ViewRoute {
        from: TuiView::Logs,
        to: TuiView::Tasks,
        reason: "open owning task from selected log context",
    },
    ViewRoute {
        from: TuiView::Logs,
        to: TuiView::Analytics,
        reason: "promote local findings to trend analysis",
    },
    ViewRoute {
        from: TuiView::Logs,
        to: TuiView::Incidents,
        reason: "escalate error streams into incident workflow",
    },
    ViewRoute {
        from: TuiView::Tasks,
        to: TuiView::Logs,
        reason: "inspect transcript for selected task",
    },
    ViewRoute {
        from: TuiView::Tasks,
        to: TuiView::Swarm,
        reason: "act on bottlenecks via template/preset controls",
    },
    ViewRoute {
        from: TuiView::Tasks,
        to: TuiView::Inbox,
        reason: "handoff or request assistance",
    },
    ViewRoute {
        from: TuiView::Tasks,
        to: TuiView::Incidents,
        reason: "escalate blocked or failing work",
    },
    ViewRoute {
        from: TuiView::Analytics,
        to: TuiView::Logs,
        reason: "drill into sampled evidence",
    },
    ViewRoute {
        from: TuiView::Analytics,
        to: TuiView::Swarm,
        reason: "apply optimization actions to active topology",
    },
    ViewRoute {
        from: TuiView::Analytics,
        to: TuiView::Incidents,
        reason: "convert anomaly into formal incident",
    },
    ViewRoute {
        from: TuiView::Swarm,
        to: TuiView::Fleet,
        reason: "inspect loop-level impact of topology changes",
    },
    ViewRoute {
        from: TuiView::Swarm,
        to: TuiView::Tasks,
        reason: "monitor queue changes after orchestration updates",
    },
    ViewRoute {
        from: TuiView::Swarm,
        to: TuiView::Analytics,
        reason: "measure topology impact over time",
    },
    ViewRoute {
        from: TuiView::Swarm,
        to: TuiView::Inbox,
        reason: "broadcast swarm changes to collaborators",
    },
    ViewRoute {
        from: TuiView::Swarm,
        to: TuiView::Incidents,
        reason: "contain active swarm degradation",
    },
    ViewRoute {
        from: TuiView::Inbox,
        to: TuiView::Tasks,
        reason: "turn messages into concrete work",
    },
    ViewRoute {
        from: TuiView::Inbox,
        to: TuiView::Swarm,
        reason: "apply template decisions from thread context",
    },
    ViewRoute {
        from: TuiView::Inbox,
        to: TuiView::Incidents,
        reason: "escalate critical messages",
    },
    ViewRoute {
        from: TuiView::Incidents,
        to: TuiView::Logs,
        reason: "inspect failure evidence",
    },
    ViewRoute {
        from: TuiView::Incidents,
        to: TuiView::Tasks,
        reason: "trace impacted tasks and assignees",
    },
    ViewRoute {
        from: TuiView::Incidents,
        to: TuiView::Swarm,
        reason: "execute mitigation controls",
    },
];

#[must_use]
pub fn can_transition(from: TuiView, to: TuiView) -> bool {
    if from == to {
        return true;
    }
    VIEW_ROUTES
        .iter()
        .any(|route| route.from == from && route.to == to)
}

/// Focusable panes present in each view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaneId {
    Nav,
    Main,
    Aux,
    Detail,
}

impl PaneId {
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Nav => "nav",
            Self::Main => "main",
            Self::Aux => "aux",
            Self::Detail => "detail",
        }
    }
}

/// Deterministic focus movement axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FocusMove {
    Left,
    Right,
    Up,
    Down,
    Next,
    Prev,
}

impl FocusMove {
    pub const ORDER: [FocusMove; 6] = [
        FocusMove::Left,
        FocusMove::Right,
        FocusMove::Up,
        FocusMove::Down,
        FocusMove::Next,
        FocusMove::Prev,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Left => "L",
            Self::Right => "R",
            Self::Up => "U",
            Self::Down => "D",
            Self::Next => "N",
            Self::Prev => "P",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PaneRouting {
    pane: PaneId,
    left: PaneId,
    right: PaneId,
    up: PaneId,
    down: PaneId,
    next: PaneId,
    prev: PaneId,
}

impl PaneRouting {
    #[must_use]
    fn target(self, movement: FocusMove) -> PaneId {
        match movement {
            FocusMove::Left => self.left,
            FocusMove::Right => self.right,
            FocusMove::Up => self.up,
            FocusMove::Down => self.down,
            FocusMove::Next => self.next,
            FocusMove::Prev => self.prev,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ViewFocusSpec {
    view: TuiView,
    default_pane: PaneId,
    panes: &'static [PaneId],
    routes: &'static [PaneRouting],
}

const FOUR_PANES: [PaneId; 4] = [PaneId::Nav, PaneId::Main, PaneId::Aux, PaneId::Detail];
const THREE_PANES: [PaneId; 3] = [PaneId::Nav, PaneId::Main, PaneId::Detail];

const FOUR_PANE_ROUTES: [PaneRouting; 4] = [
    PaneRouting {
        pane: PaneId::Nav,
        left: PaneId::Nav,
        right: PaneId::Main,
        up: PaneId::Nav,
        down: PaneId::Nav,
        next: PaneId::Main,
        prev: PaneId::Detail,
    },
    PaneRouting {
        pane: PaneId::Main,
        left: PaneId::Nav,
        right: PaneId::Aux,
        up: PaneId::Main,
        down: PaneId::Detail,
        next: PaneId::Aux,
        prev: PaneId::Nav,
    },
    PaneRouting {
        pane: PaneId::Aux,
        left: PaneId::Main,
        right: PaneId::Aux,
        up: PaneId::Aux,
        down: PaneId::Detail,
        next: PaneId::Detail,
        prev: PaneId::Main,
    },
    PaneRouting {
        pane: PaneId::Detail,
        left: PaneId::Nav,
        right: PaneId::Aux,
        up: PaneId::Main,
        down: PaneId::Detail,
        next: PaneId::Nav,
        prev: PaneId::Aux,
    },
];

const THREE_PANE_ROUTES: [PaneRouting; 3] = [
    PaneRouting {
        pane: PaneId::Nav,
        left: PaneId::Nav,
        right: PaneId::Main,
        up: PaneId::Nav,
        down: PaneId::Nav,
        next: PaneId::Main,
        prev: PaneId::Detail,
    },
    PaneRouting {
        pane: PaneId::Main,
        left: PaneId::Nav,
        right: PaneId::Detail,
        up: PaneId::Main,
        down: PaneId::Detail,
        next: PaneId::Detail,
        prev: PaneId::Nav,
    },
    PaneRouting {
        pane: PaneId::Detail,
        left: PaneId::Main,
        right: PaneId::Detail,
        up: PaneId::Main,
        down: PaneId::Detail,
        next: PaneId::Nav,
        prev: PaneId::Main,
    },
];

const FOCUS_SPECS: [ViewFocusSpec; 8] = [
    ViewFocusSpec {
        view: TuiView::Overview,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Fleet,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Logs,
        default_pane: PaneId::Main,
        panes: &THREE_PANES,
        routes: &THREE_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Tasks,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Analytics,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Swarm,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Inbox,
        default_pane: PaneId::Main,
        panes: &THREE_PANES,
        routes: &THREE_PANE_ROUTES,
    },
    ViewFocusSpec {
        view: TuiView::Incidents,
        default_pane: PaneId::Main,
        panes: &FOUR_PANES,
        routes: &FOUR_PANE_ROUTES,
    },
];

#[must_use]
pub fn panes_for(view: TuiView) -> &'static [PaneId] {
    focus_spec(view).panes
}

#[must_use]
pub fn default_focus(view: TuiView) -> PaneId {
    focus_spec(view).default_pane
}

#[must_use]
pub fn focus_target(view: TuiView, from: PaneId, movement: FocusMove) -> PaneId {
    let spec = focus_spec(view);
    if !spec.panes.contains(&from) {
        return spec.default_pane;
    }
    spec.routes
        .iter()
        .find(|route| route.pane == from)
        .map_or(spec.default_pane, |route| route.target(movement))
}

fn focus_spec(view: TuiView) -> &'static ViewFocusSpec {
    for spec in &FOCUS_SPECS {
        if spec.view == view {
            return spec;
        }
    }
    &FOCUS_SPECS[0]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoomLayer {
    Fleet,
    Group,
    Loop,
    Task,
    Diff,
}

impl ZoomLayer {
    pub const ORDER: [ZoomLayer; 5] = [
        ZoomLayer::Fleet,
        ZoomLayer::Group,
        ZoomLayer::Loop,
        ZoomLayer::Task,
        ZoomLayer::Diff,
    ];

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Fleet => "fleet",
            Self::Group => "group",
            Self::Loop => "loop",
            Self::Task => "task",
            Self::Diff => "diff",
        }
    }

    #[must_use]
    pub fn default_zoom_percent(self) -> u8 {
        match self {
            Self::Fleet => 20,
            Self::Group => 40,
            Self::Loop => 60,
            Self::Task => 80,
            Self::Diff => 100,
        }
    }

    #[must_use]
    pub fn detail_hint(self) -> &'static str {
        match self {
            Self::Fleet => "bird's-eye dots",
            Self::Group => "clustered loop cards",
            Self::Loop => "single-loop panel",
            Self::Task => "task work-item detail",
            Self::Diff => "code diff hunks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoomCommand {
    In,
    Out,
    Set(ZoomLayer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoomSpatialAnchor {
    pub fleet_cell_x: u16,
    pub fleet_cell_y: u16,
    pub cluster_id: String,
    pub loop_id: String,
    pub task_id: String,
}

impl Default for ZoomSpatialAnchor {
    fn default() -> Self {
        Self {
            fleet_cell_x: 0,
            fleet_cell_y: 0,
            cluster_id: String::new(),
            loop_id: String::new(),
            task_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticZoomState {
    pub layer: ZoomLayer,
    pub zoom_percent: u8,
    pub anchor: ZoomSpatialAnchor,
}

impl Default for SemanticZoomState {
    fn default() -> Self {
        let layer = ZoomLayer::Fleet;
        Self {
            layer,
            zoom_percent: layer.default_zoom_percent(),
            anchor: ZoomSpatialAnchor::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticZoomTransition {
    pub from: ZoomLayer,
    pub to: ZoomLayer,
    pub command: ZoomCommand,
    pub zoom_percent: u8,
    pub anchor_preserved: bool,
    pub detail_hint: &'static str,
}

#[must_use]
pub fn apply_semantic_zoom(
    state: &SemanticZoomState,
    command: ZoomCommand,
) -> (SemanticZoomState, SemanticZoomTransition) {
    let target_layer = match command {
        ZoomCommand::In => zoom_layer_step(state.layer, 1),
        ZoomCommand::Out => zoom_layer_step(state.layer, -1),
        ZoomCommand::Set(layer) => layer,
    };
    let next = SemanticZoomState {
        layer: target_layer,
        zoom_percent: target_layer.default_zoom_percent(),
        anchor: state.anchor.clone(),
    };
    let transition = SemanticZoomTransition {
        from: state.layer,
        to: target_layer,
        command,
        zoom_percent: next.zoom_percent,
        anchor_preserved: next.anchor == state.anchor,
        detail_hint: target_layer.detail_hint(),
    };
    (next, transition)
}

#[must_use]
pub fn zoom_layer_for_percent(percent: u8) -> ZoomLayer {
    match percent {
        0..=29 => ZoomLayer::Fleet,
        30..=49 => ZoomLayer::Group,
        50..=69 => ZoomLayer::Loop,
        70..=89 => ZoomLayer::Task,
        _ => ZoomLayer::Diff,
    }
}

#[must_use]
pub fn semantic_zoom_status_rows(state: &SemanticZoomState, max_rows: usize) -> Vec<String> {
    if max_rows == 0 {
        return Vec::new();
    }
    let mut rows = vec![
        format!(
            "zoom:{} ({:>3}%) {}",
            state.layer.slug(),
            state.zoom_percent,
            state.layer.detail_hint()
        ),
        format!(
            "anchor:fleet=({}, {}) cluster={} loop={} task={}",
            state.anchor.fleet_cell_x,
            state.anchor.fleet_cell_y,
            if state.anchor.cluster_id.is_empty() {
                "-"
            } else {
                state.anchor.cluster_id.as_str()
            },
            if state.anchor.loop_id.is_empty() {
                "-"
            } else {
                state.anchor.loop_id.as_str()
            },
            if state.anchor.task_id.is_empty() {
                "-"
            } else {
                state.anchor.task_id.as_str()
            },
        ),
    ];
    rows.truncate(max_rows);
    rows
}

fn zoom_layer_step(layer: ZoomLayer, delta: i32) -> ZoomLayer {
    let mut idx = 0i32;
    for (current_idx, candidate) in ZoomLayer::ORDER.iter().enumerate() {
        if *candidate == layer {
            idx = current_idx as i32;
            break;
        }
    }
    let max_idx = (ZoomLayer::ORDER.len() as i32) - 1;
    let next_idx = (idx + delta).clamp(0, max_idx);
    ZoomLayer::ORDER[next_idx as usize]
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        apply_semantic_zoom, can_transition, focus_target, semantic_zoom_status_rows,
        zoom_layer_for_percent, FocusMove, PaneId, SemanticZoomState, TuiView, ViewRoute,
        ZoomCommand, ZoomLayer, ZoomSpatialAnchor, VIEW_ROUTES,
    };

    fn adjacency_snapshot() -> String {
        TuiView::ORDER
            .iter()
            .map(|from| {
                let mut targets = VIEW_ROUTES
                    .iter()
                    .filter(|route| route.from == *from)
                    .map(|route| route.to.slug())
                    .collect::<Vec<&str>>();
                targets.sort_unstable();
                format!("{} -> {}", from.slug(), targets.join(","))
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn focus_matrix_line(view: TuiView, from: PaneId) -> String {
        let cells = FocusMove::ORDER
            .iter()
            .map(|movement| {
                let to = focus_target(view, from, *movement);
                format!("{}={}", movement.slug(), to.slug())
            })
            .collect::<Vec<String>>()
            .join(" ");
        format!("{}: {}", from.slug(), cells)
    }

    fn focus_matrix_snapshot(view: TuiView) -> String {
        super::panes_for(view)
            .iter()
            .map(|pane| focus_matrix_line(view, *pane))
            .collect::<Vec<String>>()
            .join("\n")
    }

    #[test]
    fn canonical_view_graph_snapshot() {
        let snapshot = adjacency_snapshot();
        let expected = [
            "overview -> analytics,fleet,inbox,incidents,logs,swarm,tasks",
            "fleet -> incidents,swarm",
            "logs -> analytics,incidents,tasks",
            "tasks -> inbox,incidents,logs,swarm",
            "analytics -> incidents,logs,swarm",
            "swarm -> analytics,fleet,inbox,incidents,tasks",
            "inbox -> incidents,swarm,tasks",
            "incidents -> logs,swarm,tasks",
        ]
        .join("\n");
        assert_eq!(snapshot, expected);
    }

    #[test]
    fn view_routes_reference_only_canonical_views() {
        for ViewRoute { from, to, .. } in VIEW_ROUTES {
            assert!(TuiView::ORDER.contains(&from));
            assert!(TuiView::ORDER.contains(&to));
        }
    }

    #[test]
    fn all_views_are_reachable_from_overview() {
        let mut queue = VecDeque::from([TuiView::Overview]);
        let mut seen = Vec::new();
        while let Some(current) = queue.pop_front() {
            if seen.contains(&current) {
                continue;
            }
            seen.push(current);
            for edge in VIEW_ROUTES.iter().filter(|edge| edge.from == current) {
                queue.push_back(edge.to);
            }
        }
        for view in TuiView::ORDER {
            assert!(seen.contains(&view), "unreachable view: {}", view.slug());
        }
    }

    #[test]
    fn overview_focus_matrix_is_deterministic() {
        let snapshot = focus_matrix_snapshot(TuiView::Overview);
        let expected = [
            "nav: L=nav R=main U=nav D=nav N=main P=detail",
            "main: L=nav R=aux U=main D=detail N=aux P=nav",
            "aux: L=main R=aux U=aux D=detail N=detail P=main",
            "detail: L=nav R=aux U=main D=detail N=nav P=aux",
        ]
        .join("\n");
        assert_eq!(snapshot, expected);
    }

    #[test]
    fn logs_focus_matrix_is_deterministic() {
        let snapshot = focus_matrix_snapshot(TuiView::Logs);
        let expected = [
            "nav: L=nav R=main U=nav D=nav N=main P=detail",
            "main: L=nav R=detail U=main D=detail N=detail P=nav",
            "detail: L=main R=detail U=main D=detail N=nav P=main",
        ]
        .join("\n");
        assert_eq!(snapshot, expected);
    }

    #[test]
    fn inbox_focus_matrix_is_deterministic() {
        let snapshot = focus_matrix_snapshot(TuiView::Inbox);
        let expected = [
            "nav: L=nav R=main U=nav D=nav N=main P=detail",
            "main: L=nav R=detail U=main D=detail N=detail P=nav",
            "detail: L=main R=detail U=main D=detail N=nav P=main",
        ]
        .join("\n");
        assert_eq!(snapshot, expected);
    }

    #[test]
    fn four_pane_views_share_matrix_contract() {
        let expected = focus_matrix_snapshot(TuiView::Overview);
        for view in [
            TuiView::Fleet,
            TuiView::Tasks,
            TuiView::Analytics,
            TuiView::Swarm,
            TuiView::Incidents,
        ] {
            assert_eq!(focus_matrix_snapshot(view), expected);
        }
    }

    #[test]
    fn invalid_pane_falls_back_to_default_focus() {
        let to = focus_target(TuiView::Logs, PaneId::Aux, FocusMove::Right);
        assert_eq!(to, PaneId::Main);
    }

    #[test]
    fn key_investigation_handoffs_are_connected() {
        assert!(can_transition(TuiView::Logs, TuiView::Incidents));
        assert!(can_transition(TuiView::Incidents, TuiView::Logs));
        assert!(can_transition(TuiView::Tasks, TuiView::Inbox));
        assert!(can_transition(TuiView::Inbox, TuiView::Tasks));
    }

    #[test]
    fn semantic_zoom_in_and_out_steps_layers_with_clamp() {
        let mut state = SemanticZoomState::default();
        for _ in 0..8 {
            (state, _) = apply_semantic_zoom(&state, ZoomCommand::In);
        }
        assert_eq!(state.layer, ZoomLayer::Diff);
        assert_eq!(state.zoom_percent, 100);

        for _ in 0..8 {
            (state, _) = apply_semantic_zoom(&state, ZoomCommand::Out);
        }
        assert_eq!(state.layer, ZoomLayer::Fleet);
        assert_eq!(state.zoom_percent, 20);
    }

    #[test]
    fn semantic_zoom_preserves_spatial_anchor_across_layers() {
        let state = SemanticZoomState {
            layer: ZoomLayer::Fleet,
            zoom_percent: 20,
            anchor: ZoomSpatialAnchor {
                fleet_cell_x: 7,
                fleet_cell_y: 3,
                cluster_id: "cluster-night".to_owned(),
                loop_id: "loop-17".to_owned(),
                task_id: "forge-sd4".to_owned(),
            },
        };
        let (next, transition) = apply_semantic_zoom(&state, ZoomCommand::Set(ZoomLayer::Task));
        assert_eq!(next.layer, ZoomLayer::Task);
        assert!(transition.anchor_preserved);
        assert_eq!(next.anchor, state.anchor);
        assert_eq!(transition.detail_hint, "task work-item detail");
    }

    #[test]
    fn zoom_layer_for_percent_uses_semantic_bands() {
        assert_eq!(zoom_layer_for_percent(0), ZoomLayer::Fleet);
        assert_eq!(zoom_layer_for_percent(35), ZoomLayer::Group);
        assert_eq!(zoom_layer_for_percent(65), ZoomLayer::Loop);
        assert_eq!(zoom_layer_for_percent(75), ZoomLayer::Task);
        assert_eq!(zoom_layer_for_percent(100), ZoomLayer::Diff);
    }

    #[test]
    fn semantic_zoom_status_rows_snapshot() {
        let state = SemanticZoomState {
            layer: ZoomLayer::Loop,
            zoom_percent: 60,
            anchor: ZoomSpatialAnchor {
                fleet_cell_x: 12,
                fleet_cell_y: 4,
                cluster_id: "east".to_owned(),
                loop_id: "loop-9".to_owned(),
                task_id: "forge-ecp".to_owned(),
            },
        };
        let rows = semantic_zoom_status_rows(&state, 4);
        assert_eq!(
            rows,
            vec![
                "zoom:loop ( 60%) single-loop panel".to_owned(),
                "anchor:fleet=(12, 4) cluster=east loop=loop-9 task=forge-ecp".to_owned(),
            ]
        );
    }
}
