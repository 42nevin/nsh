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
        // SAFETY: compiled just now by this runtime.
        bundle: unsafe { Bundle::bytecode(nsh_js::compile_bytecode(&bundle.code, false).unwrap()) },
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

/// Redirects `/hop/<url>` to `<url>`; answers everything else.
struct RedirectNetwork {
    seen: Arc<Mutex<Vec<String>>>,
}

impl Network for RedirectNetwork {
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch {
        self.seen.lock().unwrap().push(request.url.clone());
        assert!(
            !request.follow_redirects,
            "the engine follows redirects itself"
        );
        let response = match request.url.split_once("/hop/") {
            Some((_, to)) => HttpResponse {
                status: 302,
                url: request.url.clone(),
                headers: vec![("location".into(), to.into())],
                ..HttpResponse::default()
            },
            None => HttpResponse {
                status: 200,
                url: request.url.clone(),
                body: b"ok".to_vec(),
                ..HttpResponse::default()
            },
        };
        std::thread::spawn(move || done(Ok(response)));
        Box::new(|| {})
    }
}

const CHECKED_APP: &str = r#"
import { createSignal } from "solid-js";
import { render, root } from "nsh";
const [text, setText] = createSignal("loading");
const outcome = (p) => p.then((r) => r.text().then((t) => `${t}${r.redirected ? " (redirected)" : ""}`), (e) => e.message);
Promise.all([
  outcome(fetch("https://evil.io/")),
  outcome(fetch("https://api.example.com/hop/https://api.example.com/final")),
  outcome(fetch("https://api.example.com/hop/https://evil.io/steal")),
]).then((r) => setText(r.join(" | ")));
render(() => <text>{text()}</text>, root);
"#;

#[test]
fn fetch_stays_within_the_capabilities() {
    let dir = std::env::temp_dir().join(format!("nsh-fetch-caps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, CHECKED_APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
    })
    .unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = mpsc::channel();
    let config = EngineConfig {
        // SAFETY: compiled just now by this runtime.
        bundle: unsafe { Bundle::bytecode(nsh_js::compile_bytecode(&bundle.code, false).unwrap()) }
            .with_capabilities_json(Some(r#"{ "network": { "hosts": ["api.example.com"] } }"#))
            .unwrap(),
        viewport: Size::new(320.0, 480.0),
        trace: false,
        network: Some(Arc::new(RedirectNetwork { seen: seen.clone() })),
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
    let result = texts(&rx.recv_timeout(Duration::from_secs(5)).unwrap()).join("");
    let parts: Vec<&str> = result.split(" | ").collect();
    assert!(
        parts[0].contains("`evil.io` to capabilities.network"),
        "{result}"
    );
    assert_eq!(parts[1], "ok (redirected)");
    assert!(
        parts[2].contains("https://evil.io/steal is not granted"),
        "{result}"
    );
    // evil.io was never contacted.
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .all(|u| !u.starts_with("https://evil.io"))
    );
}
