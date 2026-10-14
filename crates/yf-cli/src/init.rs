//! `yf init` (a new app) and the TypeScript declarations editors read
//! (`.yf/types`, refreshed by every command that builds an app).

use std::{fs, path::Path};

use anyhow::{Context as _, bail};

const APP: &str = r##"import { createSignal } from "solid-js";
import { render, root, SafeAreaView } from "yf";

function App() {
  const [count, setCount] = createSignal(0);
  return (
    <SafeAreaView style={{ flex: 1, alignItems: "center", justifyContent: "center", gap: 16 }}>
      <text style={{ fontSize: 28, fontWeight: "600" }}>{count()}</text>
      <view
        onPress={() => setCount(count() + 1)}
        activeOpacity={0.6}
        accessibilityRole="button"
        style={{ paddingHorizontal: 20, paddingVertical: 12, borderRadius: 10, backgroundColor: "#0969da" }}
      >
        <text style={{ color: "white", fontSize: 17 }}>Tap</text>
      </view>
    </SafeAreaView>
  );
}

render(() => <App />, root);
"##;

const GITIGNORE: &str = ".yf/\nout/\n";

/// Creates an app in `dir`: `App.tsx`, `yf.json`, `tsconfig.json`,
/// `.gitignore` and the type declarations. Existing files are kept.
pub(crate) fn init(dir: &Path, name: Option<String>) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let dir = dir.canonicalize()?;
    let name = name.unwrap_or_else(|| {
        dir.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("app")
            .to_owned()
    });
    let id_part: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    let config = serde_json::json!({
        "name": name,
        "id": format!("com.example.{}", if id_part.is_empty() { "app" } else { &id_part }),
        "version": "1.0.0",
        "build": 1,
    });
    let files = [
        ("App.tsx", APP.to_owned()),
        (
            "yf.json",
            format!("{}\n", serde_json::to_string_pretty(&config)?),
        ),
        ("tsconfig.json", yf_bundler::TSCONFIG.to_owned()),
        (".gitignore", GITIGNORE.to_owned()),
    ];
    if files.iter().all(|(f, _)| dir.join(f).exists()) {
        bail!("{} already has an app", dir.display());
    }
    for (file, contents) in files {
        let path = dir.join(file);
        if path.exists() {
            log::info!("kept {}", path.display());
            continue;
        }
        fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
        log::info!("created {}", path.display());
    }
    write_types(&dir);
    log::info!("run it with `yf run {}`", dir.join("App.tsx").display());
    Ok(())
}

/// Writes `.yf/types` in the app directory `dir` (files whose contents did
/// not change are left alone, so editors do not reload them).
pub(crate) fn write_types(dir: &Path) {
    let types = dir.join(".yf/types");
    for (path, contents) in yf_bundler::type_definitions() {
        let path = types.join(path);
        if fs::read(&path).is_ok_and(|old| old == contents.as_bytes()) {
            continue;
        }
        let written = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&path, contents));
        if let Err(err) = written {
            log::warn!("cannot write {}: {err}", path.display());
            return;
        }
    }
}

/// [`write_types`] for the app whose entry is `entry`.
pub(crate) fn write_types_for(entry: &Path) {
    if let Some(dir) = entry
        .canonicalize()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        write_types(&dir);
    }
}
