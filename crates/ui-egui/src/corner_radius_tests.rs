//! TOOL-214-12: a live rectangle's corner radius widgets, as in Photoshop.

use egui::Modifiers;
use photocraft_doc::LayerContent;
use photocraft_doc::vector::LiveShape;
use serde_json::json;

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, tool_event};
use crate::state::Tool;

/// A 200 × 200 document at 100 % with a live rectangle at (40, 40), 120 × 80, radii 10.
fn app(tool: Tool) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": 200, "height": 200})).unwrap();
    app.sync_views();
    app.run("shape.create", json!({"kind": "roundedRect", "rect": [40, 40, 120, 80], "radii": 10})).unwrap();
    app.ui.tool = tool;
    app
}

fn radii(app: &PhotocraftApp) -> Option<[f64; 4]> {
    let st = app.session.active()?;
    let l = st.active_layer.and_then(|id| st.doc.layer(id))?;
    match &l.content {
        LayerContent::Shape(sh) => match sh.live {
            Some(LiveShape::Rect { radii, .. }) => Some(radii),
            _ => None,
        },
        _ => None,
    }
}

fn steps(app: &PhotocraftApp) -> usize {
    app.session.active().unwrap().history.entries().len()
}

fn drag(app: &mut PhotocraftApp, from: [f64; 2], to: [f64; 2], mods: Modifiers) {
    tool_event(app, ToolEvent::Down { x: from[0], y: from[1], pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Move { x: to[0], y: to[1], pressure: 1.0 }, mods);
    tool_event(app, ToolEvent::Up { x: to[0], y: to[1] }, mods);
}

#[test]
fn dragging_a_widget_rounds_every_corner_in_one_step() {
    let mut app = app(Tool::PathSelection);
    let n = steps(&app);
    // The top-left widget sits at the arc centre (50, 50); dragging it 15 px along the diagonal
    // adds 15 to the radius.
    drag(&mut app, [50.0, 50.0], [65.0, 65.0], Modifiers::NONE);
    assert_eq!(radii(&app), Some([25.0; 4]));
    assert_eq!(steps(&app), n + 1, "one history step");
    assert!(app.ui.corner_radius.is_none());
    assert!(app.session.undo());
    assert_eq!(radii(&app), Some([10.0; 4]));
}

#[test]
fn alt_drags_one_corner_and_radii_stay_within_half_the_short_side() {
    let mut app = app(Tool::Rectangle);
    // The bottom-right widget, with ⌥: only that corner, capped at 40 (half of the height 80).
    drag(&mut app, [150.0, 110.0], [20.0, 20.0], Modifiers::ALT);
    assert_eq!(radii(&app), Some([10.0, 10.0, 40.0, 10.0]));
    // Dragging out past the corner makes it sharp.
    drag(&mut app, [150.0, 50.0], [190.0, 10.0], Modifiers::ALT);
    assert_eq!(radii(&app), Some([10.0, 0.0, 40.0, 10.0]));
    // The Rectangle tool drew no new shape.
    assert_eq!(app.session.active().unwrap().doc.layers.len(), 2);
}

#[test]
fn a_sharp_corner_keeps_a_grabbable_widget_and_a_click_changes_nothing() {
    let mut app = app(Tool::PathSelection);
    let id = app.session.active().unwrap().active_layer.unwrap().0;
    app.run("shape.edit", json!({"layer": id, "radii": 0})).unwrap();
    let n = steps(&app);
    // At 100 % the widget of a sharp corner is 10 px inside it.
    drag(&mut app, [50.0, 50.0], [50.0, 50.0], Modifiers::NONE);
    assert_eq!((radii(&app), steps(&app)), (Some([0.0; 4]), n), "a click commits nothing");
    drag(&mut app, [50.0, 50.0], [60.0, 60.0], Modifiers::NONE);
    assert_eq!(radii(&app), Some([10.0; 4]));
}

#[test]
fn only_live_rectangles_with_the_right_tools_take_the_press() {
    for tool in [Tool::Move, Tool::DirectSelection, Tool::Brush] {
        let mut a = app(tool);
        assert!(!super::pointer(&mut a, ToolEvent::Down { x: 50.0, y: 50.0, pressure: 1.0 }, Modifiers::NONE), "{tool:?}");
    }
    // ⌘/Ctrl leaves the press to the tool.
    let mut a = app(Tool::PathSelection);
    assert!(!super::pointer(&mut a, ToolEvent::Down { x: 50.0, y: 50.0, pressure: 1.0 }, Modifiers::COMMAND));
    // An ellipse has no corners.
    let mut a = app_with(json!({"kind": "ellipse", "rect": [40, 40, 120, 80]}));
    assert!(!super::pointer(&mut a, ToolEvent::Down { x: 50.0, y: 50.0, pressure: 1.0 }, Modifiers::NONE));
    // A press off the widgets is the tool's.
    let mut a = app(Tool::PathSelection);
    assert!(!super::pointer(&mut a, ToolEvent::Down { x: 100.0, y: 80.0, pressure: 1.0 }, Modifiers::NONE));
}

fn app_with(shape: serde_json::Value) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.session.execute("file.new", json!({"width": 200, "height": 200})).unwrap();
    app.sync_views();
    app.run("shape.create", shape).unwrap();
    app.ui.tool = Tool::PathSelection;
    app
}
