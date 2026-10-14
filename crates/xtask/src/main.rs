//! Maintainer tasks, run with `cargo xtask <task>`:
//!
//! - `android`: the Android runtime (`libyf.so` per ABI, `classes.dex`)
//!   into `target/android/<profile>`, where `yf run` / `yf package` find it.
//! - `dist`: the distribution in `dist/yf`: the `yf` CLI plus every
//!   prebuilt runtime, so app developers never need Rust.

mod android;
mod dist;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask")]
struct Cli {
    #[command(subcommand)]
    task: Task,
}

#[derive(Subcommand)]
enum Task {
    /// Build the Android runtime.
    Android {
        /// Optimized build.
        #[arg(long)]
        release: bool,
        /// Include development support (dev server, error overlay).
        #[arg(long)]
        dev: bool,
        /// ABIs to build (default: arm64-v8a and x86_64).
        #[arg(long)]
        abi: Vec<String>,
    },
    /// Run clippy on the Android crate (with the NDK set up).
    ClippyAndroid,
    /// Build the distribution in `dist/yf`.
    Dist,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().task {
        Task::Android { release, dev, abi } => {
            let abis = if abi.is_empty() {
                android::ABIS.iter().map(|a| a.to_string()).collect()
            } else {
                abi
            };
            android::build(release, dev, &abis).map(|_| ())
        }
        Task::ClippyAndroid => android::clippy(),
        Task::Dist => dist::dist(),
    }
}
