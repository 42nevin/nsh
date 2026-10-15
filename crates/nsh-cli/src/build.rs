use std::{fs, path::Path, time::Instant};

use anyhow::Context as _;
use nsh_bundler::{BundleError, BundleOptions};

/// A failed build, formatted for terminals and for device error screens.
#[derive(Debug)]
pub(crate) struct BuildError {
    pub(crate) colored: String,
    pub(crate) plain: String,
}

impl From<BundleError> for BuildError {
    fn from(err: BundleError) -> Self {
        Self {
            colored: err.colored,
            plain: err.plain,
        }
    }
}

impl From<nsh_js::JsError> for BuildError {
    fn from(err: nsh_js::JsError) -> Self {
        Self {
            colored: err.message.clone(),
            plain: err.message,
        }
    }
}

/// A compiled app, in memory.
pub(crate) struct Built {
    /// The bundle as JS source.
    pub(crate) code: String,
    /// The bundle as QuickJS bytecode.
    pub(crate) bytecode: Vec<u8>,
    /// Source map of `code`.
    pub(crate) source_map: Option<String>,
    /// Imported asset files, relative to the entry directory.
    pub(crate) assets: Vec<std::path::PathBuf>,
    /// What the app declares it may reach (`nsh.json` `capabilities`).
    pub(crate) capabilities: Option<nsh_core::access::Capabilities>,
}

impl Built {
    /// `capabilities.json` (written next to the bundle), when the app
    /// declares capabilities.
    pub(crate) fn capabilities_json(&self) -> Option<String> {
        self.capabilities.as_ref().map(|c| c.to_json().to_string())
    }
}

/// Bundles `entry` and compiles it to bytecode.
pub(crate) fn compile(entry: &Path, release: bool) -> Result<Built, BuildError> {
    let bundle = nsh_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: release,
    })?;
    let start = Instant::now();
    let bytecode = nsh_js::compile_bytecode(&bundle.code, release)?;
    log::info!(
        "built {} ({} B js in {:.1} ms, {} B bytecode in {:.1} ms)",
        entry.display(),
        bundle.code.len(),
        bundle.elapsed_ms,
        bytecode.len(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    Ok(Built {
        code: bundle.code,
        bytecode,
        source_map: bundle.source_map,
        assets: bundle.assets,
        capabilities: bundle.capabilities,
    })
}

/// Bundles `entry` for development with hot module replacement: the whole
/// app (source and bytecode) plus the modules, to compute later updates.
pub(crate) fn compile_hot(entry: &Path) -> Result<(Built, nsh_bundler::HotBundle), BuildError> {
    let hot = nsh_bundler::bundle_hot(entry)?;
    let (code, source_map) = hot.script();
    let start = Instant::now();
    let bytecode = nsh_js::compile_bytecode(&code, false)?;
    log::info!(
        "built {} ({} B js in {:.1} ms, {} B bytecode in {:.1} ms)",
        entry.display(),
        code.len(),
        hot.elapsed_ms,
        bytecode.len(),
        start.elapsed().as_secs_f64() * 1000.0,
    );
    let built = Built {
        code,
        bytecode,
        source_map,
        assets: hot.assets.clone(),
        capabilities: hot.capabilities.clone(),
    };
    Ok((built, hot))
}

/// Bundles `entry` into `out/bundle.js`, `out/bundle.js.map` and
/// `out/bundle.qbc`.
pub(crate) fn build(entry: &Path, out: &Path, release: bool) -> anyhow::Result<()> {
    let built = compile(entry, release).map_err(|e| anyhow::anyhow!("{}", e.colored))?;
    fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))?;
    let map = built.source_map.as_deref().unwrap_or_default();
    for (name, bytes) in [
        ("bundle.js", built.code.as_bytes()),
        ("bundle.js.map", map.as_bytes()),
        ("bundle.qbc", &built.bytecode),
    ] {
        let path = out.join(name);
        fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    }
    let declared = out.join("capabilities.json");
    match built.capabilities_json() {
        Some(json) => {
            fs::write(&declared, json).with_context(|| format!("write {}", declared.display()))?
        }
        None => {
            let _ = fs::remove_file(&declared);
        }
    }
    // Assets keep their relative paths: `asset:/img/a.png` -> `out/assets/img/a.png`.
    let root = entry
        .canonicalize()?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    for asset in &built.assets {
        let dest = out.join("assets").join(asset);
        if let Some(dir) = dest.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::copy(root.join(asset), &dest).with_context(|| format!("copy {}", asset.display()))?;
    }
    log::info!("wrote {} ({} assets)", out.display(), built.assets.len());
    Ok(())
}
