//! Locates the prebuilt runtime: native executables and libraries that
//! apps are packaged with, so building an app never needs Rust.
//!
//! Distribution layout (`cargo xtask dist`), next to `bin/nsh`:
//!
//! ```text
//! runtime/ios/iphoneos/nsh-app          release runner (device)
//! runtime/ios/iphonesimulator/nsh-app   release runner (simulator)
//! runtime/ios/iphonesimulator/nsh-host  development host (simulator)
//! runtime/ios/maccatalyst/nsh-host      development host (Mac Catalyst)
//! runtime/android/release/             classes.dex + lib/<abi>/libnsh.so
//! runtime/android/dev/                 the same with development support
//! ```
//!
//! In a cargo checkout, the same files come from `target/<triple>/<profile>`
//! (built with the profile of this CLI). `NSH_RUNTIME` overrides the root.

use std::{env, path::PathBuf};

use anyhow::{Context as _, bail};

/// A runtime platform directory and its cargo target triple.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Platform {
    /// iOS devices.
    IphoneOs,
    /// The iOS simulator.
    IphoneSimulator,
    /// Mac Catalyst.
    MacCatalyst,
}

impl Platform {
    fn dir(self) -> &'static str {
        match self {
            Platform::IphoneOs => "ios/iphoneos",
            Platform::IphoneSimulator => "ios/iphonesimulator",
            Platform::MacCatalyst => "ios/maccatalyst",
        }
    }

    pub(crate) fn triple(self) -> &'static str {
        match self {
            Platform::IphoneOs => "aarch64-apple-ios",
            Platform::IphoneSimulator => "aarch64-apple-ios-sim",
            Platform::MacCatalyst => "aarch64-apple-ios-macabi",
        }
    }
}

/// The Android runtime directory (`classes.dex`, `lib/<abi>/libnsh.so`),
/// with development support or not.
pub(crate) fn android(dev: bool) -> anyhow::Result<PathBuf> {
    let variant = if dev { "dev" } else { "release" };
    if let Some(root) = env::var_os("NSH_RUNTIME") {
        return android_existing(PathBuf::from(root).join("android").join(variant), dev);
    }
    let exe = env::current_exe()?;
    let bin = exe.parent().context("cli has no parent directory")?;
    let dist = bin.join("../runtime/android").join(variant);
    if dist.join("classes.dex").exists() {
        return Ok(dist);
    }
    // Cargo checkout: `cargo xtask android [--release] [--dev]` output.
    let profile = bin
        .file_name()
        .context("cli profile dir")?
        .to_string_lossy();
    let name = if dev {
        format!("{profile}-dev")
    } else {
        profile.into_owned()
    };
    let built = bin
        .parent()
        .context("cli is not in a target directory")?
        .join("android")
        .join(name);
    android_existing(built, dev)
}

fn android_existing(dir: PathBuf, dev: bool) -> anyhow::Result<PathBuf> {
    if dir.join("classes.dex").exists() {
        return Ok(dir);
    }
    bail!(
        "the Android runtime is not at {}\nin a nsh checkout, build it with: cargo xtask android{}",
        dir.display(),
        if dev { " --dev" } else { "" }
    )
}

/// Path of runtime file `name` (`nsh-app`, `nsh-host`) for `platform`.
pub(crate) fn file(platform: Platform, name: &str) -> anyhow::Result<PathBuf> {
    if let Some(root) = env::var_os("NSH_RUNTIME") {
        return existing(
            PathBuf::from(root).join(platform.dir()).join(name),
            platform,
            name,
        );
    }
    let exe = env::current_exe()?;
    let bin = exe.parent().context("cli has no parent directory")?;
    // Distribution: bin/nsh + runtime/.
    let dist = bin.join("../runtime").join(platform.dir()).join(name);
    if dist.exists() {
        return Ok(dist);
    }
    // Cargo checkout: target/<profile>/nsh -> target/<triple>/<profile>/<name>.
    let profile = bin.file_name().context("cli profile dir")?;
    let built = bin
        .parent()
        .context("cli is not in a target directory")?
        .join(platform.triple())
        .join(profile)
        .join(name);
    existing(built, platform, name)
}

fn existing(path: PathBuf, platform: Platform, name: &str) -> anyhow::Result<PathBuf> {
    if path.exists() {
        return Ok(path);
    }
    bail!(
        "{name} for {} not found at {}\nin a nsh checkout, build it with: cargo build -p {name} --target {}",
        platform.dir(),
        path.display(),
        platform.triple()
    )
}
