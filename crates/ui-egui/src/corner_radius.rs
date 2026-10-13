//! On-canvas corner radius widgets for live rectangles (TOOL-214-12), as in Photoshop: with Path
//! Selection or the Rectangle tool, the active shape layer's live rectangle shows a round widget
//! inside each corner, at the centre of its arc. Dragging one rounds every corner alike; ⌥/Alt
//! drags that corner alone. The drag previews the new outline and commits one `shape.edit` with
//! `radii` on release (one history step); a click without movement changes nothing.

use egui::{Color32, Modifiers, Pos2, Stroke};
use photocraft_doc::LayerContent;
use photocraft_doc::vector::LiveShape;

use crate::PhotocraftApp;
use crate::canvas::ToolEvent;
use crate::state::Tool;

/// A widget sits at least this many screen points inside its corner, so a sharp corner's widget
/// can still be grabbed.
const MIN_INSET: f64 = 10.0;

/// A corner radius drag in progress.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    layer: u64,
    /// The live rectangle `[x, y, w, h]` (positive size) and its radii at the press.
    rect: [f64; 4],
    radii: [f64; 4],
    /// The dragged corner, `[tl, tr, br, bl]` order.
    corner: usize,
    /// Every corner (no ⌥) or the dragged one only.
    all: bool,
    /// Where the press was.
    start: [f64; 2],
    /// The radii so far.
    now: [f64; 4],
}

fn shows_for(tool: Tool) -> bool {
    matches!(tool, Tool::PathSelection | Tool::Rectangle)
}

/// The active layer's live rectangle: its id, `[x, y, w, h]` with a positive size, and its radii.
fn live_rect(app: &PhotocraftApp) -> Option<(u64, [f64; 4], [f64; 4])> {
    if !shows_for(app.ui.tool) || app.ui.transform.is_some() {
        return None;
    }
    let st = app.session.active()?;
    let l = st.active_layer.and_then(|id| st.doc.layer(id))?;
    let LayerContent::Shape(sh) = &l.content else { return None };
    let Some(LiveShape::Rect { rect: [x, y, w, h], radii }) = sh.live else { return None };
    let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
    let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
    (w > 0.0 && h > 0.0 && [x, y, w, h].iter().chain(&radii).all(|v| v.is_finite())).then_some((l.id.0, [x, y, w, h], radii))
}

/// The largest radius a corner of `rect` takes.
fn max_radius(rect: [f64; 4]) -> f64 {
    rect[2].min(rect[3]) / 2.0
}

/// Corner `i` of `rect` and the unit-step direction pointing inside from it.
fn corner(rect: [f64; 4], i: usize) -> ([f64; 2], [f64; 2]) {
    let [x, y, w, h] = rect;
    match i {
        0 => ([x, y], [1.0, 1.0]),
        1 => ([x + w, y], [-1.0, 1.0]),
        2 => ([x + w, y + h], [-1.0, -1.0]),
        _ => ([x, y + h], [1.0, -1.0]),
    }
}

/// Where corner `i`'s widget is drawn: the centre of its arc, or a few screen points inside a
/// sharper corner.
fn widget(rect: [f64; 4], radii: [f64; 4], i: usize, zoom: f64) -> [f64; 2] {
    let (c, d) = corner(rect, i);
    let r = radii.get(i).copied().unwrap_or(0.0).max(MIN_INSET / zoom).min(max_radius(rect));
    [c[0] + d[0] * r, c[1] + d[1] * r]
}

/// The radius corner `i` gets when the pointer has moved from `start` to `p`: its radius at the
/// press plus the move projected onto the corner's diagonal (inward rounds more), within 0..half
/// the shorter side. Relative, so a click on a sharp corner's inset widget changes nothing.
fn radius_after(rect: [f64; 4], i: usize, from: f64, start: [f64; 2], p: [f64; 2]) -> f64 {
    let (_, d) = corner(rect, i);
    let r = from + ((p[0] - start[0]) * d[0] + (p[1] - start[1]) * d[1]) / 2.0;
    if r.is_finite() { r.clamp(0.0, max_radius(rect)) } else { from }
}

fn zoom(app: &PhotocraftApp) -> f64 {
    f64::from(app.point_zoom().max(0.01))
}

/// Canvas pointer events for the corner widgets; false when the event is not theirs.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> bool {
    match ev {
        ToolEvent::Down { x, y, .. } => {
            if mods.command {
                return false;
            }
            let Some((layer, rect, radii)) = live_rect(app) else { return false };
            let z = zoom(app);
            let tol = 7.0 / z;
            let hit = (0..4)
                .map(|i| {
                    let w = widget(rect, radii, i, z);
                    (i, (w[0] - x).hypot(w[1] - y))
                })
                .filter(|(_, d)| *d <= tol)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((corner, _)) = hit else { return false };
            app.ui.corner_radius = Some(Drag { layer, rect, radii, corner, all: !mods.alt, start: [x, y], now: radii });
            true
        }
        ToolEvent::Move { x, y, .. } => {
            let Some(d) = app.ui.corner_radius.as_mut() else { return false };
            let from = d.radii.get(d.corner).copied().unwrap_or(0.0).clamp(0.0, max_radius(d.rect));
            let r = radius_after(d.rect, d.corner, from, d.start, [x, y]);
            if d.all {
                d.now = [r; 4];
            } else if let Some(v) = d.now.get_mut(d.corner) {
                *v = r;
            }
            true
        }
        ToolEvent::Up { x, y } => {
            if app.ui.corner_radius.is_none() {
                return false;
            }
            pointer(app, ToolEvent::Move { x, y, pressure: 1.0 }, mods);
            let Some(d) = app.ui.corner_radius.take() else { return true };
            if d.now != d.radii
                && let Err(e) = app.run("shape.edit", serde_json::json!({"layer": d.layer, "radii": d.now}))
            {
                app.ui.status = e;
                app.ui.status_error = true;
            }
            true
        }
    }
}

/// The widgets of the active live rectangle, and while one is dragged, the new outline.
pub fn draw(app: &PhotocraftApp, painter: &egui::Painter, to_scr: &dyn Fn([f64; 2]) -> Pos2, accent: Color32) {
    let Some((layer, rect, radii)) = live_rect(app) else { return };
    let drag = app.ui.corner_radius.as_ref().filter(|d| d.layer == layer);
    let radii = drag.map_or(radii, |d| d.now);
    if drag.is_some() {
        let path = photocraft_vector::shapes::rounded_rect(rect[0], rect[1], rect[2], rect[3], radii);
        crate::vector_ui::draw_outline(painter, &path, to_scr, accent);
    }
    let z = zoom(app);
    for i in 0..4 {
        let p = to_scr(widget(rect, radii, i, z));
        painter.circle(p, 5.0, Color32::WHITE, Stroke::new(1.5, accent));
        painter.circle_filled(p, 2.5, accent);
    }
}

#[cfg(test)]
#[path = "corner_radius_tests.rs"]
mod tests;
