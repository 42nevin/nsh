use std::{fs, path::Path, time::Instant};

use anyhow::Context as _;
use yf_bundler::{BundleError, BundleOptions};

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

impl From<yf_js::JsError> for BuildError {
    fn from(err: yf_js::JsError) -> Self {
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
}

/// Bundles `entry` and compiles it to bytecode.
pub(crate) fn compile(entry: &Path, release: bool) -> Result<Built, BuildError> {
    let bundle = yf_bundler::bundle(&BundleOptions {
        entry: entry.to_path_buf(),
        minify: release,
    })?;
    let start = Instant::now();
    let bytecode = yf_js::compile_bytecode(&bundle.code, release)?;
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
    })
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
