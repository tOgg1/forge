//! Export the current rendered view as text, HTML, and SVG artifacts.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use forge_ftui_adapter::render::{CellStyle, RenderFrame, TermColor};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewExportMeta {
    pub view_label: String,
    pub mode_label: String,
    pub generated_epoch_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewExportPayload {
    pub text: String,
    pub html: String,
    pub svg: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewExportFiles {
    pub text_path: PathBuf,
    pub html_path: PathBuf,
    pub svg_path: PathBuf,
}

#[must_use]
pub fn epoch_millis_now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

#[must_use]
pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_dash = false;
            continue;
        }
        if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    if out.is_empty() {
        "view".to_owned()
    } else {
        out.trim_matches('-').to_owned()
    }
}

#[must_use]
pub fn default_basename(view_label: &str, epoch_ms: u128) -> String {
    format!("forge-view-{}-{epoch_ms}", slugify(view_label))
}

#[must_use]
pub fn build_payload(frame: &RenderFrame, meta: &ViewExportMeta) -> ViewExportPayload {
    ViewExportPayload {
        text: render_text(frame, meta),
        html: render_html(frame, meta),
        svg: render_svg(frame, meta),
    }
}

pub fn write_payload_files(
    payload: &ViewExportPayload,
    output_dir: &Path,
    basename: &str,
) -> Result<ViewExportFiles, String> {
    fs::create_dir_all(output_dir).map_err(|err| {
        format!(
            "create export directory {}: {err}",
            output_dir.to_string_lossy()
        )
    })?;

    let text_path = output_dir.join(format!("{basename}.txt"));
    let html_path = output_dir.join(format!("{basename}.html"));
    let svg_path = output_dir.join(format!("{basename}.svg"));

    fs::write(&text_path, &payload.text)
        .map_err(|err| format!("write {}: {err}", text_path.to_string_lossy()))?;
    fs::write(&html_path, &payload.html)
        .map_err(|err| format!("write {}: {err}", html_path.to_string_lossy()))?;
    fs::write(&svg_path, &payload.svg)
        .map_err(|err| format!("write {}: {err}", svg_path.to_string_lossy()))?;

    Ok(ViewExportFiles {
        text_path,
        html_path,
        svg_path,
    })
}

pub fn export_frame_to_files(
    frame: &RenderFrame,
    output_dir: &Path,
    basename: &str,
    meta: &ViewExportMeta,
) -> Result<ViewExportFiles, String> {
    let payload = build_payload(frame, meta);
    write_payload_files(&payload, output_dir, basename)
}

fn render_text(frame: &RenderFrame, meta: &ViewExportMeta) -> String {
    let mut out = String::new();
    let _ = writeln!(&mut out, "# forge-tui view export");
    let _ = writeln!(&mut out, "view: {}", meta.view_label);
    let _ = writeln!(&mut out, "mode: {}", meta.mode_label);
    let _ = writeln!(&mut out, "generated-epoch-ms: {}", meta.generated_epoch_ms);
    let _ = writeln!(&mut out);
    out.push_str(&frame.snapshot());
    out.push('\n');
    out
}

fn render_html(frame: &RenderFrame, meta: &ViewExportMeta) -> String {
    let size = frame.size();
    let mut out = String::new();
    let _ = writeln!(&mut out, "<!doctype html>");
    let _ = writeln!(&mut out, "<html lang=\"en\">");
    let _ = writeln!(&mut out, "<head>");
    let _ = writeln!(&mut out, "  <meta charset=\"utf-8\">");
    let _ = writeln!(
        &mut out,
        "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
    );
    let _ = writeln!(
        &mut out,
        "  <title>forge-tui export | {}</title>",
        escape_html(&meta.view_label)
    );
    let _ = writeln!(&mut out, "  <style>");
    let _ = writeln!(
        &mut out,
        "    body {{ margin: 0; padding: 16px; background: #111; }}"
    );
    let _ = writeln!(
        &mut out,
        "    pre {{ font: 14px/1.2 Menlo, Consolas, Monaco, monospace; white-space: pre; display: inline-block; }}"
    );
    let _ = writeln!(
        &mut out,
        "    .meta {{ color: #9aa0a6; font: 12px/1.4 Menlo, Consolas, Monaco, monospace; margin-bottom: 8px; }}"
    );
    let _ = writeln!(&mut out, "  </style>");
    let _ = writeln!(&mut out, "</head>");
    let _ = writeln!(&mut out, "<body>");
    let _ = writeln!(
        &mut out,
        "  <div class=\"meta\">view={} | mode={} | generated-epoch-ms={} | size={}x{}</div>",
        escape_html(&meta.view_label),
        escape_html(&meta.mode_label),
        meta.generated_epoch_ms,
        size.width,
        size.height
    );
    let _ = writeln!(&mut out, "  <pre>");

    for y in 0..size.height {
        for run in collect_row_runs(frame, y) {
            let style = css_style(run.style);
            let _ = write!(
                &mut out,
                "<span style=\"{style}\">{}</span>",
                escape_html(&run.text)
            );
        }
        if y + 1 < size.height {
            out.push('\n');
        }
    }

    let _ = writeln!(&mut out, "</pre>");
    let _ = writeln!(&mut out, "</body>");
    let _ = writeln!(&mut out, "</html>");
    out
}

fn render_svg(frame: &RenderFrame, meta: &ViewExportMeta) -> String {
    const CELL_WIDTH: usize = 9;
    const CELL_HEIGHT: usize = 16;
    const FONT_SIZE: usize = 14;
    const BASELINE_OFFSET: usize = 12;

    let size = frame.size();
    let canvas_width = size.width.saturating_mul(CELL_WIDTH);
    let canvas_height = size.height.saturating_mul(CELL_HEIGHT);

    let mut out = String::new();
    let _ = writeln!(
        &mut out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{canvas_width}\" height=\"{canvas_height}\" viewBox=\"0 0 {canvas_width} {canvas_height}\">"
    );
    let _ = writeln!(
        &mut out,
        "  <desc>forge-tui export view={} mode={} generated-epoch-ms={}</desc>",
        escape_xml(&meta.view_label),
        escape_xml(&meta.mode_label),
        meta.generated_epoch_ms
    );

    let background = frame
        .cell(0, 0)
        .map(|cell| cell.style.bg)
        .unwrap_or(TermColor::Ansi256(0));
    let _ = writeln!(
        &mut out,
        "  <rect x=\"0\" y=\"0\" width=\"{canvas_width}\" height=\"{canvas_height}\" fill=\"{}\"/>",
        color_hex(background)
    );

    for y in 0..size.height {
        let runs = collect_row_runs(frame, y);
        for run in runs {
            let x = run.start_col.saturating_mul(CELL_WIDTH);
            let row_y = y.saturating_mul(CELL_HEIGHT);
            let width = run.char_count.saturating_mul(CELL_WIDTH);
            let _ = writeln!(
                &mut out,
                "  <rect x=\"{x}\" y=\"{row_y}\" width=\"{width}\" height=\"{CELL_HEIGHT}\" fill=\"{}\"/>",
                color_hex(run.style.bg)
            );

            if run.text.chars().all(|ch| ch == ' ') {
                continue;
            }
            let text_y = row_y.saturating_add(BASELINE_OFFSET);
            let weight = if run.style.bold { "bold" } else { "normal" };
            let decoration = if run.style.underline {
                " text-decoration=\"underline\""
            } else {
                ""
            };
            let opacity = if run.style.dim {
                " opacity=\"0.7\""
            } else {
                ""
            };
            let _ = writeln!(
                &mut out,
                "  <text x=\"{x}\" y=\"{text_y}\" fill=\"{}\" font-family=\"Menlo, Consolas, Monaco, monospace\" font-size=\"{FONT_SIZE}\" font-weight=\"{weight}\" xml:space=\"preserve\"{decoration}{opacity}>{}</text>",
                color_hex(run.style.fg),
                escape_xml(&run.text)
            );
        }
    }

    let _ = writeln!(&mut out, "</svg>");
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StyleRun {
    start_col: usize,
    char_count: usize,
    text: String,
    style: CellStyle,
}

fn collect_row_runs(frame: &RenderFrame, y: usize) -> Vec<StyleRun> {
    let size = frame.size();
    if y >= size.height || size.width == 0 {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut current_style: Option<CellStyle> = None;
    let mut current_text = String::new();
    let mut current_start = 0usize;
    let mut current_count = 0usize;

    for x in 0..size.width {
        let Some(cell) = frame.cell(x, y) else {
            continue;
        };
        match current_style {
            Some(style) if style == cell.style => {
                current_text.push(cell.glyph);
                current_count = current_count.saturating_add(1);
            }
            Some(style) => {
                runs.push(StyleRun {
                    start_col: current_start,
                    char_count: current_count,
                    text: std::mem::take(&mut current_text),
                    style,
                });
                current_style = Some(cell.style);
                current_start = x;
                current_count = 1;
                current_text.push(cell.glyph);
            }
            None => {
                current_style = Some(cell.style);
                current_start = x;
                current_count = 1;
                current_text.push(cell.glyph);
            }
        }
    }

    if let Some(style) = current_style {
        runs.push(StyleRun {
            start_col: current_start,
            char_count: current_count,
            text: current_text,
            style,
        });
    }
    runs
}

fn css_style(style: CellStyle) -> String {
    let mut out = format!(
        "color:{};background-color:{};",
        color_hex(style.fg),
        color_hex(style.bg)
    );
    if style.bold {
        out.push_str("font-weight:bold;");
    }
    if style.dim {
        out.push_str("opacity:0.7;");
    }
    if style.underline {
        out.push_str("text-decoration:underline;");
    }
    out
}

fn color_hex(color: TermColor) -> String {
    let (r, g, b) = term_color_rgb(color);
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn term_color_rgb(color: TermColor) -> (u8, u8, u8) {
    match color {
        TermColor::Rgb(r, g, b) => (r, g, b),
        TermColor::Ansi256(idx) => ansi256_to_rgb(idx),
    }
}

fn ansi256_to_rgb(idx: u8) -> (u8, u8, u8) {
    match idx {
        0 => (0x00, 0x00, 0x00),
        1 => (0x80, 0x00, 0x00),
        2 => (0x00, 0x80, 0x00),
        3 => (0x80, 0x80, 0x00),
        4 => (0x00, 0x00, 0x80),
        5 => (0x80, 0x00, 0x80),
        6 => (0x00, 0x80, 0x80),
        7 => (0xc0, 0xc0, 0xc0),
        8 => (0x80, 0x80, 0x80),
        9 => (0xff, 0x00, 0x00),
        10 => (0x00, 0xff, 0x00),
        11 => (0xff, 0xff, 0x00),
        12 => (0x00, 0x00, 0xff),
        13 => (0xff, 0x00, 0xff),
        14 => (0x00, 0xff, 0xff),
        15 => (0xff, 0xff, 0xff),
        16..=231 => {
            let cube = idx - 16;
            let r = cube / 36;
            let g = (cube % 36) / 6;
            let b = cube % 6;
            let levels = [0, 95, 135, 175, 215, 255];
            (
                levels[usize::from(r)],
                levels[usize::from(g)],
                levels[usize::from(b)],
            )
        }
        232..=255 => {
            let level = 8 + (idx - 232) * 10;
            (level, level, level)
        }
    }
}

fn escape_html(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_xml(value: &str) -> String {
    escape_html(value)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::process;

    use forge_ftui_adapter::render::{FrameSize, RenderFrame, TermColor, TextRole};
    use forge_ftui_adapter::style::{ThemeKind, ThemeSpec};

    use super::{
        build_payload, default_basename, epoch_millis_now, export_frame_to_files, slugify,
        ViewExportMeta,
    };

    fn sample_frame() -> RenderFrame {
        let mut frame = RenderFrame::new(
            FrameSize {
                width: 8,
                height: 3,
            },
            ThemeSpec::for_kind(ThemeKind::Dark),
        );
        frame.draw_text(0, 0, "<A&B>", TextRole::Accent);
        frame.draw_styled_text(
            0,
            1,
            "ok",
            TermColor::Rgb(1, 2, 3),
            TermColor::Rgb(10, 20, 30),
            true,
        );
        frame.draw_text(3, 2, "x", TextRole::Warning);
        frame
    }

    fn sample_meta() -> ViewExportMeta {
        ViewExportMeta {
            view_label: "Logs".to_owned(),
            mode_label: "Main".to_owned(),
            generated_epoch_ms: 1700000000123,
        }
    }

    fn temp_export_dir(label: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!(
            "forge-tui-view-export-{label}-{}-{}",
            process::id(),
            epoch_millis_now()
        ));
        path
    }

    #[test]
    fn slugify_normalizes_label() {
        assert_eq!(slugify("Multi Logs"), "multi-logs");
        assert_eq!(slugify("  !!!  "), "view");
    }

    #[test]
    fn default_basename_is_stable_shape() {
        assert_eq!(
            default_basename("Overview", 123),
            "forge-view-overview-123".to_owned()
        );
    }

    #[test]
    fn payload_contains_text_html_svg_representations() {
        let payload = build_payload(&sample_frame(), &sample_meta());
        assert!(payload.text.contains("# forge-tui view export"));
        assert!(payload.text.contains("<A&B>"));
        assert!(payload.text.contains("generated-epoch-ms: 1700000000123"));

        assert!(payload.html.contains("<!doctype html>"));
        assert!(payload.html.contains("&lt;A&amp;B&gt;"));
        assert!(payload.html.contains("<pre>"));

        assert!(payload.svg.contains("<svg"));
        assert!(payload.svg.contains("&lt;A&amp;B&gt;"));
        assert!(payload.svg.contains("generated-epoch-ms=1700000000123"));
    }

    #[test]
    fn export_writes_txt_html_and_svg_files() {
        let dir = temp_export_dir("write");
        let frame = sample_frame();
        let basename = default_basename("Runs", 1700000000999);
        let result = export_frame_to_files(&frame, &dir, &basename, &sample_meta());
        assert!(result.is_ok());
        let files = match result {
            Ok(files) => files,
            Err(err) => panic!("export should succeed: {err}"),
        };

        assert!(files.text_path.exists());
        assert!(files.html_path.exists());
        assert!(files.svg_path.exists());

        let text = match fs::read_to_string(&files.text_path) {
            Ok(text) => text,
            Err(err) => panic!("text file should be readable: {err}"),
        };
        assert!(text.contains("view: Logs"));
        assert!(text.contains("<A&B>"));

        let _ = fs::remove_file(&files.text_path);
        let _ = fs::remove_file(&files.html_path);
        let _ = fs::remove_file(&files.svg_path);
        let _ = fs::remove_dir_all(&dir);
    }
}
