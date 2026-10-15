//! JS bundler for nsh apps, written in Rust end to end (no Node.js).
//!
//! Pipeline per `.tsx`/`.jsx` module: Oxc strips TypeScript (keeping JSX),
//! the Solid compiler lowers JSX in universal mode against the `nsh`
//! runtime module, and rolldown bundles everything into one IIFE script.
//! `solid-js` and the `nsh` runtime are embedded in the binary, so apps
//! need no `node_modules` for them.

mod classes;
mod embedded;
mod hot;
mod plugin;
mod types;

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
pub use types::{TSCONFIG, type_definitions};

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

/// The `"tailwind"` object of the `nsh.json` next to the entry, checked.
fn tailwind_setup(dir: &std::path::Path) -> Result<Arc<plugin::TailwindSetup>, BundleError> {
    let path = dir.join("nsh.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Arc::default());
    };
    let config: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| BundleError::other(format!("nsh.json: {e}")))?;
    let Some(tailwind) = config.get("tailwind") else {
        return Ok(Arc::default());
    };
    let json = tailwind.to_string();
    let theme = nsh_core::tailwind::Theme::from_json(&json)
        .map_err(|e| BundleError::other(format!("nsh.json: tailwind: {e}")))?;
    Ok(Arc::new(plugin::TailwindSetup {
        json: Some(json),
        theme,
    }))
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
    let tailwind = tailwind_setup(&cwd)?;
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
        vec![Pluginable::new_shared(plugin::NshPlugin {
            root: cwd_for_errors,
            error: plugin_error.clone(),
            assets: assets.clone(),
            hot: false,
            tailwind,
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

/// The hot runtime, run before every module of a development bundle.
const HOT_RUNTIME: &str = include_str!("../../../js/runtime/hot.js");

/// A development bundle with hot module replacement: the app's modules,
/// each kept separate so an update can replace just the ones that changed.
#[derive(Debug, Clone)]
pub struct HotBundle {
    modules: Vec<HotModule>,
    /// Module id of the entry.
    entry: String,
    /// Module id of solid-js (used by the hot runtime).
    solid: String,
    /// Asset files the app imports (as in [`Bundle::assets`]).
    pub assets: Vec<PathBuf>,
    /// Wall time spent bundling, in milliseconds.
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone)]
struct HotModule {
    /// Chunk file name, e.g. `App.js` (what `require` resolves to).
    id: String,
    code: String,
    map: Option<rolldown_sourcemap::SourceMap>,
    /// solid-js or the nsh runtime: never replaced in place.
    embedded: bool,
}

/// What a rebuild means for a running development bundle.
#[derive(Debug, Clone)]
pub enum HotPatch {
    /// No module changed.
    Unchanged,
    /// A script that replaces the changed modules in place; it evaluates to
    /// `false` when the update cannot be applied (the host reloads).
    Update {
        /// The script.
        code: String,
        /// Its source map (JSON).
        source_map: Option<String>,
    },
    /// The change needs a full reload.
    Reload,
}

impl HotBundle {
    /// The whole app as one script: the hot runtime, every module, and the
    /// start of the entry.
    pub fn script(&self) -> (String, Option<String>) {
        let mut joiner = rolldown_sourcemap::SourceJoiner::default();
        joiner.enable_sourcemap = true;
        joiner.append_source(HOT_RUNTIME);
        for m in &self.modules {
            let open = format!(
                "{HOT}.define({:?}, function (module, exports, require) {{",
                m.id
            );
            let close = if m.embedded { "}, true);" } else { "});" };
            append_module(&mut joiner, m, &open, close);
        }
        joiner.append_source(format!("{HOT}.start({:?}, {:?});", self.solid, self.entry));
        let (code, map) = joiner.join();
        (code, map.map(|m| m.to_json_string()))
    }

    /// The update from `previous` (what hosts run) to this bundle.
    pub fn patch(&self, previous: &HotBundle) -> HotPatch {
        if self.entry != previous.entry || self.solid != previous.solid {
            return HotPatch::Reload;
        }
        let old: std::collections::HashMap<&str, &HotModule> = previous
            .modules
            .iter()
            .map(|m| (m.id.as_str(), m))
            .collect();
        let changed: Vec<&HotModule> = self
            .modules
            .iter()
            .filter(|m| old.get(m.id.as_str()).is_none_or(|o| o.code != m.code))
            .collect();
        if changed.is_empty() {
            return HotPatch::Unchanged;
        }
        if changed.iter().any(|m| m.embedded) {
            return HotPatch::Reload;
        }
        let mut joiner = rolldown_sourcemap::SourceJoiner::default();
        joiner.enable_sourcemap = true;
        joiner.append_source(format!("{HOT}.update({{"));
        for m in changed {
            append_module(
                &mut joiner,
                m,
                &format!("{:?}: function (module, exports, require) {{", m.id),
                "},",
            );
        }
        joiner.append_source("});");
        let (code, map) = joiner.join();
        HotPatch::Update {
            code,
            source_map: map.map(|m| m.to_json_string()),
        }
    }
}

/// Name of the hot runtime global (`js/runtime/hot.js`).
const HOT: &str = "__nsh_hot";

/// Appends a module's code between `open` and `close` (each on its own
/// line, so the module's own lines keep their source map).
fn append_module(
    joiner: &mut rolldown_sourcemap::SourceJoiner<'_>,
    m: &HotModule,
    open: &str,
    close: &str,
) {
    joiner.append_source(open.to_owned());
    match &m.map {
        Some(map) => joiner.append_source(rolldown_sourcemap::SourceMapSource::new(
            m.code.clone(),
            map.clone(),
        )),
        None => joiner.append_source(m.code.clone()),
    }
    joiner.append_source(close.to_owned());
}

/// Bundles an app for development with hot module replacement, blocking
/// the current thread.
pub fn bundle_hot(entry: &std::path::Path) -> Result<HotBundle, BundleError> {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(BundleError::other)?
        .block_on(bundle_hot_async(entry))
}

/// Bundles an app for development with hot module replacement.
pub async fn bundle_hot_async(entry: &std::path::Path) -> Result<HotBundle, BundleError> {
    let start = Instant::now();
    let entry = entry
        .canonicalize()
        .map_err(|e| BundleError::other(format!("entry not found: {}: {e}", entry.display())))?;
    let cwd = entry
        .parent()
        .ok_or_else(|| BundleError::other("entry has no parent directory"))?
        .to_path_buf();
    let plugin_error = Arc::new(Mutex::new(None));
    let assets = Arc::new(Mutex::new(Vec::new()));
    let tailwind = tailwind_setup(&cwd)?;
    let mut bundler = Bundler::with_plugins(
        BundlerOptions {
            input: Some(vec![InputItem {
                import: entry.to_string_lossy().into_owned(),
                ..Default::default()
            }]),
            cwd: Some(cwd.clone()),
            dir: Some(".".into()),
            // One CommonJS chunk per module: modules can be replaced one by
            // one, and imports are read at use (`dep.name`), so a module run
            // again in place is seen by its importers.
            format: Some(OutputFormat::Cjs),
            preserve_modules: Some(true),
            // Every export stays: an edit may start using one of solid-js
            // or the runtime that the app did not use before, and those
            // modules are never replaced.
            treeshake: rolldown_common::TreeshakeOptions::Boolean(false),
            platform: Some(Platform::Browser),
            sourcemap: Some(SourceMapType::Hidden),
            ..Default::default()
        },
        vec![Pluginable::new_shared(plugin::NshPlugin {
            root: cwd.clone(),
            error: plugin_error.clone(),
            assets: assets.clone(),
            hot: true,
            tailwind,
        })],
    )
    .map_err(|e| BundleError::new(e.into_vec()))?;
    let output = bundler.generate().await.map_err(|e| {
        match plugin_error.lock().expect("error lock").take() {
            Some(message) => BundleError::other(message),
            None => BundleError::new(e.into_vec()),
        }
    })?;
    let mut modules = Vec::new();
    let (mut entry_id, mut solid) = (None, None);
    for asset in output.assets {
        let Output::Chunk(chunk) = asset else {
            continue;
        };
        let id = chunk.filename.to_string();
        let embedded = chunk
            .module_ids
            .iter()
            .any(|m| m.as_str().starts_with('\0'));
        if chunk
            .module_ids
            .iter()
            .any(|m| m.as_str() == embedded::SOLID_ID)
        {
            solid = Some(id.clone());
        }
        if chunk.is_entry {
            entry_id = Some(id.clone());
        }
        modules.push(HotModule {
            id,
            code: chunk.code.clone(),
            map: chunk.map.clone(),
            embedded,
        });
    }
    let mut assets: Vec<PathBuf> = std::mem::take(&mut *assets.lock().expect("assets lock"))
        .into_iter()
        .filter_map(|p| p.strip_prefix(&cwd).ok().map(PathBuf::from))
        .collect();
    assets.sort();
    assets.dedup();
    Ok(HotBundle {
        modules,
        entry: entry_id.ok_or_else(|| BundleError::other("bundle produced no entry chunk"))?,
        solid: solid.ok_or_else(|| BundleError::other("the bundle does not include solid-js"))?,
        assets,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}
