//! A Rust app: the engine mounts its root component with no bundle and no
//! VM; it renders, handles presses and runs its tasks.

use std::time::Duration;

use crossbeam_channel::Receiver;
use nsh_core::{
    ApproxTextEngine, EventKind, EventPayload, MountBatch, MountItem, Size,
    ui::{Root, prelude::*},
};
use nsh_engine::{App, Engine, EngineConfig, Environment};

use crate::ChannelSink;

#[derive(Default)]
struct Counter {
    count: u32,
    ticks: u32,
    _timer: Option<Task<()>>,
}

#[derive(Clone, PartialEq, Props)]
struct CounterProps {
    #[prop(default = "Count".into())]
    label: SharedString,
}

impl Component for Counter {
    const NAME: &'static str = "app.counter";
    type Props = CounterProps;
    type Event = NoEvent;

    fn setup(&mut self, _: &CounterProps, cx: &mut Context<'_, Self>) {
        self._timer = Some(
            cx.set_timeout(Duration::from_millis(10), |this: &mut Self, cx| {
                this.ticks += 1;
                cx.notify();
            }),
        );
    }

    fn render(&self, props: &CounterProps, cx: &mut RenderCx<'_, Self>) -> impl IntoElement {
        view()
            .flex_1()
            .on_press(cx.listener(|this: &mut Self, _, cx| {
                this.count += 1;
                cx.notify();
            }))
            .child(text(format!(
                "{} {} ({} ticks)",
                props.label, self.count, self.ticks
            )))
    }
}

fn start() -> (Engine, Receiver<MountBatch>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        app: App::Rust(Root::new::<Counter>(|| CounterProps {
            label: "Taps".into(),
        })),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Environment::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        // Nothing registered: Rust renders its components by type.
        components: Default::default(),
        runtime: None,
    };
    (
        Engine::start(config, || ApproxTextEngine, ChannelSink(tx)).unwrap(),
        rx,
    )
}

/// The text shown after the batches received within `wait`, and the view
/// it is in.
fn shown(rx: &Receiver<MountBatch>, wait: Duration, until: &str) -> Option<(String, u32)> {
    let deadline = std::time::Instant::now() + wait;
    let mut last = None;
    let mut host = None;
    while let Ok(batch) = rx.recv_deadline(deadline) {
        for item in &batch.items {
            match item {
                MountItem::Text { text, .. } => last = Some(text.text.clone()),
                MountItem::Insert {
                    parent: nsh_core::ROOT_ID,
                    child,
                    ..
                } => host = Some(*child),
                _ => {}
            }
        }
        if last.as_deref() == Some(until) {
            break;
        }
    }
    Some((last?, host.unwrap_or(0)))
}

#[test]
fn a_rust_app_renders_handles_presses_and_runs_tasks() {
    let (engine, rx) = start();
    let (text, host) = shown(&rx, Duration::from_secs(5), "Taps 0 (0 ticks)").unwrap();
    assert_eq!(text, "Taps 0 (0 ticks)");
    assert_ne!(host, 0, "the root component is in the root view");
    // The timer task wakes the engine and renders again.
    let (text, _) = shown(&rx, Duration::from_secs(5), "Taps 0 (1 ticks)").unwrap();
    assert_eq!(text, "Taps 0 (1 ticks)");
    engine
        .control()
        .dispatch_event(host, EventKind::Press, EventPayload::None);
    let (text, _) = shown(&rx, Duration::from_secs(5), "Taps 1 (1 ticks)").unwrap();
    assert_eq!(text, "Taps 1 (1 ticks)");
}
