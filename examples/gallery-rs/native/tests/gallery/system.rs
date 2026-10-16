//! The System demo: the environment, the status bar's settings, system
//! dialogs and their answers, a timer.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    Command, Dialog, EventKind, EventPayload, MountBatch, MountItem, PayloadValue, ROOT_ID,
};
use nsh_engine::Engine;

use crate::common::shows;

/// Presses the parent of the text `label` and returns the dialog it shows.
fn press_for_dialog(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    label: &str,
) -> Dialog {
    let text = screen.find(label).expect(label);
    engine
        .control()
        .dispatch_event(screen.parents[&text], EventKind::Press, EventPayload::None);
    let mut dialog = None;
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
        screen.apply(&batch);
        dialog = dialog.or_else(|| {
            batch.items.iter().find_map(|item| match item {
                MountItem::Command {
                    id: ROOT_ID,
                    command: Command::Dialog(d),
                } => Some((**d).clone()),
                _ => None,
            })
        });
    }
    dialog.unwrap_or_else(|| panic!("{label} shows no dialog"))
}

/// Answers `dialog` with button `index`.
fn answer(
    engine: &Engine,
    rx: &Receiver<MountBatch>,
    screen: &mut crate::common::Screen,
    dialog: &Dialog,
    index: i32,
) {
    engine.control().dispatch_event(
        ROOT_ID,
        EventKind::Dialog,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(dialog.id as f64)),
            ("index", PayloadValue::Number(index as f64)),
        ]),
    );
    crate::common::drain(rx, screen);
}

#[test]
fn system_shows_the_environment_and_the_status_bar() {
    let (engine, rx, mut screen) = crate::common::open("System");
    for text in [
        "ENVIRONMENT (REACTIVE)",
        "390 × 844",
        "0 / 0 / 0 / 0",
        "0 pt",
        "light",
        "1.00",
        "STATUS BAR",
        "default",
        "ALERTS",
        "Answer",
        "FRAMES AND TIMERS",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }
    crate::common::press(&engine, &rx, &mut screen, "Next style");
    assert!(shows(&screen, "light-content"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Next style");
    assert!(shows(&screen, "dark-content"), "{:?}", screen.texts);
    crate::common::press(&engine, &rx, &mut screen, "Hide");
    assert!(shows(&screen, "Show"), "{:?}", screen.texts);
}

#[test]
fn system_shows_what_dialogs_answer() {
    let (engine, rx, mut screen) = crate::common::open("System");
    let dialog = press_for_dialog(&engine, &rx, &mut screen, "Confirm (cancel / destructive)");
    assert_eq!(dialog.title.as_deref(), Some("Delete the photo?"));
    assert_eq!(dialog.buttons.len(), 2);
    assert!(dialog.cancelable && !dialog.sheet);
    answer(&engine, &rx, &mut screen, &dialog, 1);
    assert!(shows(&screen, "Delete"), "{:?}", screen.texts);

    let sheet = press_for_dialog(&engine, &rx, &mut screen, "Action sheet");
    assert!(sheet.sheet);
    assert_eq!(sheet.buttons[2].text, "Save");
    answer(&engine, &rx, &mut screen, &sheet, 2);
    assert!(shows(&screen, "Save"), "{:?}", screen.texts);

    // Dismissing the sheet is its cancel action.
    let sheet = press_for_dialog(&engine, &rx, &mut screen, "Action sheet");
    answer(&engine, &rx, &mut screen, &sheet, -1);
    assert!(shows(&screen, "Cancel"), "{:?}", screen.texts);
}

#[test]
fn system_counts_seconds() {
    let (_engine, rx, mut screen) = crate::common::open("System");
    std::thread::sleep(Duration::from_millis(1100));
    crate::common::drain(&rx, &mut screen);
    assert!(
        screen
            .texts
            .iter()
            .any(|t| t.1.parse::<u32>().is_ok_and(|n| n >= 1)),
        "{:?}",
        screen.texts
    );
}
