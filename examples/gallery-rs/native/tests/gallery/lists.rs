//! The Lists demo on a headless engine: its tabs switch, the chat sends,
//! the empty list fills and clears.

use nsh_core::{EventKind, EventPayload};

use crate::common::{press_text, shows};

#[test]
fn lists_switch_tabs_and_change_their_rows() {
    let (engine, rx, mut screen) = crate::common::open("Lists");
    for label in [
        "Grid", "Sections", "Chat", "More", "Visible ", "Item 60", "End", "0", "1",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }

    press_text(&engine, &rx, &mut screen, "Sections");
    for label in ["A", "Ada", "Alan", "B", "Barbara"] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    assert!(!shows(&screen, "Item 60"), "the grid is gone");

    // The chat: type, then send.
    press_text(&engine, &rx, &mut screen, "Chat");
    assert!(shows(&screen, "Message 30"), "{:?}", screen.texts);
    let send = screen.find("Send").expect("a send button");
    let bar = screen.parents[&send];
    let input = screen
        .parents
        .iter()
        .find(|&(&child, &parent)| parent == bar && child != send)
        .map(|(&child, _)| child)
        .expect("the message input");
    engine.control().dispatch_event(
        input,
        EventKind::ChangeText,
        EventPayload::Text {
            text: "Hello there".into(),
            count: 1,
        },
    );
    crate::common::drain(&rx, &mut screen);
    press_text(&engine, &rx, &mut screen, "Send");
    assert!(shows(&screen, "Hello there"), "{:?}", screen.texts);

    // The empty list: Add fills it, Clear empties it.
    press_text(&engine, &rx, &mut screen, "More");
    for label in [
        "Pull down to refresh (refreshed 0 times)",
        "Today",
        "A horizontal list of 1000 recycled cards",
        "An empty list (0 items)",
        "Nothing here yet: tap Add",
    ] {
        assert!(shows(&screen, label), "{label}: {:?}", screen.texts);
    }
    press_text(&engine, &rx, &mut screen, "Add");
    press_text(&engine, &rx, &mut screen, "Add");
    assert!(
        shows(&screen, "An empty list (2 items)"),
        "{:?}",
        screen.texts
    );
    assert!(shows(&screen, "Item 1") && shows(&screen, "Item 2"));
    assert!(!shows(&screen, "Nothing here yet: tap Add"));
    press_text(&engine, &rx, &mut screen, "Clear");
    assert!(
        shows(&screen, "An empty list (0 items)"),
        "{:?}",
        screen.texts
    );
    assert!(shows(&screen, "Nothing here yet: tap Add"));
}
