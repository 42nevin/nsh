//! The distribution: everything an app developer needs, prebuilt.
//!
//! ```text
//! dist/yf/bin/yf                                     the CLI
//! dist/yf/runtime/ios/iphoneos/yf-app                packaged app runner
//! dist/yf/runtime/ios/iphonesimulator/{yf-app,yf-host}
//! dist/yf/runtime/ios/maccatalyst/yf-host            development host
//! dist/yf/runtime/android/{release,dev}/             classes.dex, lib/<abi>/libyf.so
//! dist/yf/embed/ios/{YF,YFDev}.xcframework           embedding in iOS apps
//! dist/yf/embed/android/{yf,yf-dev}.aar              embedding in Android apps
//! ```

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context as _, bail};

use crate::android;

const IOS: &str = "aarch64-apple-ios";
const IOS_SIM: &str = "aarch64-apple-ios-sim";
const CATALYST: &str = "aarch64-apple-ios-macabi";

pub(crate) fn dist() -> anyhow::Result<()> {
    let root = android::root();
    let out = root.join("dist/yf");
    if out.exists() {
        fs::remove_dir_all(&out)?;
    }
    let release = root.join("target");

    // The CLI for this machine.
    cargo(&["build", "--release", "-p", "yf-cli"])?;
    copy(&release.join("release/yf"), &out.join("bin/yf"))?;

    // iOS runtimes: release runners, development hosts.
    for (triple, dir) in [(IOS, "iphoneos"), (IOS_SIM, "iphonesimulator")] {
        cargo(&["build", "--release", "-p", "yf-app", "--target", triple])?;
        copy(
            &release.join(triple).join("release/yf-app"),
            &out.join("runtime/ios").join(dir).join("yf-app"),
        )?;
    }
    for (triple, dir) in [(IOS_SIM, "iphonesimulator"), (CATALYST, "maccatalyst")] {
        cargo(&["build", "--release", "-p", "yf-host", "--target", triple])?;
        copy(
            &release.join(triple).join("release/yf-host"),
            &out.join("runtime/ios").join(dir).join("yf-host"),
        )?;
    }

    // Android runtimes.
    let abis: Vec<String> = android::ABIS.iter().map(|a| a.to_string()).collect();
    for (dev, name) in [(false, "release"), (true, "dev")] {
        let built = android::build(true, dev, &abis)?;
        let dest = out.join("runtime/android").join(name);
        copy(&built.join("classes.dex"), &dest.join("classes.dex"))?;
        for abi in &abis {
            copy(
                &built.join("lib").join(abi).join("libyf.so"),
                &dest.join("lib").join(abi).join("libyf.so"),
            )?;
        }
        let aar = out
            .join("embed/android")
            .join(if dev { "yf-dev.aar" } else { "yf.aar" });
        make_aar(&built, &abis, dev, &aar)?;
    }

    // iOS embedding: one static library per platform in an XCFramework.
    for (dev, name) in [(false, "YF"), (true, "YFDev")] {
        let mut libs = Vec::new();
        for triple in [IOS, IOS_SIM, CATALYST] {
            let mut args = vec!["build", "--release", "-p", "yf-ffi", "--target", triple];
            if dev {
                args.extend(["--features", "dev"]);
            }
            cargo(&args)?;
            // Separate copies: dev and release builds share the target dir.
            let lib = out.join("work").join(name).join(triple).join("libyf.a");
            copy(&release.join(triple).join("release/libyf.a"), &lib)?;
            libs.push(lib);
        }
        let framework = out.join("embed/ios").join(format!("{name}.xcframework"));
        let headers = root.join("crates/yf-ffi/include");
        let mut cmd = Command::new("xcodebuild");
        cmd.arg("-create-xcframework");
        for lib in &libs {
            cmd.arg("-library").arg(lib).arg("-headers").arg(&headers);
        }
        cmd.arg("-output").arg(&framework);
        run(&mut cmd)?;
    }
    fs::remove_dir_all(out.join("work"))?;
    println!("{}", out.display());
    Ok(())
}

/// An Android library archive: the Java runtime and `libyf.so` per ABI.
///
/// The development library allows cleartext HTTP (merged into the app's
/// manifest): it loads assets from the `yf dev` server over plain HTTP.
fn make_aar(built: &Path, abis: &[String], dev: bool, aar: &Path) -> anyhow::Result<()> {
    let work = aar.with_extension("aar-work");
    if work.exists() {
        fs::remove_dir_all(&work)?;
    }
    fs::create_dir_all(&work)?;
    fs::write(
        work.join("AndroidManifest.xml"),
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dev.yf">
    <uses-sdk android:minSdkVersion="{}" />
    <uses-permission android:name="android.permission.INTERNET" />{}
</manifest>
"#,
            android::MIN_SDK,
            if dev {
                "\n    <application android:usesCleartextTraffic=\"true\" />"
            } else {
                ""
            }
        ),
    )?;
    fs::write(work.join("R.txt"), "")?;
    copy(&built.join("classes.jar"), &work.join("classes.jar"))?;
    for abi in abis {
        copy(
            &built.join("lib").join(abi).join("libyf.so"),
            &work.join("jni").join(abi).join("libyf.so"),
        )?;
    }
    if let Some(dir) = aar.parent() {
        fs::create_dir_all(dir)?;
    }
    let _ = fs::remove_file(aar);
    run(Command::new("zip")
        .current_dir(&work)
        .args(["-q", "-r"])
        .arg(aar.canonicalize().unwrap_or_else(|_| abs(aar)))
        .arg("."))?;
    fs::remove_dir_all(&work)?;
    Ok(())
}

fn abs(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(p)
    }
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    run(Command::new(cargo).current_dir(android::root()).args(args))
}

fn copy(from: &Path, to: &Path) -> anyhow::Result<()> {
    if let Some(dir) = to.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::copy(from, to).with_context(|| format!("copy {} -> {}", from.display(), to.display()))?;
    Ok(())
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("run {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{:?} failed", cmd.get_program());
    }
    Ok(())
}
