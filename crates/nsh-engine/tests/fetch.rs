//! `fetch` through the engine: requests reach the platform network on its
//! own threads, responses come back to JS, aborted requests are cancelled.

use std::{
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use nsh_core::{ApproxTextEngine, MountBatch, MountItem, Size};
use nsh_engine::{
    Bundle, CancelFetch, Engine, EngineConfig, FetchDone, HttpRequest, HttpResponse, MountSink,
    Network,
};

struct Sink(mpsc::Sender<MountBatch>);

impl MountSink for Sink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// Answers on a background thread, echoing the request.
#[derive(Default)]
struct EchoNetwork {
    cancelled: Arc<Mutex<Vec<String>>>,
}

impl Network for EchoNetwork {
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch {
        let url = request.url.clone();
        if url.ends_with("/slow") {
            let cancelled = self.cancelled.clone();
            return Box::new(move || cancelled.lock().unwrap().push(url));
        }
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            let body = format!(
                "{} {} {}",
                request.method,
                request.url,
                String::from_utf8_lossy(&request.body)
            );
            done(Ok(HttpResponse {
                status: 200,
                url: request.url,
                headers: vec![("content-type".into(), "text/plain".into())],
                body: body.into_bytes(),
                ..HttpResponse::default()
            }));
        });
        Box::new(|| {})
    }
}

const APP: &str = r#"
import { createSignal } from "solid-js";
import { render, root } from "nsh";
const [text, setText] = createSignal("loading");
const slow = new AbortController();
fetch("https://example.com/slow", { signal: slow.signal }).catch((e) => console.log(e.name));
fetch("https://example.com/echo", { method: "PUT", body: "hi" })
  .then((r) => r.text())
  .then((t) => { setText(t); slow.abort(); });
render(() => <text>{text()}</text>, root);
"#;

fn texts(batch: &MountBatch) -> Vec<String> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.text.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn fetch_round_trips_through_the_platform_network() {
    let dir = std::env::temp_dir().join(format!("nsh-fetch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
    })
    .unwrap();
    let network = Arc::new(EchoNetwork::default());
    let cancelled = network.cancelled.clone();
    let (tx, rx) = mpsc::channel();
    let config = EngineConfig {
        bundle: Bundle::bytecode(nsh_js::compile_bytecode(&bundle.code, false).unwrap()),
        viewport: Size::new(320.0, 480.0),
        trace: false,
        network: Some(network),
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
    };
    let _engine = Engine::start(config, || ApproxTextEngine, Sink(tx)).unwrap();
    assert_eq!(
        texts(&rx.recv_timeout(Duration::from_secs(5)).unwrap()),
        ["loading"]
    );
    assert_eq!(
        texts(&rx.recv_timeout(Duration::from_secs(5)).unwrap()),
        ["PUT https://example.com/echo hi"]
    );
    // The abort reaches the network once the engine pumps it.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while cancelled.lock().unwrap().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "slow request not cancelled"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(*cancelled.lock().unwrap(), ["https://example.com/slow"]);
}
