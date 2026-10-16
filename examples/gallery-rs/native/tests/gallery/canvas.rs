//! The Canvas demo on a headless engine: its sections render and its
//! buttons draw again.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch, MountItem, canvas::CanvasOp};
use nsh_engine::Engine;

/// Presses the parent of the text `label` and returns the texts the
/// canvases drew in what it changed.
fn press_drawing(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    label: &str,
) -> Vec<String> {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    let target = screen.parents[&text];
    engine
        .control()
        .dispatch_event(target, EventKind::Press, EventPayload::None);
    let mut drawn = Vec::new();
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        for item in &batch.items {
            if let MountItem::Canvas { drawing, .. } = item {
                drawn.extend(drawing.ops.iter().filter_map(|op| match op {
                    CanvasOp::FillText { text, .. } => Some(text.to_string()),
                    _ => None,
                }));
            }
        }
    }
    drawn
}

#[test]
fn canvas_renders_its_sections() {
    let (_engine, _rx, screen) = crate::common::open("Canvas");
    for text in [
        "NATIVE ANIMATION (PROGRESS RING)",
        "CHART",
        "TRANSFORMS (A CLOCK)",
        "PATH2D (SVG PATH DATA)",
        "heart",
        "check",
        "IMAGES, COMPOSITING, SHADOWS",
        "SKETCH (TOUCH → DRAW)",
        "Random",
        "New data",
        "Clear",
    ] {
        assert!(
            screen.texts.iter().any(|t| t.1 == text),
            "{text}: {:?}",
            screen.texts
        );
    }
}

#[test]
fn full_draws_the_ring_again() {
    let (engine, rx, mut screen) = crate::common::open("Canvas");
    let drawn = press_drawing(&engine, &rx, &mut screen, "Full");
    assert!(drawn.iter().any(|t| t == "100%"), "{drawn:?}");
}
