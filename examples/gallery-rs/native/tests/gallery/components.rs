//! The Components demo on a headless engine: its sections render, the
//! mobile UI's buttons and the press feedback cards answer presses, the
//! keyboard avoiding view's screen pushes.

use crate::common::shows;

#[test]
fn components_render_and_answer_presses() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    for label in [
        "NSH/UI/MOBILE",
        "Primary (0)",
        "Card with slots",
        "An outlined card, children only.",
        "PRESS FEEDBACK",
        "Last pressed: nothing yet",
        "Open a sheet",
        "LAYOUT ANIMATION",
        "0 characters",
        "MODAL OPTIONS",
        "statusBarTranslucent: false",
        "cx.reduce_motion()",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    // A nested Rust component's event reaches this demo's state.
    crate::common::press(&engine, &rx, &mut screen, "Primary (0)");
    crate::common::press(&engine, &rx, &mut screen, "Secondary");
    assert!(shows(&screen, "Primary (2)"), "{:?}", screen.texts);
    // Its slot's content: the footer's button.
    crate::common::press(&engine, &rx, &mut screen, "Reset");
    assert!(shows(&screen, "Primary (0)"), "{:?}", screen.texts);

    crate::common::press(
        &engine,
        &rx,
        &mut screen,
        "activeOpacity: fades while pressed",
    );
    assert!(
        shows(&screen, "Last pressed: activeOpacity"),
        "{:?}",
        screen.texts
    );
    crate::common::press(&engine, &rx, &mut screen, "Disable");
    assert!(
        shows(&screen, "disabled: takes no presses") && shows(&screen, "Enable"),
        "{:?}",
        screen.texts
    );

    // Layout animation: the boxes 1, 2, 3; Add puts 4 first.
    assert!(!shows(&screen, "4"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Add");
    assert!(shows(&screen, "4"), "{:?}", screen.texts);

    crate::common::press(&engine, &rx, &mut screen, "statusBarTranslucent: false");
    assert!(shows(&screen, "statusBarTranslucent: true"));
    crate::common::press(&engine, &rx, &mut screen, "Open");
    assert!(
        shows(
            &screen,
            "The yellow background starts under the status bar on Android."
        ),
        "{:?}",
        screen.texts
    );
}

#[test]
fn components_push_the_keyboard_avoiding_view() {
    let (engine, rx, mut screen) = crate::common::open("Components");
    crate::common::press(&engine, &rx, &mut screen, "padding / height / position");
    assert!(
        screen.titles.iter().any(|t| t.1 == "KeyboardAvoidingView"),
        "{:?}",
        screen.titles
    );
    assert!(shows(&screen, "behavior=\"padding\""), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "position");
    assert!(
        shows(&screen, "behavior=\"position\""),
        "{:?}",
        screen.texts
    );
}
