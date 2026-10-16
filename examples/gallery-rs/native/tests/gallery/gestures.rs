//! The Gestures demo on a headless engine: press events are logged, taps
//! reach the button under the overlay, touches are logged with their
//! fingers.

use nsh_core::{EventKind, EventPayload, TouchEvent, TouchPhase, TouchPoint};

use crate::common::shows;

/// A touch of one finger on `target` at `(x, y)`.
fn touch(phase: TouchPhase, target: u32, x: f32, y: f32) -> EventPayload {
    EventPayload::Touch(Box::new(TouchEvent {
        phase,
        timestamp: 1000.0,
        points: vec![TouchPoint {
            id: 1,
            target,
            path: vec![target],
            page_x: x + 100.0,
            page_y: y + 200.0,
            location_x: x,
            location_y: y,
            changed: true,
        }],
    }))
}

#[test]
fn gestures_log_presses_and_touches() {
    let (engine, rx, mut screen) = crate::common::open("Gestures");
    for text in [
        "PRESS EVENTS",
        "No events yet",
        "No touches yet",
        "idle",
        "Touch here (try two fingers)",
        "The dot takes taps 30 pt around it: 0",
        "Ripple, long press 250 ms",
    ] {
        assert!(shows(&screen, text), "{text}: {:?}", screen.texts);
    }

    crate::common::press(&engine, &rx, &mut screen, "Tap, hold, or slide off");
    assert!(shows(&screen, "press"), "{:?}", screen.texts);
    crate::common::press(
        &engine,
        &rx,
        &mut screen,
        "Like (labelled for VoiceOver / TalkBack)",
    );
    assert!(shows(&screen, "Like · press"), "{:?}", screen.texts);

    crate::common::press(&engine, &rx, &mut screen, "Button under an overlay: 0");
    assert!(
        shows(&screen, "Button under an overlay: 1"),
        "{:?}",
        screen.texts
    );

    let label = screen.find("Touch here (try two fingers)").unwrap();
    let area = screen.parents[&label];
    engine.control().dispatch_event(
        area,
        EventKind::Touch,
        touch(TouchPhase::Start, area, 10.4, 20.6),
    );
    crate::common::drain(&rx, &mut screen);
    assert!(
        shows(&screen, "start at 10,21 (page 110,221), 1 down"),
        "{:?}",
        screen.texts
    );
    engine.control().dispatch_event(
        area,
        EventKind::Touch,
        touch(TouchPhase::End, area, 12.0, 22.0),
    );
    crate::common::drain(&rx, &mut screen);
    assert!(
        shows(
            &screen,
            "end at 12,22 (page 112,222), 0 down\nstart at 10,21 (page 110,221), 1 down"
        ),
        "{:?}",
        screen.texts
    );
}
