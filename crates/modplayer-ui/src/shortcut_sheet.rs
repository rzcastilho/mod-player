// SPDX-License-Identifier: MIT OR Apache-2.0

//! Keyboard shortcut reference sheet (029-keyboard-shortcut-reference,
//! contracts/shortcut-sheet.md S6–S18).
//!
//! A read-only, non-modal [`egui::Window`] rebuilt every frame from the
//! live action registry (so a binding change shows on the next frame),
//! plus a self-contained printable HTML export. Only `App` opens the
//! exported file's URL (design note 11): [`show`] merely returns it in
//! [`SheetOutcome::OpenPrintable`].

use std::path::Path;

use egui::{Context, Id, Key, Modifiers, Ui, Window, vec2};
use modplayer_core::actions::{ActionRegistry, Platform};
use modplayer_core::{Chord, tr, tr_args};

use crate::key_label::KeyLabel;
use crate::shortcut_groups::{self, FilteredRow, ShortcutGroup, ShortcutRow};

/// File name of the printable export inside the temp directory.
pub const PRINTABLE_FILE_NAME: &str = "modplayer-shortcuts.html";

/// The locale tag written to the printable file's `<html lang>`.
pub const PRINTABLE_LANG: &str = "en-US";

/// Why the printable file could not be written. Carries the error kind
/// only — never the file contents or path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportError {
    pub kind: std::io::ErrorKind,
}

/// The sheet's session state (data-model §4.3). Never persisted: a fresh
/// `App` starts with the sheet closed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutSheetState {
    pub open: bool,
    pub export_error: Option<ExportError>,
}

impl ShortcutSheetState {
    /// Open the sheet (idempotent; the window is brought to the front by
    /// [`show`]).
    pub fn open(&mut self) {
        self.open = true;
    }
}

/// What the sheet asks `App` to do this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetOutcome {
    None,
    /// The printable file was written; `App` opens this `file://` URL.
    OpenPrintable(String),
}

/// The fixed window id (at most one instance, S6).
pub fn window_id() -> Id {
    Id::new("shortcut-sheet")
}

/// Turn a filesystem path into a `file://` URL. Backslashes become `/`,
/// a Windows drive path gets `file:///`, an absolute Unix path
/// `file://`, and every byte outside RFC 3986 unreserved characters plus
/// `/` and `:` is percent-encoded.
///
/// ```
/// use std::path::Path;
/// use modplayer_ui::shortcut_sheet::file_url;
///
/// assert_eq!(
///     file_url(Path::new("/tmp/modplayer-shortcuts.html")),
///     "file:///tmp/modplayer-shortcuts.html"
/// );
/// ```
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':' => {
                encoded.push(char::from(byte));
            }
            other => encoded.push_str(&format!("%{other:02X}")),
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// Escape `& < > " '` for HTML text and attribute positions.
///
/// ```
/// use modplayer_ui::shortcut_sheet::html_escape;
/// assert_eq!(html_escape("a&b\"<c>'"), "a&amp;b&quot;&lt;c&gt;&#39;");
/// ```
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

const PRINT_CSS: &str = "body{font-family:system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;\
color:#000;background:#fff;margin:2rem;line-height:1.4}\
h1{font-size:1.6rem;margin:0 0 .25rem}\
p.generated{margin:0 0 1.5rem;color:#333}\
section{margin-bottom:1.5rem}\
h2{font-size:1.15rem;margin:0 0 .4rem;border-bottom:1px solid #000}\
table{border-collapse:collapse;width:100%;table-layout:fixed}th:first-child{width:60%}\
th,td{text-align:left;padding:.25rem .5rem;border-bottom:1px solid #999;vertical-align:top}\
th{font-weight:600}\
kbd{font-family:ui-monospace,Menlo,Consolas,monospace;border:1px solid #000;\
border-radius:3px;padding:0 .35rem;margin-right:.35rem;white-space:nowrap}\
@media print{body{margin:0}section{break-inside:avoid}}";

/// Build the self-contained printable document. Pure: it takes only the
/// grouped rows, the platform whose key labels to show, the locale tag
/// and the already-formatted generation date, so no account, device or
/// path data can reach it (S17). Every interpolated string is escaped
/// (S16).
///
/// ```
/// use modplayer_core::actions::{ActionRegistry, KeymapOverrides, Platform};
/// use modplayer_ui::shortcut_groups::snapshot;
/// use modplayer_ui::shortcut_sheet::printable_html;
///
/// let registry = ActionRegistry::new(KeymapOverrides::default());
/// let html = printable_html(&snapshot(&registry), Platform::Other, "en-US", "2026-01-01 10:00");
/// assert!(html.contains("<html lang=\"en-US\">"));
/// assert!(!html.contains("<script"));
/// ```
pub fn printable_html(
    groups: &[ShortcutGroup],
    platform: Platform,
    locale: &str,
    generated: &str,
) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html>\n");
    html.push_str(&format!(
        "<html lang=\"{}\">\n<head>\n",
        html_escape(locale)
    ));
    html.push_str("<meta charset=\"utf-8\">\n");
    let title = html_escape(&tr("shortcuts-sheet-title"));
    html.push_str(&format!("<title>{title}</title>\n"));
    html.push_str(&format!("<style>{PRINT_CSS}</style>\n</head>\n<body>\n"));
    html.push_str(&format!("<h1>{title}</h1>\n"));
    html.push_str(&format!(
        "<p class=\"generated\">{}</p>\n",
        html_escape(&tr_args(
            "shortcuts-printable-generated",
            &[("date", generated.to_string())]
        ))
    ));
    let col_action = html_escape(&tr("shortcuts-printable-col-action"));
    let col_shortcut = html_escape(&tr("shortcuts-printable-col-shortcut"));
    for group in groups {
        html.push_str("<section>\n");
        html.push_str(&format!("<h2>{}</h2>\n", html_escape(&group.heading)));
        html.push_str(&format!(
            "<table>\n<thead><tr><th>{col_action}</th><th>{col_shortcut}</th></tr></thead>\n<tbody>\n"
        ));
        for row in &group.rows {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>\n",
                html_escape(&printable_name(row)),
                printable_bindings(row, platform)
            ));
        }
        html.push_str("</tbody>\n</table>\n</section>\n");
    }
    html.push_str("</body>\n</html>\n");
    html
}

fn printable_name(row: &ShortcutRow) -> String {
    if row.enabled {
        row.label.clone()
    } else {
        format!("{} {}", row.label, tr("controls-inactive"))
    }
}

fn printable_bindings(row: &ShortcutRow, platform: Platform) -> String {
    if row.bindings.is_empty() {
        return html_escape(&tr("shortcuts-not-bound"));
    }
    row.bindings
        .iter()
        .map(|&chord| {
            let label = KeyLabel::new(chord, platform);
            let mut cell = format!("<kbd>{}</kbd>", html_escape(&label.visible));
            if row.conflicts.contains(&chord) {
                cell.push_str(&format!(
                    "\u{26a0} {}",
                    html_escape(&tr("shortcuts-conflict-marker"))
                ));
            }
            cell
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write `html` to `dir/modplayer-shortcuts.html` (overwriting) and
/// return the file's `file://` URL.
///
/// ```
/// use modplayer_ui::shortcut_sheet::write_printable;
///
/// let dir = std::env::temp_dir();
/// let url = write_printable(&dir, "<html></html>").unwrap();
/// assert!(url.starts_with("file://"));
/// ```
pub fn write_printable(dir: &Path, html: &str) -> std::io::Result<String> {
    let path = dir.join(PRINTABLE_FILE_NAME);
    std::fs::write(&path, html)?;
    Ok(file_url(&path))
}

/// Build and write the printable version into `dir`, recording a failure
/// in `state.export_error` (S14, S18). A failure logs only the error
/// kind and opens nothing.
pub fn export_printable(
    state: &mut ShortcutSheetState,
    groups: &[ShortcutGroup],
    platform: Platform,
    dir: &Path,
) -> SheetOutcome {
    let generated = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let html = printable_html(groups, platform, PRINTABLE_LANG, &generated);
    match write_printable(dir, &html) {
        Ok(url) => {
            state.export_error = None;
            SheetOutcome::OpenPrintable(url)
        }
        Err(error) => {
            eprintln!(
                "shortcut sheet: printable export failed ({:?})",
                error.kind()
            );
            state.export_error = Some(ExportError { kind: error.kind() });
            SheetOutcome::None
        }
    }
}

fn focus_flag_id() -> Id {
    Id::new("shortcut-sheet-focus-inside")
}

fn clicked_inside_id() -> Id {
    Id::new("shortcut-sheet-clicked-inside")
}

/// Esc closes the sheet only when focus was inside it (S11). egui drops
/// widget focus at the start of the frame an Esc arrives, so the
/// previous frame's "focus inside" flag decides. Consumes the key so no
/// action fires from it. `App` calls this before the action dispatcher;
/// [`show`] calls it again (a no-op once consumed). Returns whether the
/// sheet was closed.
pub fn consume_escape(ctx: &Context, state: &mut ShortcutSheetState) -> bool {
    if !state.open {
        return false;
    }
    let had_focus = ctx
        .data(|data| data.get_temp::<bool>(focus_flag_id()))
        .unwrap_or(false);
    if had_focus && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        state.open = false;
        state.export_error = None;
        ctx.data_mut(|data| {
            data.insert_temp(focus_flag_id(), false);
            data.insert_temp(clicked_inside_id(), false);
        });
        return true;
    }
    false
}

/// Draw the sheet when `state.open`. Rebuilt from `registry` every
/// frame; read-only (no `×`, Add, Reset or capture). Esc closes it only
/// while focus is inside the sheet, and consumes the key.
pub fn show(
    ctx: &Context,
    state: &mut ShortcutSheetState,
    registry: &ActionRegistry,
) -> SheetOutcome {
    if !state.open {
        return SheetOutcome::None;
    }
    let platform = shortcut_groups::platform_of(ctx);
    let groups = shortcut_groups::snapshot(registry);
    let screen = ctx.content_rect();
    let default_size = vec2(
        (shortcut_groups::TWO_COLUMN_MIN_WIDTH + 96.0).min(screen.width() * 0.9),
        (screen.height() * 0.8).max(240.0),
    );

    // Esc is handled first so no later widget sees it this frame.
    if consume_escape(ctx, state) {
        return SheetOutcome::None;
    }
    let mut open = state.open;
    let mut export_clicked = false;
    let shown = Window::new(tr("shortcuts-sheet-title"))
        .id(window_id())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .vscroll(true)
        .default_size(default_size)
        .show(ctx, |ui| {
            if state.export_error.is_some() {
                ui.label(tr("shortcuts-printable-error"));
            }
            export_clicked = ui.button(tr("shortcuts-open-printable")).clicked();
            show_groups(ui, &groups, platform);
        });

    let mut close = !open;
    if let Some(inner) = &shown {
        let layer = inner.response.layer_id;
        let focused = ctx.memory(|memory| memory.focused());
        // A click on the sheet's body (not on a widget) focuses no widget,
        // yet the user has clearly moved into the sheet: remember it until
        // focus or a click goes elsewhere.
        let rect = inner.response.rect;
        let pressed_at = ctx.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let mut clicked_inside = ctx
            .data(|data| data.get_temp::<bool>(clicked_inside_id()))
            .unwrap_or(false);
        if let Some(pos) = pressed_at {
            clicked_inside = rect.contains(pos);
        }
        ctx.data_mut(|data| data.insert_temp(clicked_inside_id(), clicked_inside));
        let focus_inside = match focused.and_then(|id| ctx.read_response(id)) {
            Some(response) => response.layer_id == layer,
            None => focused.is_none() && clicked_inside,
        };
        ctx.data_mut(|data| data.insert_temp(focus_flag_id(), focus_inside));
    }
    if consume_escape(ctx, state) {
        close = true;
    }

    let mut outcome = SheetOutcome::None;
    if close {
        state.open = false;
        state.export_error = None;
        ctx.data_mut(|data| data.insert_temp(clicked_inside_id(), false));
    } else if export_clicked {
        outcome = export_printable(state, &groups, platform, &std::env::temp_dir());
    }
    outcome
}

/// The groups of `groups`, one or two columns by the available width
/// (the same rule and break as the controls page, S9).
fn show_groups(ui: &mut Ui, groups: &[ShortcutGroup], platform: Platform) {
    let visible = shortcut_groups::filter_groups(groups, "", platform);
    if shortcut_groups::use_two_columns(ui.available_width()) {
        let weights: Vec<usize> = visible.iter().map(|(_, rows)| 1 + rows.len()).collect();
        let split = shortcut_groups::column_break(&weights);
        let (left, right) = visible.split_at(split);
        ui.columns(2, |columns| {
            for (column, part) in columns.iter_mut().zip([left, right]) {
                for (group, rows) in part {
                    show_group(column, group, rows, platform);
                }
            }
        });
    } else {
        for (group, rows) in &visible {
            show_group(ui, group, rows, platform);
        }
    }
}

fn show_group(ui: &mut Ui, group: &ShortcutGroup, rows: &[FilteredRow], platform: Platform) {
    // Same styling and un-uppercased accessible name as the controls page.
    let heading = ui.label(crate::theme::section_label(&group.heading));
    ui.ctx().accesskit_node_builder(heading.id, |b| {
        b.set_label(group.heading.clone());
    });
    for filtered in rows {
        show_row(ui, filtered.row, platform);
    }
}

fn show_row(ui: &mut Ui, row: &ShortcutRow, platform: Platform) {
    let notes: Vec<(Chord, String)> = row
        .conflicts
        .iter()
        .map(|&chord| (chord, tr("shortcuts-conflict-marker")))
        .collect();
    let spoken = if row.bindings.is_empty() {
        tr("shortcuts-not-bound")
    } else {
        row.bindings
            .iter()
            .map(|&chord| KeyLabel::new(chord, platform).spoken)
            .collect::<Vec<_>>()
            .join(", ")
    };
    ui.horizontal_wrapped(|ui| {
        let name = shortcut_groups::paint_name_with_sense(
            ui,
            row,
            egui::Sense::focusable_noninteractive(),
        );
        let row_name = tr_args(
            "controls-row",
            &[("action", row.label.clone()), ("bindings", spoken)],
        );
        ui.ctx().accesskit_node_builder(name.id, |b| {
            b.set_label(row_name);
        });
        shortcut_groups::paint_bindings(ui, row, platform, &notes, &[], &mut |_, _| {});
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_posix() {
        assert_eq!(
            file_url(Path::new("/tmp/modplayer-shortcuts.html")),
            "file:///tmp/modplayer-shortcuts.html"
        );
    }

    #[test]
    fn file_url_windows_drive_with_space() {
        assert_eq!(
            file_url(Path::new(
                "C:\\Users\\A B\\AppData\\Local\\Temp\\modplayer-shortcuts.html"
            )),
            "file:///C:/Users/A%20B/AppData/Local/Temp/modplayer-shortcuts.html"
        );
    }

    #[test]
    fn file_url_percent_encodes_non_ascii() {
        assert_eq!(
            file_url(Path::new("/tmp/é.html")),
            "file:///tmp/%C3%A9.html"
        );
    }

    #[test]
    fn escape_covers_all_five() {
        assert_eq!(html_escape("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
    }
}
