//! End-to-end: bundle the counter example, run it on the engine thread and
//! tap a button after the idle GC has run.

use std::{path::PathBuf, time::Duration};

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{ApproxTextEngine, EventKind, MountBatch, MountItem, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

fn start() -> (Engine, Receiver<MountBatch>) {
    let entry = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/counter/App.tsx");
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
    })
    .unwrap();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, false).unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        bundle: Bundle::bytecode(bytecode),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

fn texts(batch: &MountBatch) -> Vec<String> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn tap_after_idle_gc_updates_the_ui() {
    let (engine, rx) = start();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    assert!(texts(&first).contains(&"0".to_owned()));
    let buttons: Vec<_> = first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(buttons.len(), 2, "two buttons");

    // Let the idle collector run: the app must stay reachable from JS.
    while rx.recv_timeout(Duration::from_millis(1300)).is_ok() {}

    let plus = buttons[1];
    engine
        .control()
        .dispatch_event(plus, EventKind::Press, nsh_core::EventPayload::None);
    let batch = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("update after tap");
    let texts = texts(&batch);
    assert!(texts.contains(&"1".to_owned()), "{texts:?}");
    assert!(texts.contains(&"• 1".to_owned()), "{texts:?}");
}
