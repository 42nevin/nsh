use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};

use crate::dev::DevSession;

const HOST_TARGET: &str = "aarch64-apple-ios-macabi";
const APP_NAME: &str = "YfDev";

/// Starts a dev session and runs the app in the Mac Catalyst development
/// host, which loads bundles from the dev server and reloads on change.
pub(crate) fn run(entry: &Path, port: u16, release: bool, trace: bool) -> anyhow::Result<()> {
    let session = DevSession::start(entry, port, release)?;
    session.print_urls();
    let work = entry
        .canonicalize()?
        .parent()
        .context("entry has no directory")?
        .join(".yf");
    let app = package(&work, &find_host()?)?;
    let exe = app.join("Contents/MacOS/yf-host");
    let mut cmd = Command::new(&exe);
    cmd.arg("--dev-server").arg(session.local_url());
    if trace {
        cmd.arg("--trace");
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("launch {}", exe.display()))?;
    let result = session.watch(|| match child.try_wait()? {
        Some(status) if status.success() => Ok(true),
        Some(status) => bail!("host exited with {status}"),
        None => Ok(false),
    });
    let _ = child.kill();
    result
}

/// Locates the prebuilt host binary.
///
/// `YF_HOST` wins; otherwise the host built next to this CLI in a cargo
/// target directory (`target/<profile>/yf` -> `target/<triple>/<profile>/yf-host`).
fn find_host() -> anyhow::Result<PathBuf> {
    if let Some(path) = env::var_os("YF_HOST") {
        return Ok(PathBuf::from(path));
    }
    let exe = env::current_exe()?;
    let profile_dir = exe.parent().context("cli has no parent directory")?;
    let profile = profile_dir.file_name().context("cli profile dir")?;
    let candidate = profile_dir
        .parent()
        .context("cli is not in a target directory")?
        .join(HOST_TARGET)
        .join(profile)
        .join("yf-host");
    if candidate.exists() {
        Ok(candidate)
    } else {
        bail!(
            "development host not found at {}\nbuild it with: cargo build -p yf-host --target {HOST_TARGET}",
            candidate.display()
        )
    }
}

/// Creates (or refreshes) `<work>/YfDev.app` around the host binary.
fn package(work: &Path, host: &Path) -> anyhow::Result<PathBuf> {
    let app = work.join(format!("{APP_NAME}.app"));
    let contents = app.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)?;
    fs::write(contents.join("Info.plist"), INFO_PLIST)?;
    let dest = macos.join("yf-host");
    if is_stale(host, &dest)? {
        // Copy then rename: overwriting in place would modify the file a
        // running host (another session) maps, and the kernel kills
        // processes whose code no longer matches its cached signature.
        let tmp = macos.join(".yf-host.tmp");
        fs::copy(host, &tmp).with_context(|| format!("copy {}", host.display()))?;
        fs::rename(&tmp, &dest)?;
    }
    Ok(app)
}

fn is_stale(src: &Path, dest: &Path) -> anyhow::Result<bool> {
    let Ok(dest_meta) = fs::metadata(dest) else {
        return Ok(true);
    };
    let src_meta = fs::metadata(src)?;
    Ok(src_meta.len() != dest_meta.len() || src_meta.modified()? > dest_meta.modified()?)
}

const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>yf-host</string>
    <key>CFBundleIdentifier</key>
    <string>dev.yf.host</string>
    <key>CFBundleName</key>
    <string>YfDev</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>UIDeviceFamily</key>
    <array>
        <!-- 6 = Mac idiom ("Optimize for Mac"): 1:1 points instead of the 77% iPad scaling. -->
        <integer>6</integer>
    </array>
    <key>UIApplicationSupportsMultipleScenes</key>
    <false/>
</dict>
</plist>
"#;
