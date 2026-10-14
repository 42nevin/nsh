//! The `yf` command line tool.

mod android;
mod build;
mod dev;
mod init;
mod logger;
mod net;
mod package;
mod run;
mod runtime;
mod trace;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "yf", version, about = "Build and run yf apps")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create an app: `App.tsx`, `yf.json`, `tsconfig.json` and the type
    /// declarations editors use (`.yf/types`, kept up to date by the other
    /// commands).
    Init {
        /// Directory of the app (created if missing).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Display name (default: the directory's name).
        #[arg(long)]
        name: Option<String>,
    },
    /// Bundle an app and compile it to QuickJS bytecode.
    Build {
        /// Entry module, e.g. `App.tsx`.
        entry: PathBuf,
        /// Output directory.
        #[arg(short, long, default_value = "out")]
        out: PathBuf,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
    },
    /// Run an app headless and print the mount instructions and timings.
    Trace {
        /// Entry module, e.g. `App.tsx`.
        entry: PathBuf,
        /// Number of taps to send to the first pressable view.
        #[arg(long, default_value_t = 2)]
        taps: u32,
        /// Idle time before tapping, in milliseconds. The default lets the
        /// engine's idle GC run first, so retention bugs show up.
        #[arg(long, default_value_t = 1200)]
        idle_ms: u64,
        /// Scroll the first scroll view / list by this many points, in
        /// 10 pt frames, and print each frame's batch.
        #[arg(long)]
        scroll: Option<f32>,
        /// Viewport width in points.
        #[arg(long, default_value_t = 390.0)]
        width: f32,
        /// Viewport height in points.
        #[arg(long, default_value_t = 844.0)]
        height: f32,
    },
    /// Serve an app to dev hosts (simulators, devices) and push rebuilds.
    Dev {
        /// Entry module, e.g. `App.tsx`.
        entry: PathBuf,
        /// Dev server port (the next free one is used if it is taken).
        #[arg(long, default_value_t = yf_dev::protocol::DEFAULT_PORT)]
        port: u16,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
    },
    /// Package an installable app from the prebuilt runtime (no Rust
    /// needed). Metadata comes from `yf.json` next to the entry, overridden
    /// by the flags below.
    Package(Box<PackageArgs>),
    /// Like `dev`, and also run the app in a development host: the Mac
    /// Catalyst app (default), an iOS simulator, or an Android device or
    /// emulator.
    Run {
        /// Entry module, e.g. `App.tsx`.
        entry: PathBuf,
        /// Dev server port (the next free one is used if it is taken).
        #[arg(long, default_value_t = yf_dev::protocol::DEFAULT_PORT)]
        port: u16,
        /// Minify and strip debug info.
        #[arg(long)]
        release: bool,
        /// Log per-commit timings.
        #[arg(long)]
        trace: bool,
        /// Where to run.
        #[arg(long, value_enum, default_value_t = run::Target::Catalyst)]
        target: run::Target,
        /// iOS simulator to use (name or UDID); default: a booted one, else
        /// the newest iPhone.
        #[arg(long)]
        device: Option<String>,
    },
}

/// Arguments of `yf package`.
#[derive(Args)]
struct PackageArgs {
    /// Entry module, e.g. `App.tsx`.
    entry: PathBuf,
    /// What to build for.
    #[arg(long, value_enum)]
    target: package::PackageTarget,
    /// Output directory.
    #[arg(short, long, default_value = "out")]
    out: PathBuf,
    /// Display name.
    #[arg(long)]
    name: Option<String>,
    /// Bundle identifier, e.g. `com.example.app`.
    #[arg(long)]
    id: Option<String>,
    /// Version, e.g. `1.2.0`.
    #[arg(long = "app-version")]
    version: Option<String>,
    /// Build number.
    #[arg(long)]
    build: Option<u32>,
    /// App icon: a square PNG, ideally 1024×1024.
    #[arg(long)]
    icon: Option<PathBuf>,
    /// Code signing identity for devices, e.g. "Apple Development: …".
    #[arg(long)]
    sign: Option<String>,
    /// Provisioning profile for devices (`.mobileprovision`).
    #[arg(long)]
    profile: Option<PathBuf>,
    /// Android release keystore (default: the debug key).
    #[arg(long, requires = "key_alias")]
    keystore: Option<PathBuf>,
    /// Key alias in the keystore.
    #[arg(long)]
    key_alias: Option<String>,
    /// Keystore password (also `YF_KEYSTORE_PASSWORD`).
    #[arg(long, env = "YF_KEYSTORE_PASSWORD", hide_env_values = true)]
    keystore_password: Option<String>,
    /// Install and launch the app in the simulator afterwards.
    #[arg(long)]
    run: bool,
}

fn main() -> anyhow::Result<()> {
    logger::init();
    let command = Cli::parse().command;
    match &command {
        Command::Build { entry, .. } | Command::Dev { entry, .. } | Command::Run { entry, .. } => {
            init::write_types_for(entry)
        }
        Command::Package(args) => init::write_types_for(&args.entry),
        Command::Init { .. } | Command::Trace { .. } => {}
    }
    match command {
        Command::Init { dir, name } => init::init(&dir, name),
        Command::Build {
            entry,
            out,
            release,
        } => build::build(&entry, &out, release),
        Command::Trace {
            entry,
            taps,
            idle_ms,
            scroll,
            width,
            height,
        } => trace::trace(&entry, taps, idle_ms, scroll, width, height),
        Command::Dev {
            entry,
            port,
            release,
        } => {
            let session = dev::DevSession::start(&entry, port, release)?;
            session.print_urls();
            session.watch(|| Ok(false))
        }
        Command::Package(args) => {
            let PackageArgs {
                entry,
                target,
                out,
                name,
                id,
                version,
                build,
                icon,
                sign,
                profile,
                keystore,
                key_alias,
                keystore_password,
                run,
            } = *args;
            let mut config = package::read_config(&entry)?;
            config.name = name.or(config.name);
            config.id = id.or(config.id);
            config.version = version.or(config.version);
            config.build = build.or(config.build);
            config.icon = icon.or(config.icon);
            package::package(package::PackageOptions {
                entry: &entry,
                target,
                out: &out,
                config,
                sign: sign.as_deref(),
                profile: profile.as_deref(),
                keystore: keystore.as_deref().map(|k| {
                    (
                        k,
                        key_alias.as_deref().unwrap_or_default(),
                        keystore_password.as_deref().unwrap_or_default(),
                    )
                }),
                run,
            })
        }
        Command::Run {
            entry,
            port,
            release,
            trace,
            target,
            device,
        } => run::run(&entry, port, release, trace, target, device.as_deref()),
    }
}
