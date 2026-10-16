//! Stopping an engine never waits for a long-running script: platforms drop
//! engines on their main thread.

use std::time::{Duration, Instant};

use nsh_core::{ApproxTextEngine, MountBatch, Size};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};

struct NullSink;

impl MountSink for NullSink {
    fn submit(&self, _batch: MountBatch) {}
}

#[test]
fn dropping_an_engine_aborts_running_js() {
    let config = EngineConfig {
        app: nsh_engine::App::Js(Bundle::source(
            "let n = 0; while (true) { n++; }".to_owned(),
        )),
        viewport: Size::new(390.0, 844.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
        ui_sets: Vec::new(),
        runtime: None,
    };
    let engine = Engine::start(config, || ApproxTextEngine, NullSink).unwrap();
    // Let the script start spinning.
    std::thread::sleep(Duration::from_millis(200));
    let start = Instant::now();
    drop(engine);
    assert!(
        start.elapsed() < Duration::from_millis(500),
        "{:?}",
        start.elapsed()
    );
}
