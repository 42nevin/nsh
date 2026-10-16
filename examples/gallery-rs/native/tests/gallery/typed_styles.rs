//! The Typed styles demo: the Tailwind demo's cards in typed styles, its
//! buttons' state and the canvases standing in for gradients.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{MountBatch, MountItem, ViewKind};

use crate::common::shows;

/// The canvases `rx`'s batches create, applying them to `screen`.
fn canvases(rx: &Receiver<MountBatch>, screen: &mut crate::common::Screen) -> usize {
    let mut created = 0;
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        created += batch
            .items
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    MountItem::Create {
                        kind: ViewKind::Canvas,
                        ..
                    }
                )
            })
            .count();
    }
    created
}

#[test]
fn typed_styles_shows_every_card() {
    let (_engine, _rx, screen) = crate::common::open("Typed styles");
    for text in [
        "BUTTONS: ACTIVE, HOVER, TRANSITION, DISABLED",
        "CARDS: SHADOW, RING, GROUP, GRADIENT",
        "FLEX, GRID, BREAKPOINTS",
        "PALETTE",
        "SHADOWS, RINGS, OUTLINES",
        "GRADIENTS",
        "TYPOGRAPHY",
        "FILTERS, BACKDROP BLUR, BLEND MODES",
        "ANIMATIONS AND TRANSFORMS",
        "FIRST:, EVEN:, LAST:",
        "INPUTS",
        "Group press",
        "Gradient card",
        "col-span-2",
        "emerald",
        "2xl",
        "Display",
        "blur-sm",
        "bounce",
        "Archive",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }
}

#[test]
fn typed_styles_buttons_keep_their_state() {
    let (engine, rx, mut screen) = crate::common::open("Typed styles");
    crate::common::press(&engine, &rx, &mut screen, "Pressed 0×");
    crate::common::press(&engine, &rx, &mut screen, "Pressed 1×");
    assert!(shows(&screen, "Pressed 2×"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Reset");
    assert!(shows(&screen, "Pressed 0×"), "{:?}", screen.texts);

    crate::common::press(&engine, &rx, &mut screen, "Enabled");
    assert!(shows(&screen, "Disabled"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Enable again");
    assert!(shows(&screen, "Enabled"), "{:?}", screen.texts);
}

#[test]
fn typed_styles_draws_gradients_on_canvases() {
    let (engine, rx) = crate::common::start();
    let mut screen = crate::common::Screen::default();
    crate::common::drain(&rx, &mut screen);
    let text = screen.find("Typed styles").expect("the demo in the index");
    engine.control().dispatch_event(
        screen.parents[&text],
        nsh_core::EventKind::Press,
        nsh_core::EventPayload::None,
    );
    // The four gradients' canvases, and the gradient card's once its
    // layout tells the card its size.
    assert_eq!(canvases(&rx, &mut screen), 5);
}
