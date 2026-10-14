//! Development host: a prebuilt app that runs any yf app.
//!
//! - `yf-host --dev-server ws://127.0.0.1:8790/ws [--trace]` loads bundles
//!   pushed by `yf dev` / `yf run` and reloads on every rebuild.
//! - `yf-host --bundle <path> [--trace]` runs a built bundle (`.qbc` is
//!   QuickJS bytecode, anything else JS source).
//!
//! Without arguments it connects to the dev server on this machine.

#[cfg(target_os = "ios")]
fn main() {
    use std::{env, path::PathBuf};

    use yf_platform_apple::{AppOptions, AppSource, AssetBase, Bundle, dev, run_app};

    dev::init_logging();
    let mut source = None;
    let mut trace = false;
    let mut asset_base = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dev-server" => source = args.next().map(AppSource::DevServer),
            "--bundle" => {
                let path = PathBuf::from(args.next().unwrap_or_default());
                let bundle = match Bundle::read(&path) {
                    Ok(bundle) => bundle,
                    Err(err) => {
                        log::error!("cannot read bundle {}: {err}", path.display());
                        std::process::exit(1);
                    }
                };
                // `yf build` copies imported assets next to the bundle.
                asset_base = path
                    .parent()
                    .map(|dir| AssetBase::Directory(dir.join("assets")));
                source = Some(AppSource::Bundle(bundle));
            }
            "--trace" => trace = true,
            // Foundation reads `-Key value` pairs as user defaults.
            other if other.starts_with('-') && !other.starts_with("--") => {
                args.next();
            }
            other => log::warn!("unknown argument {other}"),
        }
    }
    let source = source.unwrap_or_else(|| {
        use yf_dev::protocol::{DEFAULT_PORT, WS_PATH};
        AppSource::DevServer(format!("ws://127.0.0.1:{DEFAULT_PORT}{WS_PATH}"))
    });
    run_app(source, AppOptions { trace, asset_base });
}

#[cfg(not(target_os = "ios"))]
fn main() {
    eprintln!("yf-host runs on UIKit targets; build it with --target aarch64-apple-ios-macabi");
    std::process::exit(1);
}
