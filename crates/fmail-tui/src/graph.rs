//! Graph view for the fmail TUI, ported from Go `graphView`.
//!
//! Visualizes agent communication as an ASCII-art directed graph with circular
//! layout, topic overlay mode, zoom/pan, node selection, and a details panel.

use forge_ftui_adapter::input::{translate_input, InputEvent, Key, UiAction};
use forge_ftui_adapter::render::{FrameSize, RenderFrame, TextRole};
use forge_ftui_adapter::style::ThemeSpec;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum agents before collapsing into an "others" node.
pub const GRAPH_MAX_NODES: usize = 12;

/// Max zoom level.
const ZOOM_MAX: i32 = 6;

/// Min zoom level.
const ZOOM_MIN: i32 = -3;

/// Maximum topics shown in overlay mode.
const MAX_TOPICS: usize = 10;

// ---------------------------------------------------------------------------
// GraphNode
// ---------------------------------------------------------------------------

/// An agent node in the communication graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub name: String,
    pub sent: usize,
    pub recv: usize,
    pub total: usize,
}

// ---------------------------------------------------------------------------
// GraphEdge
// ---------------------------------------------------------------------------

/// A directed edge between two agents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub count: usize,
}

// ---------------------------------------------------------------------------
// GraphTopic
// ---------------------------------------------------------------------------

/// Topic overlay entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphTopic {
    pub name: String,
    pub message_count: usize,
    pub participants: usize,
}

// ---------------------------------------------------------------------------
// GraphSnapshot
// ---------------------------------------------------------------------------

/// Computed snapshot of the communication graph.
#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub messages: usize,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub topics: Vec<GraphTopic>,
    pub agent_topic_edges: Vec<GraphEdge>,
}

// ---------------------------------------------------------------------------
// GraphMessage (input)
// ---------------------------------------------------------------------------

/// Simplified message for graph computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphMessage {
    pub id: String,
    pub from: String,
    pub to: String,
}

// ---------------------------------------------------------------------------
// GraphViewModel
// ---------------------------------------------------------------------------

/// View state for the graph view.
#[derive(Debug)]
pub struct GraphViewModel {
    /// Time window labels (display only).
    pub window_labels: Vec<String>,
    /// Current window index.
    pub window_idx: usize,
    /// Computed graph snapshot.
    pub snap: GraphSnapshot,
    /// Zoom level (-3 to +6).
    pub zoom: i32,
    /// Pan offsets.
    pub pan_x: i32,
    pub pan_y: i32,
    /// Selected node index.
    pub selected: usize,
    /// Whether the details panel is visible.
    pub show_details: bool,
    /// Topic overlay mode.
    pub topic_overlay: bool,
    /// Cluster display toggle.
    pub clusters: bool,
    /// Loading state.
    pub loading: bool,
    /// Error message (if any).
    pub error: Option<String>,
}

impl Default for GraphViewModel {
    fn default() -> Self {
        Self {
            window_labels: vec![
                "1h".into(),
                "4h".into(),
                "12h".into(),
                "24h".into(),
                "7d".into(),
                "all".into(),
            ],
            window_idx: 1,
            snap: GraphSnapshot::default(),
            zoom: 0,
            pan_x: 0,
            pan_y: 0,
            selected: 0,
            show_details: true,
            topic_overlay: false,
            clusters: false,
            loading: false,
            error: None,
        }
    }
}

impl GraphViewModel {
    fn clamp_selection(&mut self) {
        let n = self.snap.nodes.len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    /// Set messages and rebuild the graph snapshot.
    pub fn set_messages(&mut self, messages: &[GraphMessage]) {
        self.snap = build_graph_snapshot(messages, GRAPH_MAX_NODES);
        self.clamp_selection();
    }
}

// ---------------------------------------------------------------------------
// build_graph_snapshot
// ---------------------------------------------------------------------------

/// Build a communication graph from messages.
///
/// Ported from Go `buildGraphSnapshot`.  Topics create broadcast edges
/// among all participants; DMs (`@agent` targets) are direct edges.
pub fn build_graph_snapshot(messages: &[GraphMessage], max_nodes: usize) -> GraphSnapshot {
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    let max_nodes = if max_nodes == 0 {
        GRAPH_MAX_NODES
    } else {
        max_nodes
    };

    // Phase 1: classify messages as DM or topic.
    let mut topic_participants: HashMap<String, BTreeSet<String>> = HashMap::new();
    let mut topic_msgs: HashMap<String, Vec<&GraphMessage>> = HashMap::new();
    let mut topic_counts: HashMap<String, usize> = HashMap::new();
    let mut agent_topic: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut dm_edges: BTreeMap<(String, String), usize> = BTreeMap::new();

    for msg in messages {
        let from = msg.from.trim();
        let to = msg.to.trim();
        if from.is_empty() || to.is_empty() {
            continue;
        }

        if let Some(peer) = to.strip_prefix('@') {
            let peer = peer.trim();
            if peer.is_empty() || peer == from {
                continue;
            }
            *dm_edges
                .entry((from.to_owned(), peer.to_owned()))
                .or_default() += 1;
            continue;
        }

        // Topic message.
        let topic = to.to_owned();
        topic_participants
            .entry(topic.clone())
            .or_default()
            .insert(from.to_owned());
        topic_msgs.entry(topic.clone()).or_default().push(msg);
        *topic_counts.entry(topic.clone()).or_default() += 1;
        *agent_topic.entry((from.to_owned(), topic)).or_default() += 1;
    }

    // Phase 2: build directed agent→agent edges.
    let mut edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (k, v) in &dm_edges {
        *edges.entry(k.clone()).or_default() += v;
    }
    for (topic, msgs) in &topic_msgs {
        let parts = match topic_participants.get(topic) {
            Some(p) if p.len() > 1 => p,
            _ => continue,
        };
        let peers: Vec<&String> = parts.iter().collect();
        for msg in msgs {
            let from = msg.from.trim();
            if from.is_empty() {
                continue;
            }
            for peer in &peers {
                if peer.as_str() == from {
                    continue;
                }
                *edges.entry((from.to_owned(), (*peer).clone())).or_default() += 1;
            }
        }
    }

    // Phase 3: compute node totals.
    let mut node_map: HashMap<String, (usize, usize)> = HashMap::new(); // (sent, recv)
    for ((from, to), count) in &edges {
        if from.trim().is_empty() || to.trim().is_empty() || from == to || *count == 0 {
            continue;
        }
        node_map.entry(from.clone()).or_default().0 += count;
        node_map.entry(to.clone()).or_default().1 += count;
    }
    let mut nodes: Vec<GraphNode> = node_map
        .iter()
        .map(|(name, (sent, recv))| GraphNode {
            name: name.clone(),
            sent: *sent,
            recv: *recv,
            total: sent + recv,
        })
        .collect();
    nodes.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));

    // Phase 4: collapse to max_nodes.
    let use_others = nodes.len() > max_nodes;
    let mut keep: BTreeSet<String> = BTreeSet::new();
    if use_others {
        let limit = max_nodes.saturating_sub(1).max(1);
        for node in nodes.iter().take(limit) {
            keep.insert(node.name.clone());
        }
        keep.insert("others".to_owned());
    }

    let map_node = |name: &str| -> String {
        if !use_others {
            return name.to_owned();
        }
        if keep.contains(name) {
            name.to_owned()
        } else {
            "others".to_owned()
        }
    };

    let mut agg_edges: BTreeMap<(String, String), usize> = BTreeMap::new();
    for ((from, to), count) in &edges {
        let f = map_node(from);
        let t = map_node(to);
        if f == t || *count == 0 {
            continue;
        }
        *agg_edges.entry((f, t)).or_default() += count;
    }

    // Rebuild nodes from aggregated edges.
    let mut final_map: HashMap<String, (usize, usize)> = HashMap::new();
    for ((from, to), count) in &agg_edges {
        final_map.entry(from.clone()).or_default().0 += count;
        final_map.entry(to.clone()).or_default().1 += count;
    }
    let mut final_nodes: Vec<GraphNode> = final_map
        .iter()
        .map(|(name, (sent, recv))| GraphNode {
            name: name.clone(),
            sent: *sent,
            recv: *recv,
            total: sent + recv,
        })
        .collect();
    final_nodes.sort_by(|a, b| b.total.cmp(&a.total).then_with(|| a.name.cmp(&b.name)));

    let mut final_edges: Vec<GraphEdge> = agg_edges
        .iter()
        .map(|((from, to), count)| GraphEdge {
            from: from.clone(),
            to: to.clone(),
            count: *count,
        })
        .collect();
    final_edges.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });

    // Phase 5: topic overlay data (top topics + agent→topic edges).
    let mut topics: Vec<GraphTopic> = topic_counts
        .iter()
        .map(|(name, count)| {
            let parts = topic_participants.get(name).map_or(0, |s| s.len());
            GraphTopic {
                name: name.clone(),
                message_count: *count,
                participants: parts,
            }
        })
        .collect();
    topics.sort_by(|a, b| {
        b.message_count
            .cmp(&a.message_count)
            .then_with(|| a.name.cmp(&b.name))
    });
    topics.truncate(MAX_TOPICS);

    let keep_topics: BTreeSet<String> = topics.iter().map(|t| t.name.clone()).collect();
    let mut at_agg: BTreeMap<(String, String), usize> = BTreeMap::new();
    for ((from, to), count) in &agent_topic {
        if !keep_topics.contains(to) {
            continue;
        }
        let f = map_node(from);
        *at_agg.entry((f, to.clone())).or_default() += count;
    }
    let mut final_at: Vec<GraphEdge> = at_agg
        .iter()
        .filter(|((f, t), c)| !f.trim().is_empty() && !t.trim().is_empty() && **c > 0)
        .map(|((from, to), count)| GraphEdge {
            from: from.clone(),
            to: to.clone(),
            count: *count,
        })
        .collect();
    final_at.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.from.cmp(&b.from))
            .then_with(|| a.to.cmp(&b.to))
    });

    GraphSnapshot {
        messages: messages.len(),
        nodes: final_nodes,
        edges: final_edges,
        topics,
        agent_topic_edges: final_at,
    }
}

// ---------------------------------------------------------------------------
// Canvas helpers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct BoxPos {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

fn layout_boxes(
    nodes: &[GraphNode],
    width: usize,
    height: usize,
    zoom: i32,
    pan_x: i32,
    pan_y: i32,
) -> Vec<BoxPos> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let min_dim = width.min(height) as f64;
    let mut base_r = min_dim * 0.32 + (zoom as f64) * 2.0;
    if base_r < 4.0 {
        base_r = 4.0;
    }

    let cx = (width as f64) / 2.0 + (pan_x as f64);
    let cy = (height as f64) / 2.0 + (pan_y as f64);

    // Find center node (highest total).
    let center_idx = nodes
        .iter()
        .enumerate()
        .max_by_key(|(_, n)| n.total)
        .map_or(0, |(i, _)| i);

    // Build index order: center first, then others.
    let mut order: Vec<usize> = Vec::with_capacity(nodes.len());
    order.push(center_idx);
    for i in 0..nodes.len() {
        if i != center_idx {
            order.push(i);
        }
    }

    let outer_count = order.len().saturating_sub(1).max(1);

    let mut boxes = vec![
        BoxPos {
            x: 0,
            y: 0,
            w: 0,
            h: 0
        };
        nodes.len()
    ];

    for (pos, &idx) in order.iter().enumerate() {
        let name = &nodes[idx].name;
        let count_label = format!("({})", nodes[idx].sent);
        let inner_w = 8_usize.max(name.chars().count().max(count_label.len()) + 2);
        let bw = (inner_w + 2) as i32;
        let bh = 4_i32;

        let (x, y) = if pos == 0 {
            // Center node.
            (cx as i32 - bw / 2, cy as i32 - bh / 2)
        } else {
            let angle = 2.0 * std::f64::consts::PI * ((pos - 1) as f64) / (outer_count as f64);
            (
                (cx + base_r * angle.cos()) as i32 - bw / 2,
                (cy + base_r * angle.sin()) as i32 - bh / 2,
            )
        };

        let x = x.max(0).min((width as i32 - bw).max(0));
        let y = y.max(0).min((height as i32 - bh).max(0));
        boxes[idx] = BoxPos { x, y, w: bw, h: bh };
    }

    boxes
}

struct TopicBoxPos {
    x: i32,
    y: i32,
    w: i32,
}

fn layout_topic_boxes(
    topics: &[GraphTopic],
    width: usize,
    height: usize,
    zoom: i32,
    pan_x: i32,
    pan_y: i32,
) -> Vec<TopicBoxPos> {
    if topics.is_empty() {
        return Vec::new();
    }

    let min_dim = width.min(height) as f64;
    let mut base_r = min_dim * 0.18 + (zoom as f64) * 1.5;
    if base_r < 3.0 {
        base_r = 3.0;
    }

    let cx = (width as f64) / 2.0 + (pan_x as f64);
    let cy = (height as f64) / 2.0 + (pan_y as f64);
    let count = topics.len().max(1) as f64;

    topics
        .iter()
        .enumerate()
        .map(|(i, topic)| {
            let label = format!("({} {})", truncate(&topic.name, 12), topic.message_count);
            let bw = (label.chars().count()).max(6) as i32;
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / count;
            let x = ((cx + base_r * angle.cos()) as i32 - bw / 2)
                .max(0)
                .min((width as i32 - bw).max(0));
            let y = ((cy + base_r * angle.sin()) as i32)
                .max(0)
                .min((height as i32 - 1).max(0));
            TopicBoxPos { x, y, w: bw }
        })
        .collect()
}

fn center_pad(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let s = s.trim();
    let slen = s.chars().count();
    if slen >= width {
        return truncate(s, width);
    }
    let pad = width - slen;
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = input.chars().collect();
    if chars.len() <= max_chars {
        return input.to_owned();
    }
    if max_chars == 1 {
        return "\u{2026}".to_owned();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('\u{2026}');
    out
}

/// Edge line runes based on weight.
fn edge_runes(count: usize) -> (char, char) {
    match count {
        c if c >= 20 => ('\u{2501}', '\u{2503}'), // ━ ┃
        c if c >= 6 => ('\u{2550}', '\u{2551}'),  // ═ ║
        _ => ('\u{2500}', '\u{2502}'),            // ─ │
    }
}

fn box_runes(
    selected: bool,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    char,
    char,
) {
    if selected {
        (
            "\u{2554}", "\u{2557}", "\u{255A}", "\u{255D}", '\u{2550}', '\u{2551}',
        )
        // ╔ ╗ ╚ ╝ ═ ║
    } else {
        (
            "\u{250C}", "\u{2510}", "\u{2514}", "\u{2518}", '\u{2500}', '\u{2502}',
        )
        // ┌ ┐ └ ┘ ─ │
    }
}

// Grid-based canvas for ASCII rendering.
struct Canvas {
    grid: Vec<Vec<char>>,
    width: usize,
    height: usize,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            grid: vec![vec![' '; width]; height],
            width,
            height,
        }
    }

    #[cfg(test)]
    fn set(&mut self, x: i32, y: i32, ch: char) {
        if y >= 0 && (y as usize) < self.height && x >= 0 && (x as usize) < self.width {
            self.grid[y as usize][x as usize] = ch;
        }
    }

    fn set_if_empty(&mut self, x: i32, y: i32, ch: char) {
        if y >= 0
            && (y as usize) < self.height
            && x >= 0
            && (x as usize) < self.width
            && self.grid[y as usize][x as usize] == ' '
        {
            self.grid[y as usize][x as usize] = ch;
        }
    }

    fn draw_text(&mut self, x: i32, y: i32, s: &str) {
        if y < 0 || (y as usize) >= self.height {
            return;
        }
        let mut col = x;
        for ch in s.chars() {
            if col >= 0 && (col as usize) < self.width {
                self.grid[y as usize][col as usize] = ch;
            }
            col += 1;
        }
    }

    fn draw_h(&mut self, y: i32, x1: i32, x2: i32, ch: char) {
        if y < 0 || (y as usize) >= self.height {
            return;
        }
        let (lo, hi) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
        for x in lo..=hi {
            self.set_if_empty(x, y, ch);
        }
    }

    fn draw_v(&mut self, x: i32, y1: i32, y2: i32, ch: char) {
        let (lo, hi) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
        for y in lo..=hi {
            self.set_if_empty(x, y, ch);
        }
    }

    fn to_lines(&self) -> Vec<String> {
        self.grid.iter().map(|row| row.iter().collect()).collect()
    }
}

fn draw_boxes(canvas: &mut Canvas, boxes: &[BoxPos], nodes: &[GraphNode], selected: usize) {
    for (i, b) in boxes.iter().enumerate() {
        if b.w <= 0 || b.h <= 0 {
            continue;
        }
        let sel = i == selected;
        let (tl, tr, bl, br, h, v) = box_runes(sel);

        // Top border.
        let bar: String = std::iter::repeat(h)
            .take((b.w - 2).max(0) as usize)
            .collect();
        canvas.draw_text(b.x, b.y, &format!("{tl}{bar}{tr}"));

        // Name row.
        let name = center_pad(&nodes[i].name, (b.w - 2) as usize);
        canvas.draw_text(b.x, b.y + 1, &format!("{v}{name}{v}"));

        // Count row.
        let count = center_pad(&format!("({})", nodes[i].sent), (b.w - 2) as usize);
        canvas.draw_text(b.x, b.y + 2, &format!("{v}{count}{v}"));

        // Bottom border.
        canvas.draw_text(b.x, b.y + 3, &format!("{bl}{bar}{br}"));
    }
}

fn draw_edge(canvas: &mut Canvas, from: &BoxPos, to: &BoxPos, count: usize) {
    if canvas.width == 0 || canvas.height == 0 {
        return;
    }
    let w = canvas.width as i32;

    let from_cy = from.y + from.h / 2;
    let to_cy = to.y + to.h / 2;

    let (start_x, end_x) = if to.x + to.w / 2 >= from.x + from.w / 2 {
        (from.x + from.w, to.x - 1)
    } else {
        (from.x - 1, to.x + to.w)
    };
    let start_x = start_x.max(0).min(w - 1);
    let end_x = end_x.max(0).min(w - 1);
    let mid_x = ((start_x + end_x) / 2).max(0).min(w - 1);

    let (h_ch, v_ch) = edge_runes(count);

    canvas.draw_h(from_cy, start_x, mid_x, h_ch);
    canvas.draw_v(mid_x, from_cy, to_cy, v_ch);
    canvas.draw_h(to_cy, mid_x, end_x, h_ch);

    // Arrow at destination.
    let arrow = if end_x < to.x {
        '\u{2192}' // →
    } else if end_x > to.x + to.w {
        '\u{2190}' // ←
    } else if to_cy < to.y {
        '\u{2193}' // ↓
    } else {
        '\u{2191}' // ↑
    };
    canvas.set_if_empty(end_x, to_cy, arrow);

    // Edge label.
    let label = format!("{count}");
    let label_x = (mid_x - (label.len() as i32) / 2)
        .max(0)
        .min((w - 1).max(0));
    for (i, ch) in label.chars().enumerate() {
        canvas.set_if_empty(label_x + i as i32, from_cy, ch);
    }
}

fn draw_edges(canvas: &mut Canvas, boxes: &[BoxPos], snap: &GraphSnapshot) {
    let idx: std::collections::HashMap<&str, usize> = snap
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();

    let mut sorted: Vec<(usize, usize, usize)> = snap
        .edges
        .iter()
        .filter_map(|e| {
            let fi = idx.get(e.from.as_str())?;
            let ti = idx.get(e.to.as_str())?;
            Some((*fi, *ti, e.count))
        })
        .collect();
    sorted.sort_by(|a, b| b.2.cmp(&a.2));

    for (fi, ti, count) in sorted {
        draw_edge(canvas, &boxes[fi], &boxes[ti], count);
    }
}

// ---------------------------------------------------------------------------
// Input handler
// ---------------------------------------------------------------------------

/// Process input events for the graph view.
///
/// Returns `true` if the event was consumed.
pub fn apply_graph_input(view: &mut GraphViewModel, event: InputEvent) -> bool {
    let action = translate_input(&event);
    let (key, modifiers) = match event {
        InputEvent::Key(ke) => (ke.key, ke.modifiers),
        _ => return false,
    };

    match action {
        UiAction::MoveUp => {
            view.pan_y -= 1;
            return true;
        }
        UiAction::MoveDown => {
            view.pan_y += 1;
            return true;
        }
        _ => {}
    }

    match key {
        Key::Left => {
            view.pan_x -= 1;
            true
        }
        Key::Right => {
            view.pan_x += 1;
            true
        }
        Key::Tab => {
            if !view.snap.nodes.is_empty() {
                if modifiers.shift {
                    if view.selected == 0 {
                        view.selected = view.snap.nodes.len() - 1;
                    } else {
                        view.selected -= 1;
                    }
                } else {
                    view.selected = (view.selected + 1) % view.snap.nodes.len();
                }
            }
            true
        }
        Key::Enter => {
            view.show_details = !view.show_details;
            true
        }
        Key::Char('[') => {
            if view.window_idx > 0 {
                view.window_idx -= 1;
            }
            true
        }
        Key::Char(']') => {
            if view.window_idx + 1 < view.window_labels.len() {
                view.window_idx += 1;
            }
            true
        }
        Key::Char('t') => {
            view.topic_overlay = !view.topic_overlay;
            true
        }
        Key::Char('c') => {
            view.clusters = !view.clusters;
            true
        }
        Key::Char('+') => {
            if view.zoom < ZOOM_MAX {
                view.zoom += 1;
            }
            true
        }
        Key::Char('-') => {
            if view.zoom > ZOOM_MIN {
                view.zoom -= 1;
            }
            true
        }
        Key::Char('r') => {
            // Signal refresh (caller handles actual data loading).
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Render the graph view frame.
pub fn render_graph_frame(
    view: &GraphViewModel,
    width: usize,
    height: usize,
    theme: ThemeSpec,
) -> RenderFrame {
    let mut frame = RenderFrame::new(FrameSize { width, height }, theme);
    if width == 0 || height == 0 {
        return frame;
    }

    // Header line.
    let header = header_line(view);
    frame.draw_text(0, 0, &truncate(&header, width), TextRole::Accent);

    if view.loading {
        if height > 1 {
            frame.draw_text(0, 1, &truncate("loading\u{2026}", width), TextRole::Muted);
        }
        return frame;
    }

    if let Some(ref err) = view.error {
        if height > 1 {
            let msg = format!("error: {err}");
            frame.draw_text(0, 1, &truncate(&msg, width), TextRole::Danger);
        }
        return frame;
    }

    // Hint line.
    let hint = "[/]:range  t:overlay  c:clusters  Tab:next  +/-:zoom  arrows:pan  Enter:details  r:refresh";
    if height > 1 {
        frame.draw_text(0, 1, &truncate(hint, width), TextRole::Muted);
    }

    let header_h = 2_usize;
    let details_h = if view.show_details { 6 } else { 0 };
    let canvas_h = height
        .saturating_sub(header_h)
        .saturating_sub(details_h)
        .max(4.min(height.saturating_sub(header_h)));

    // Render canvas.
    let lines = if view.topic_overlay {
        render_overlay_canvas(view, width, canvas_h)
    } else {
        render_agent_canvas(view, width, canvas_h)
    };
    for (i, line) in lines.iter().enumerate() {
        let y = header_h + i;
        if y < height {
            frame.draw_text(0, y, &truncate(line, width), TextRole::Primary);
        }
    }

    // Details panel.
    if view.show_details {
        let details = render_details(view, width);
        for (i, line) in details.iter().enumerate() {
            let y = header_h + canvas_h + i;
            if y < height {
                let role = if i == 0 {
                    TextRole::Accent
                } else {
                    TextRole::Primary
                };
                frame.draw_text(0, y, &truncate(line, width), role);
            }
        }
    }

    frame
}

fn header_line(view: &GraphViewModel) -> String {
    let label = view
        .window_labels
        .get(view.window_idx)
        .map_or("?", |s| s.as_str());
    let nodes = view.snap.nodes.len();
    let edges = view.snap.edges.len();
    let mode = if view.topic_overlay {
        "topics"
    } else {
        "agents"
    };
    let cluster = if view.clusters { "  clusters:on" } else { "" };
    format!(
        "Graph  last {}  mode:{}{}  {} messages  {} nodes  {} edges",
        label, mode, cluster, view.snap.messages, nodes, edges
    )
}

fn render_agent_canvas(view: &GraphViewModel, width: usize, height: usize) -> Vec<String> {
    if height == 0 {
        return Vec::new();
    }
    let mut canvas = Canvas::new(width, height);
    let boxes = layout_boxes(
        &view.snap.nodes,
        width,
        height,
        view.zoom,
        view.pan_x,
        view.pan_y,
    );
    draw_edges(&mut canvas, &boxes, &view.snap);
    draw_boxes(&mut canvas, &boxes, &view.snap.nodes, view.selected);
    canvas.to_lines()
}

fn render_overlay_canvas(view: &GraphViewModel, width: usize, height: usize) -> Vec<String> {
    if height == 0 {
        return Vec::new();
    }
    let mut canvas = Canvas::new(width, height);
    let agent_boxes = layout_boxes(
        &view.snap.nodes,
        width,
        height,
        view.zoom,
        view.pan_x,
        view.pan_y,
    );
    let topic_boxes = layout_topic_boxes(
        &view.snap.topics,
        width,
        height,
        view.zoom,
        view.pan_x,
        view.pan_y,
    );

    // Agent→topic edges.
    let agent_idx: std::collections::HashMap<&str, usize> = view
        .snap
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    let topic_idx: std::collections::HashMap<&str, usize> = view
        .snap
        .topics
        .iter()
        .enumerate()
        .map(|(i, t)| (t.name.as_str(), i))
        .collect();

    let mut at_edges: Vec<&GraphEdge> = view.snap.agent_topic_edges.iter().collect();
    at_edges.sort_by(|a, b| b.count.cmp(&a.count));
    for e in at_edges {
        if let (Some(&ai), Some(&ti)) =
            (agent_idx.get(e.from.as_str()), topic_idx.get(e.to.as_str()))
        {
            let tb = &topic_boxes[ti];
            let fake_box = BoxPos {
                x: tb.x,
                y: tb.y,
                w: tb.w,
                h: 1,
            };
            draw_edge(&mut canvas, &agent_boxes[ai], &fake_box, e.count);
        }
    }

    // Topic labels.
    for (i, topic) in view.snap.topics.iter().enumerate() {
        if let Some(tb) = topic_boxes.get(i) {
            let label = format!("({} {})", truncate(&topic.name, 12), topic.message_count);
            canvas.draw_text(tb.x, tb.y, &truncate(&label, tb.w as usize));
        }
    }

    // Agent boxes on top.
    draw_boxes(&mut canvas, &agent_boxes, &view.snap.nodes, view.selected);
    canvas.to_lines()
}

fn render_details(view: &GraphViewModel, width: usize) -> Vec<String> {
    if view.snap.nodes.is_empty() {
        return vec![truncate("no data", width)];
    }
    let sel = view.selected.min(view.snap.nodes.len().saturating_sub(1));
    let node = &view.snap.nodes[sel];

    let mut lines: Vec<String> = Vec::with_capacity(6);
    lines.push(format!(
        "Selected: {}  sent:{} recv:{}",
        node.name, node.sent, node.recv
    ));

    // Top 5 outgoing edges.
    let mut out: Vec<(&str, usize)> = view
        .snap
        .edges
        .iter()
        .filter(|e| e.from == node.name)
        .map(|e| (e.to.as_str(), e.count))
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.truncate(5);

    if out.is_empty() {
        lines.push("Top: (no edges)".to_owned());
    } else {
        let parts: Vec<String> = out.iter().map(|(to, c)| format!("{to}:{c}")).collect();
        lines.push(format!("Top out: {}", parts.join("  ")));
    }

    // Pad to 6 lines.
    while lines.len() < 6 {
        lines.push(String::new());
    }

    for line in &mut lines {
        *line = truncate(line, width);
    }

    lines
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use forge_ftui_adapter::input::{InputEvent, Key, KeyEvent, Modifiers};
    use forge_ftui_adapter::snapshot::assert_render_frame_snapshot;
    use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

    fn theme() -> ThemeSpec {
        ThemeSpec::for_kind(ThemeKind::HighContrast)
    }

    fn key(k: Key) -> InputEvent {
        InputEvent::Key(KeyEvent::plain(k))
    }

    fn shift_tab() -> InputEvent {
        InputEvent::Key(KeyEvent {
            key: Key::Tab,
            modifiers: Modifiers {
                shift: true,
                ctrl: false,
                alt: false,
            },
        })
    }

    fn msg(id: &str, from: &str, to: &str) -> GraphMessage {
        GraphMessage {
            id: id.to_owned(),
            from: from.to_owned(),
            to: to.to_owned(),
        }
    }

    // --- ViewModel defaults ---

    #[test]
    fn new_viewmodel_defaults() {
        let v = GraphViewModel::default();
        assert_eq!(v.window_idx, 1);
        assert_eq!(v.zoom, 0);
        assert_eq!(v.pan_x, 0);
        assert_eq!(v.pan_y, 0);
        assert_eq!(v.selected, 0);
        assert!(v.show_details);
        assert!(!v.topic_overlay);
        assert!(!v.clusters);
        assert!(!v.loading);
        assert!(v.error.is_none());
        assert_eq!(v.snap.nodes.len(), 0);
    }

    // --- Graph computation ---

    #[test]
    fn build_snapshot_topic_broadcast_edges() {
        let messages = vec![msg("1", "alice", "task"), msg("2", "bob", "task")];
        let snap = build_graph_snapshot(&messages, 12);
        assert_eq!(snap.messages, 2);
        assert_eq!(snap.nodes.len(), 2);
        assert_eq!(snap.edges.len(), 2);

        assert_edge(&snap, "alice", "bob", 1);
        assert_edge(&snap, "bob", "alice", 1);
    }

    #[test]
    fn build_snapshot_topic_counts_scale() {
        let messages = vec![
            msg("1", "alice", "task"),
            msg("2", "alice", "task"),
            msg("3", "alice", "task"),
            msg("4", "bob", "task"),
        ];
        let snap = build_graph_snapshot(&messages, 12);
        assert_edge(&snap, "alice", "bob", 3);
        assert_edge(&snap, "bob", "alice", 1);
    }

    #[test]
    fn build_snapshot_dms_directed() {
        let messages = vec![
            msg("1", "alice", "@bob"),
            msg("2", "bob", "@alice"),
            msg("3", "bob", "@alice"),
        ];
        let snap = build_graph_snapshot(&messages, 12);
        assert_edge(&snap, "alice", "bob", 1);
        assert_edge(&snap, "bob", "alice", 2);
    }

    #[test]
    fn build_snapshot_collapses_to_others() {
        let mut messages = Vec::new();
        for i in 0..20 {
            let from = format!("a{}", (b'a' + i) as char);
            messages.push(msg(&format!("{i}"), &from, "@z"));
        }
        let snap = build_graph_snapshot(&messages, 4);
        assert!(snap.nodes.len() <= 4, "nodes={}", snap.nodes.len());
        assert!(
            snap.nodes.iter().any(|n| n.name == "others"),
            "expected others node"
        );
    }

    #[test]
    fn build_snapshot_empty_messages() {
        let snap = build_graph_snapshot(&[], 12);
        assert_eq!(snap.messages, 0);
        assert!(snap.nodes.is_empty());
        assert!(snap.edges.is_empty());
    }

    #[test]
    fn build_snapshot_topics_overlay_data() {
        let messages = vec![
            msg("1", "alice", "task"),
            msg("2", "bob", "task"),
            msg("3", "charlie", "bugs"),
        ];
        let snap = build_graph_snapshot(&messages, 12);
        assert!(!snap.topics.is_empty());
        assert!(snap.topics[0].name == "task" || snap.topics[0].name == "bugs");
    }

    #[test]
    fn build_snapshot_self_dm_ignored() {
        let messages = vec![msg("1", "alice", "@alice")];
        let snap = build_graph_snapshot(&messages, 12);
        assert!(snap.nodes.is_empty());
        assert!(snap.edges.is_empty());
    }

    #[test]
    fn build_snapshot_nodes_sorted_by_total() {
        let messages = vec![
            msg("1", "alice", "@bob"),
            msg("2", "alice", "@bob"),
            msg("3", "alice", "@bob"),
            msg("4", "charlie", "@bob"),
        ];
        let snap = build_graph_snapshot(&messages, 12);
        // bob has highest total (recv 4), then alice (sent 3), then charlie (sent 1)
        assert_eq!(snap.nodes[0].name, "bob");
    }

    fn assert_edge(snap: &GraphSnapshot, from: &str, to: &str, want: usize) {
        for e in &snap.edges {
            if e.from == from && e.to == to {
                assert_eq!(e.count, want, "edge {from}->{to}={}, want {want}", e.count);
                return;
            }
        }
        panic!("missing edge {from}->{to}");
    }

    // --- set_messages ---

    #[test]
    fn set_messages_builds_snapshot() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "task"), msg("2", "bob", "task")]);
        assert_eq!(v.snap.messages, 2);
        assert_eq!(v.snap.nodes.len(), 2);
    }

    // --- Input handling ---

    #[test]
    fn input_bracket_keys_cycle_window() {
        let mut v = GraphViewModel::default();
        assert_eq!(v.window_idx, 1);
        apply_graph_input(&mut v, key(Key::Char(']')));
        assert_eq!(v.window_idx, 2);
        apply_graph_input(&mut v, key(Key::Char('[')));
        assert_eq!(v.window_idx, 1);
    }

    #[test]
    fn input_bracket_clamps_at_bounds() {
        let mut v = GraphViewModel {
            window_idx: 0,
            ..Default::default()
        };
        apply_graph_input(&mut v, key(Key::Char('[')));
        assert_eq!(v.window_idx, 0);

        v.window_idx = v.window_labels.len() - 1;
        apply_graph_input(&mut v, key(Key::Char(']')));
        assert_eq!(v.window_idx, v.window_labels.len() - 1);
    }

    #[test]
    fn input_tab_cycles_selected() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob"), msg("2", "charlie", "@bob")]);
        assert_eq!(v.selected, 0);
        apply_graph_input(&mut v, key(Key::Tab));
        assert_eq!(v.selected, 1);
        apply_graph_input(&mut v, key(Key::Tab));
        assert_eq!(v.selected, 2);
        apply_graph_input(&mut v, key(Key::Tab));
        assert_eq!(v.selected, 0);
    }

    #[test]
    fn input_shift_tab_cycles_selected_backwards() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob"), msg("2", "charlie", "@bob")]);
        assert_eq!(v.selected, 0);
        apply_graph_input(&mut v, shift_tab());
        assert_eq!(v.selected, 2);
        apply_graph_input(&mut v, shift_tab());
        assert_eq!(v.selected, 1);
    }

    #[test]
    fn input_enter_toggles_details() {
        let mut v = GraphViewModel::default();
        assert!(v.show_details);
        apply_graph_input(&mut v, key(Key::Enter));
        assert!(!v.show_details);
        apply_graph_input(&mut v, key(Key::Enter));
        assert!(v.show_details);
    }

    #[test]
    fn input_t_toggles_overlay() {
        let mut v = GraphViewModel::default();
        assert!(!v.topic_overlay);
        apply_graph_input(&mut v, key(Key::Char('t')));
        assert!(v.topic_overlay);
        apply_graph_input(&mut v, key(Key::Char('t')));
        assert!(!v.topic_overlay);
    }

    #[test]
    fn input_c_toggles_clusters() {
        let mut v = GraphViewModel::default();
        assert!(!v.clusters);
        apply_graph_input(&mut v, key(Key::Char('c')));
        assert!(v.clusters);
    }

    #[test]
    fn input_plus_minus_zoom() {
        let mut v = GraphViewModel::default();
        apply_graph_input(&mut v, key(Key::Char('+')));
        assert_eq!(v.zoom, 1);
        apply_graph_input(&mut v, key(Key::Char('-')));
        assert_eq!(v.zoom, 0);
        apply_graph_input(&mut v, key(Key::Char('-')));
        assert_eq!(v.zoom, -1);
    }

    #[test]
    fn input_zoom_clamps_at_bounds() {
        let mut v = GraphViewModel {
            zoom: ZOOM_MAX,
            ..Default::default()
        };
        apply_graph_input(&mut v, key(Key::Char('+')));
        assert_eq!(v.zoom, ZOOM_MAX);

        v.zoom = ZOOM_MIN;
        apply_graph_input(&mut v, key(Key::Char('-')));
        assert_eq!(v.zoom, ZOOM_MIN);
    }

    #[test]
    fn input_arrows_pan() {
        let mut v = GraphViewModel::default();
        apply_graph_input(&mut v, key(Key::Up));
        assert_eq!(v.pan_y, -1);
        apply_graph_input(&mut v, key(Key::Down));
        assert_eq!(v.pan_y, 0);
        apply_graph_input(&mut v, key(Key::Left));
        assert_eq!(v.pan_x, -1);
        apply_graph_input(&mut v, key(Key::Right));
        assert_eq!(v.pan_x, 0);
    }

    #[test]
    fn input_r_consumed() {
        let mut v = GraphViewModel::default();
        let consumed = apply_graph_input(&mut v, key(Key::Char('r')));
        assert!(consumed);
    }

    #[test]
    fn input_unknown_not_consumed() {
        let mut v = GraphViewModel::default();
        let consumed = apply_graph_input(&mut v, key(Key::Char('z')));
        assert!(!consumed);
    }

    // --- Rendering ---

    #[test]
    fn render_empty_graph() {
        let v = GraphViewModel::default();
        let frame = render_graph_frame(&v, 60, 20, theme());
        let row0 = frame.row_text(0);
        assert!(row0.contains("Graph"), "header: {row0}");
        assert!(row0.contains("0 nodes"), "header: {row0}");
    }

    #[test]
    fn render_loading_state() {
        let v = GraphViewModel {
            loading: true,
            ..Default::default()
        };
        let frame = render_graph_frame(&v, 60, 20, theme());
        let row1 = frame.row_text(1);
        assert!(row1.contains("loading"), "row1: {row1}");
    }

    #[test]
    fn render_error_state() {
        let v = GraphViewModel {
            error: Some("connection failed".to_owned()),
            ..Default::default()
        };
        let frame = render_graph_frame(&v, 60, 20, theme());
        let row1 = frame.row_text(1);
        assert!(row1.contains("error"), "row1: {row1}");
        assert!(row1.contains("connection failed"), "row1: {row1}");
    }

    #[test]
    fn render_with_data_shows_boxes() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob"), msg("2", "bob", "@alice")]);
        let frame = render_graph_frame(&v, 80, 30, theme());
        let row0 = frame.row_text(0);
        assert!(row0.contains("2 messages"), "header: {row0}");
        assert!(row0.contains("2 nodes"), "header: {row0}");

        // Check some canvas row contains agent name.
        let mut found = false;
        for y in 2..30 {
            let row = frame.row_text(y);
            if row.contains("alice") || row.contains("bob") {
                found = true;
                break;
            }
        }
        assert!(found, "expected agent names in canvas");
    }

    #[test]
    fn render_details_shows_selected() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob"), msg("2", "bob", "@alice")]);
        v.show_details = true;
        let frame = render_graph_frame(&v, 80, 30, theme());
        // Look for "Selected:" in the details area.
        let mut found = false;
        for y in 0..30 {
            let row = frame.row_text(y);
            if row.contains("Selected:") {
                found = true;
                break;
            }
        }
        assert!(found, "expected details panel with Selected:");
    }

    #[test]
    fn render_no_details_when_hidden() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob")]);
        v.show_details = false;
        let frame = render_graph_frame(&v, 80, 30, theme());
        let mut found = false;
        for y in 0..30 {
            let row = frame.row_text(y);
            if row.contains("Selected:") {
                found = true;
                break;
            }
        }
        assert!(!found, "details should be hidden");
    }

    #[test]
    fn render_topic_overlay_mode() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "task"), msg("2", "bob", "task")]);
        v.topic_overlay = true;
        let frame = render_graph_frame(&v, 80, 30, theme());
        let row0 = frame.row_text(0);
        assert!(row0.contains("mode:topics"), "header: {row0}");
    }

    #[test]
    fn render_clusters_on_in_header() {
        let v = GraphViewModel {
            clusters: true,
            ..Default::default()
        };
        let frame = render_graph_frame(&v, 80, 20, theme());
        let row0 = frame.row_text(0);
        assert!(row0.contains("clusters:on"), "header: {row0}");
    }

    #[test]
    fn render_zero_size_no_panic() {
        let v = GraphViewModel::default();
        let _ = render_graph_frame(&v, 0, 0, theme());
        let _ = render_graph_frame(&v, 80, 0, theme());
        let _ = render_graph_frame(&v, 0, 20, theme());
    }

    #[test]
    fn graph_snapshot_render() {
        let mut v = GraphViewModel::default();
        v.set_messages(&[msg("1", "alice", "@bob"), msg("2", "bob", "@alice")]);
        let frame = render_graph_frame(&v, 80, 24, theme());
        let text: String = (0..24)
            .map(|r| {
                let t = frame.row_text(r);
                format!("{:<80}", t)
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_render_frame_snapshot("graph_basic_render", &frame, &text);
    }

    // --- Canvas helpers ---

    #[test]
    fn edge_runes_vary_by_count() {
        let (h1, _) = edge_runes(1);
        let (h6, _) = edge_runes(6);
        let (h20, _) = edge_runes(20);
        assert_ne!(h1, h6);
        assert_ne!(h6, h20);
    }

    #[test]
    fn box_runes_differ_by_selection() {
        let (tl1, _, _, _, _, _) = box_runes(false);
        let (tl2, _, _, _, _, _) = box_runes(true);
        assert_ne!(tl1, tl2);
    }

    #[test]
    fn center_pad_works() {
        assert_eq!(center_pad("hi", 10), "    hi    ");
        assert_eq!(center_pad("hi", 2), "hi");
        assert_eq!(center_pad("hello world", 5), "hell\u{2026}");
    }

    #[test]
    fn truncate_works() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
        assert_eq!(truncate("", 5), "");
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(truncate("hello", 1), "\u{2026}");
    }

    #[test]
    fn layout_boxes_empty() {
        let boxes = layout_boxes(&[], 80, 30, 0, 0, 0);
        assert!(boxes.is_empty());
    }

    #[test]
    fn layout_boxes_single_node() {
        let nodes = vec![GraphNode {
            name: "alice".into(),
            sent: 1,
            recv: 0,
            total: 1,
        }];
        let boxes = layout_boxes(&nodes, 80, 30, 0, 0, 0);
        assert_eq!(boxes.len(), 1);
        assert!(boxes[0].w > 0);
        assert!(boxes[0].h > 0);
    }

    #[test]
    fn canvas_draw_and_to_lines() {
        let mut c = Canvas::new(5, 3);
        c.set(0, 0, 'A');
        c.set(4, 2, 'Z');
        let lines = c.to_lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "A    ");
        assert_eq!(lines[2], "    Z");
    }

    #[test]
    fn canvas_set_if_empty_skips_occupied() {
        let mut c = Canvas::new(3, 1);
        c.set(1, 0, 'X');
        c.set_if_empty(1, 0, 'Y');
        assert_eq!(c.grid[0][1], 'X');
    }
}
