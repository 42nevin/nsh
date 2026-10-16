//! The Styles demo on a headless engine: its cards render, the clipping
//! card toggles.

#[test]
fn styles_render_their_cards() {
    let (_engine, _rx, screen) = crate::common::open("Styles");
    for text in [
        "Shadow",
        "Top corners",
        "Mixed radii",
        "Clipped (tap)",
        "Per-side borders",
        "zIndex (tap)",
        "rotateY + perspective",
        "skewX (iOS)",
        "Front (tap)",
        "Back: backfaceVisibility",
        "Left to right",
        "12",
    ] {
        assert!(
            screen.texts.iter().any(|t| t.1 == text),
            "{text}: {:?}",
            screen.texts
        );
    }
}

#[test]
fn the_clipping_card_toggles() {
    let (engine, rx, mut screen) = crate::common::open("Styles");
    crate::common::press(&engine, &rx, &mut screen, "Clipped (tap)");
    assert!(
        screen.texts.iter().any(|t| t.1 == "Visible (tap)"),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "Visible (tap)");
    assert!(
        screen.texts.iter().any(|t| t.1 == "Clipped (tap)"),
        "{:?}",
        screen.texts
    );
}
