//! JS bundler for yf apps, written in Rust end to end (no Node.js).
//!
//! Pipeline per `.tsx`/`.jsx` module: Oxc strips TypeScript (keeping JSX),
//! the Solid compiler lowers JSX in universal mode against the `yf`
//! runtime module, and rolldown bundles everything into one IIFE script.
//! `solid-js` and the `yf` runtime are embedded in the binary, so apps
//! need no `node_modules` for them.

mod embedded;
mod plugin;

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use rolldown::{
    Bundler, BundlerOptions, InputItem, OutputFormat, Platform, RawMinifyOptions, SourceMapType,
    plugin::Pluginable,
};
use rolldown_common::Output;

/// What to bundle and how.
#[derive(Debug, Clone)]
pub struct BundleOptions {
    /// Entry module (usually `App.tsx`).
    pub entry: PathBuf,
    /// Minify the output (release builds).
    pub minify: bool,
}

/// A failed build, formatted for people.
#[derive(Debug, Clone)]
pub struct BundleError {
    /// With ANSI colors, for terminals.
    pub colored: String,
    /// Plain text, for on-device error screens.
    pub plain: String,
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.plain)
    }
}

impl std::error::Error for BundleError {}

impl BundleError {
    fn new(diagnostics: Vec<rolldown_error::BuildDiagnostic>) -> Self {
        let render = |color| {
            diagnostics
                .iter()
                .map(|d| d.to_diagnostic().convert_to_string(color))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Self {
            colored: render(true),
            // Plugin messages may carry their own escape codes.
            plain: strip_ansi(&render(false)),
        }
    }

    fn other(err: impl std::fmt::Display) -> Self {
        let message = err.to_string();
        Self {
            colored: message.clone(),
            plain: message,
        }
    }
}

/// Removes ANSI escape sequences (`ESC [ ... letter`).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A bundled app.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The bundle as a single IIFE script.
    pub code: String,
    /// Source map (JSON) from `code` back to the app sources.
    pub source_map: Option<String>,
    /// Asset files the app imports, relative to the entry directory
    /// (`asset:/<path>` URIs refer to them).
    pub assets: Vec<PathBuf>,
    /// Wall time spent bundling, in milliseconds.
    pub elapsed_ms: f64,
}

/// Bundles an app, blocking the current thread.
pub fn bundle(options: &BundleOptions) -> Result<Bundle, BundleError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(BundleError::other)?
        .block_on(bundle_async(options))
}

/// Bundles an app.
pub async fn bundle_async(options: &BundleOptions) -> Result<Bundle, BundleError> {
    let start = Instant::now();
    let entry = options.entry.canonicalize().map_err(|e| {
        BundleError::other(format!("entry not found: {}: {e}", options.entry.display()))
    })?;
    let cwd = entry
        .parent()
        .ok_or_else(|| BundleError::other("entry has no parent directory"))?
        .to_path_buf();
    let cwd_for_errors = cwd.clone();
    let cwd_root = cwd.clone();
    let plugin_error = Arc::new(Mutex::new(None));
    let assets = Arc::new(Mutex::new(Vec::new()));
    let mut bundler = Bundler::with_plugins(
        BundlerOptions {
            input: Some(vec![InputItem {
                import: entry.to_string_lossy().into_owned(),
                ..Default::default()
            }]),
            cwd: Some(cwd),
            // Nothing is written; this only makes source map paths relative
            // to the entry directory (`App.tsx`, not `../App.tsx`).
            dir: Some(".".into()),
            format: Some(OutputFormat::Iife),
            // Never `Node`: solid-js maps the `node` export condition to its
            // non-reactive server build.
            platform: Some(Platform::Browser),
            minify: Some(RawMinifyOptions::Bool(options.minify)),
            // Kept out of the code; used to symbolicate stack traces.
            sourcemap: Some(SourceMapType::Hidden),
            ..Default::default()
        },
        vec![Pluginable::new_shared(plugin::YfPlugin {
            root: cwd_for_errors,
            error: plugin_error.clone(),
            assets: assets.clone(),
        })],
    )
    .map_err(|e| BundleError::new(e.into_vec()))?;
    let output = bundler.generate().await.map_err(|e| {
        // Prefer our own compile error (clean code frame) over rolldown's
        // wrapped report of it.
        match plugin_error.lock().expect("error lock").take() {
            Some(message) => BundleError::other(message),
            None => BundleError::new(e.into_vec()),
        }
    })?;
    let (code, source_map) = output
        .assets
        .into_iter()
        .find_map(|asset| match asset {
            Output::Chunk(chunk) if chunk.is_entry => Some((
                chunk.code.clone(),
                chunk.map.as_ref().map(|m| m.to_json_string()),
            )),
            _ => None,
        })
        .ok_or_else(|| BundleError::other("bundle produced no entry chunk"))?;
    let mut assets: Vec<PathBuf> = std::mem::take(&mut *assets.lock().expect("assets lock"))
        .into_iter()
        .filter_map(|p| p.strip_prefix(&cwd_root).ok().map(PathBuf::from))
        .collect();
    assets.sort();
    assets.dedup();
    Ok(Bundle {
        code,
        source_map,
        assets,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}
