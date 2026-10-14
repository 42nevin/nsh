//! The `yf` command line tool.

mod build;
mod dev;
mod logger;
mod run;
mod trace;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "yf", version, about = "Build and run yf apps")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
    /// Like `dev`, and also run the app in the Mac Catalyst development host.
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
    },
}

fn main() -> anyhow::Result<()> {
    logger::init();
    match Cli::parse().command {
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
        Command::Run {
            entry,
            port,
            release,
            trace,
        } => run::run(&entry, port, release, trace),
    }
}
