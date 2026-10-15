//! Registered native components end to end: `nsh/native`'s props reach the
//! platform as JSON, events come back to the handlers, commands go out.

use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};
use nsh_core::{
    ApproxTextEngine, Command, EventKind, EventPayload, JsonValue, MountBatch, MountItem, Size,
    ViewKind,
};
use nsh_engine::{Bundle, Engine, EngineConfig, MountSink};
use serde_json::json;

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

const APP: &str = r#"import { createSignal } from "solid-js";
import { render, root, type YNode } from "nsh";
import { nativeComponent } from "nsh/native";

const Map = nativeComponent<
  { center: { lat: number; lng: number }; zoom?: number; title?: string },
  { regionChange: { zoom: number } }
>("com.example.map", { props: ["center", "zoom", "title"], events: ["regionChange"] });

function App() {
  const [zoom, setZoom] = createSignal(3);
  let map: YNode | undefined;
  return (
    <view>
      <Map
        ref={(n) => (map = n)}
        center={{ lat: 1.5, lng: 2 }}
        zoom={zoom()}
        style={{ height: 100 }}
        onRegionChange={(e) => {
          setZoom(e.zoom);
          map?.command("flyTo", { zoom: e.zoom, animated: true });
        }}
      />
      <text>{`zoom ${zoom()}`}</text>
    </view>
  );
}

render(() => <App />, root);
"#;

fn start() -> (Engine, Receiver<MountBatch>) {
    let dir = std::env::temp_dir().join(format!("nsh-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let entry = dir.join("App.tsx");
    std::fs::write(&entry, APP).unwrap();
    let bundle = nsh_bundler::bundle(&nsh_bundler::BundleOptions {
        entry,
        minify: false,
    })
    .unwrap();
    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        bundle: Bundle::source(bundle.code),
        viewport: Size::new(320.0, 480.0),
        trace: false,
        network: None,
        assets: None,
        environment: Default::default(),
        header_heights: (44.0, 96.0),
        limits: nsh_engine::Limits::default(),
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
fn props_events_and_commands_cross_as_json() {
    let (engine, rx) = start();
    let first = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("first frame");
    let id = first
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Create {
                id,
                kind: ViewKind::Native,
            } => Some(*id),
            _ => None,
        })
        .expect("a native container");
    assert!(first.items.iter().any(
        |i| matches!(i, MountItem::Component { id: c, name } if *c == id && &**name == "com.example.map")
    ));
    let props: Vec<(String, Option<JsonValue>)> = first
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::ComponentProps { id: c, props } if *c == id => Some(props.clone()),
            _ => None,
        })
        .flatten()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
    assert!(props.contains(&("center".into(), Some(json!({ "lat": 1.5, "lng": 2 })))));
    assert!(props.contains(&("zoom".into(), Some(json!(3)))));
    // Unset props are not sent.
    assert!(!props.iter().any(|(k, _)| k == "title"));
    assert!(texts(&first).contains(&"zoom 3".to_owned()));

    // The component emits; the handler re-renders and calls a command.
    engine.control().dispatch_event(
        id,
        EventKind::NativeEvent,
        EventPayload::Native {
            name: "regionChange".into(),
            data: json!({ "zoom": 7 }),
        },
    );
    let batch = rx.recv_timeout(Duration::from_secs(5)).expect("a batch");
    assert!(texts(&batch).contains(&"zoom 7".to_owned()));
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::ComponentProps { id: c, props } if *c == id && props == &[("zoom".into(), Some(json!(7)))]
    )));
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Command { id: c, command: Command::Native { name, args } }
            if *c == id && &**name == "flyTo" && *args == json!({ "zoom": 7, "animated": true })
    )));
}
