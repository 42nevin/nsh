//! The Images demo on a headless engine: its sections render, its images
//! report their events, "Show" and "Reload" change what loads.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{EventKind, EventPayload, MountBatch, MountItem};
use nsh_engine::Engine;

/// What is on screen, and the sources of the images by node.
#[derive(Default)]
struct Shown {
    screen: crate::common::Screen,
    images: Vec<(u32, String)>,
}

impl Shown {
    fn drain(&mut self, rx: &Receiver<MountBatch>) {
        while let Ok(batch) = rx.recv_timeout(Duration::from_millis(300)) {
            self.screen.apply(&batch);
            for item in &batch.items {
                if let MountItem::Image {
                    id,
                    source: Some(source),
                    ..
                } = item
                {
                    self.images.retain(|i| i.0 != *id);
                    self.images.push((*id, source.uri.to_string()));
                }
            }
        }
    }

    fn image(&self, uri: &str) -> u32 {
        self.images
            .iter()
            .find(|i| i.1 == uri)
            .unwrap_or_else(|| panic!("no image {uri}: {:?}", self.images))
            .0
    }

    fn shows(&self, text: &str) -> bool {
        self.screen.texts.iter().any(|t| t.1 == text)
    }

    fn press(&mut self, engine: &Engine, rx: &Receiver<MountBatch>, label: &str) {
        let text = self
            .screen
            .find(label)
            .unwrap_or_else(|| panic!("no {label}: {:?}", self.screen.texts));
        let target = self.screen.parents[&text];
        engine
            .control()
            .dispatch_event(target, EventKind::Press, EventPayload::None);
        self.drain(rx);
    }

    fn send(
        &mut self,
        engine: &Engine,
        rx: &Receiver<MountBatch>,
        id: u32,
        kind: EventKind,
        payload: EventPayload,
    ) {
        engine.control().dispatch_event(id, kind, payload);
        self.drain(rx);
    }
}

fn open() -> (Engine, Receiver<MountBatch>, Shown) {
    let (engine, rx) = crate::common::start();
    let mut shown = Shown::default();
    shown.drain(&rx);
    shown.press(&engine, &rx, "Images");
    assert!(
        shown.screen.titles.iter().any(|t| t.1 == "Images"),
        "Images opens: {:?}",
        shown.screen.titles
    );
    (engine, rx, shown)
}

#[test]
fn images_render_their_sections() {
    let (_engine, _rx, shown) = open();
    for text in [
        "RESIZE MODES (A 2:1 IMAGE IN SQUARES)",
        "cover",
        "center",
        "logo@2x.png",
        "#1a7f37",
        "spinner.webp",
        "fade 300ms",
        "blur 16",
        "Prefetch, then show: no download, no progress",
        "no load events yet",
    ] {
        assert!(shown.shows(text), "{text}: {:?}", shown.screen.texts);
    }
    shown.image("asset:/bounce.gif");
    shown.image("https://picsum.photos/seed/nsh-fade-1-1000/300/300");
}

#[test]
fn images_report_their_events() {
    let (engine, rx, mut shown) = open();
    let missing = shown.image("https://example.invalid/missing.png");
    shown.send(
        &engine,
        &rx,
        missing,
        EventKind::Error,
        EventPayload::Error {
            message: "not found".into(),
        },
    );
    assert!(
        shown.shows("onError: not found"),
        "{:?}",
        shown.screen.texts
    );

    let loads = shown.image("https://picsum.photos/seed/nsh-load/300/200");
    shown.send(
        &engine,
        &rx,
        loads,
        EventKind::LoadStart,
        EventPayload::None,
    );
    shown.send(
        &engine,
        &rx,
        loads,
        EventKind::Load,
        EventPayload::Image {
            width: 300.0,
            height: 200.0,
        },
    );
    shown.send(&engine, &rx, loads, EventKind::LoadEnd, EventPayload::None);
    assert!(shown.shows("onLoad: 300×200"), "{:?}", shown.screen.texts);
    assert!(
        shown.shows("onLoadStart → onLoad → onLoadEnd"),
        "{:?}",
        shown.screen.texts
    );
}

#[test]
fn show_and_reload_change_what_loads() {
    let (engine, rx, mut shown) = open();
    shown.press(&engine, &rx, "Show");
    assert!(shown.shows("Hide"), "{:?}", shown.screen.texts);
    let big = shown.image("https://picsum.photos/seed/nsh-progress/2400/1600");
    shown.send(
        &engine,
        &rx,
        big,
        EventKind::Load,
        EventPayload::Image {
            width: 2400.0,
            height: 1600.0,
        },
    );
    assert!(
        shown.shows("loaded (no download)"),
        "{:?}",
        shown.screen.texts
    );

    shown.press(&engine, &rx, "Reload");
    shown.image("https://picsum.photos/seed/nsh-fade-2-0/300/300");
}
