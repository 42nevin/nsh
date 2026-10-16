//! The Text demo on a headless engine: its texts render, links and the
//! button inside a sentence take presses.

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch};
use nsh_engine::Engine;

use crate::common::shows;

/// Sends `kind` to the text `label` itself (a link, not its sentence).
fn on_text(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    label: &str,
    kind: EventKind,
) {
    let text = screen
        .find(label)
        .unwrap_or_else(|| panic!("no {label}: {:?}", screen.texts));
    engine
        .control()
        .dispatch_event(text, kind, EventPayload::None);
    crate::common::drain(rx, screen);
}

#[test]
fn text_renders_its_samples() {
    let (_engine, _rx, screen) = crate::common::open("Text");
    for text in [
        "Terms of service",
        "Pressed: Tap a link in the sentence",
        "LIVE",
        "♥ 3",
        "Custom fonts",
        "UPPERCASE ",
        "Capitalized Words Here",
        "Text shadow",
        "System text size (fontScale 1.00)",
        "numberOfLines=1 head / middle / clip",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }
}

#[test]
fn links_and_the_inline_button_take_presses() {
    let (engine, rx, mut screen) = crate::common::open("Text");
    on_text(
        &engine,
        &rx,
        &mut screen,
        "Privacy policy",
        EventKind::Press,
    );
    assert!(
        shows(&screen, "Pressed: Privacy policy"),
        "{:?}",
        screen.texts
    );
    on_text(&engine, &rx, &mut screen, "links", EventKind::Press);
    assert!(
        shows(&screen, "Pressed: a link inside bold text"),
        "{:?}",
        screen.texts
    );
    on_text(
        &engine,
        &rx,
        &mut screen,
        "Terms of service",
        EventKind::LongPress,
    );
    assert!(
        shows(&screen, "Pressed: Terms (long press)"),
        "{:?}",
        screen.texts
    );
    // The sentence around the links.
    crate::common::press(&engine, &rx, &mut screen, "Privacy policy");
    assert!(
        shows(&screen, "Pressed: the sentence (not a link)"),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "♥ 3");
    assert!(shows(&screen, "♥ 4"), "{:?}", screen.texts);
}
