//! Locates the prebuilt runtime: native executables and libraries that
//! apps are packaged with, so building an app never needs Rust.
//!
//! Distribution layout (`cargo xtask dist`), next to `bin/nsh`:
//!
//! ```text
//! runtime/ios/iphoneos/nsh-ios-app           release runner (device)
//! runtime/ios/iphonesimulator/nsh-ios-app    release runner (simulator)
//! runtime/ios/iphonesimulator/nsh-ios-host   development host (simulator)
//! runtime/ios/maccatalyst/nsh-ios-host       development host (Mac Catalyst)
//! runtime/android/release/             classes.dex + lib/<abi>/libnsh.so
//! runtime/android/dev/                 the same with development support
//! ```
//!
//! In a cargo checkout, the same files come from `target/<triple>/<profile>`
//! (with the profile of this CLI), and are built with cargo first, so a run
//! never uses a missing or stale host. `NSH_RUNTIME` overrides the root.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

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
    if let Some(root) = checkout(bin) {
        let mut args = vec!["xtask", "android"];
        if profile == "release" {
            args.push("--release");
        }
        if dev {
            args.push("--dev");
        }
        cargo(&root, &args)?;
    }
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

/// Path of runtime file `name` (`nsh-ios-app`, `nsh-ios-host`) for `platform`.
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
    if let Some(root) = checkout(bin) {
        let mut args = vec!["build", "-p", name, "--target", platform.triple()];
        match profile.to_str() {
            Some("debug") => {}
            Some("release") => args.push("--release"),
            Some(other) => args.extend(["--profile", other]),
            None => bail!("cli profile dir is not UTF-8"),
        }
        cargo(&root, &args)?;
    }
    let built = bin
        .parent()
        .context("cli is not in a target directory")?
        .join(platform.triple())
        .join(profile)
        .join(name);
    existing(built, platform, name)
}

/// The workspace of a cargo checkout of nsh this CLI was built in
/// (`<root>/target/<profile>/nsh`), if it is one.
fn checkout(bin: &Path) -> Option<PathBuf> {
    let root = bin.parent()?.parent()?;
    root.join("crates/nsh-ios-host/Cargo.toml")
        .exists()
        .then(|| root.to_owned())
}

/// Runs cargo in the checkout `root` (the one that runs this CLI under
/// `cargo run`, else `cargo` from the path).
fn cargo(root: &Path, args: &[&str]) -> anyhow::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(&cargo)
        .args(args)
        .current_dir(root)
        .status()
        .context("run cargo")?;
    if !status.success() {
        bail!("cargo {} failed", args.join(" "));
    }
    Ok(())
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
