use std::{path::Path, time::Duration};

use anyhow::{Context as _, bail};
use crossbeam_channel::{Receiver, Sender};
use yf_bundler::BundleOptions;
use yf_core::{ApproxTextEngine, EventKind, MountBatch, MountItem, Size};
use yf_engine::{Bundle, Engine, EngineConfig, MountSink};

struct ChannelSink(Sender<MountBatch>);

impl MountSink for ChannelSink {
    fn submit(&self, batch: MountBatch) {
        let _ = self.0.send(batch);
    }
}

/// Runs the app headless, taps the first pressable view `taps` times and
/// prints every mount batch.
pub(crate) fn trace(
    entry: &Path,
    taps: u32,
    idle_ms: u64,
    scroll: Option<f32>,
    width: f32,
    height: f32,
) -> anyhow::Result<()> {
    let bundle = yf_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: false,
    })
    .map_err(|e| anyhow::anyhow!("{}", e.colored))?;
    eprintln!(
        "bundled in {:.1} ms ({} B)",
        bundle.elapsed_ms,
        bundle.code.len()
    );
    let bytecode = yf_js::compile_bytecode(&bundle.code, false)?;

    let (tx, rx) = crossbeam_channel::unbounded();
    let config = EngineConfig {
        bundle: match bundle.source_map {
            Some(map) => Bundle::bytecode(bytecode).with_source_map(map),
            None => Bundle::bytecode(bytecode),
        },
        viewport: Size::new(width, height),
        trace: true,
    };
    let engine = Engine::start(config, || ApproxTextEngine, ChannelSink(tx))?;

    let first = next(&rx).context("the app rendered nothing")?;
    print_batch("initial render", &first);
    let pressable = first.items.iter().find_map(|item| match item {
        MountItem::Events { id, events } if events.contains(EventKind::Press) => Some(*id),
        _ => None,
    });
    let scroll_view = first.items.iter().find_map(|item| match item {
        MountItem::Create {
            id,
            kind: yf_core::ViewKind::Scroll,
        } => Some(*id),
        _ => None,
    });
    if let (Some(distance), Some(id)) = (scroll, scroll_view) {
        let mut y = 0.0;
        while y < distance {
            y += 10.0;
            engine.control().scroll(id, 0.0, y);
            // Frames that need nothing produce no batch.
            if let Ok(batch) = rx.recv_timeout(Duration::from_millis(50)) {
                print_batch(&format!("scroll to {y}"), &batch);
            }
        }
    }
    // Batches emitted while idling (e.g. releases after GC) are printed too.
    while let Ok(batch) = rx.recv_timeout(Duration::from_millis(idle_ms)) {
        print_batch("while idle", &batch);
    }
    match pressable {
        Some(id) => {
            for i in 1..=taps {
                engine
                    .control()
                    .dispatch_event(id, EventKind::Press, yf_core::EventPayload::None);
                match next(&rx) {
                    Some(batch) => print_batch(&format!("tap #{i} on view {id}"), &batch),
                    None => bail!("tap #{i} produced no update"),
                }
            }
        }
        None if taps > 0 => eprintln!("no pressable view found"),
        None => {}
    }
    Ok(())
}

fn next(rx: &Receiver<MountBatch>) -> Option<MountBatch> {
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

fn print_batch(title: &str, batch: &MountBatch) {
    println!("── {title}: {} items", batch.items.len());
    for item in &batch.items {
        match item {
            MountItem::Text {
                id, text, content, ..
            } => println!(
                "   Text {{ id: {id}, text: {:?}, content: {}x{} @ {},{} }}",
                text.text, content.width, content.height, content.x, content.y
            ),
            MountItem::Frame { id, frame } => println!(
                "   Frame {{ id: {id}, x: {}, y: {}, w: {}, h: {} }}",
                frame.x, frame.y, frame.width, frame.height
            ),
            other => println!("   {other:?}"),
        }
    }
}
