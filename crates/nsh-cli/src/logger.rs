use std::io::{IsTerminal, Write};

use log::{Level, LevelFilter, Log, Metadata, Record};
use nsh_dev::{ClientLog, protocol::LogLevel};

struct Logger {
    color: bool,
}

static LOGGER: std::sync::OnceLock<Logger> = std::sync::OnceLock::new();

/// Installs the terminal logger.
pub(crate) fn init() {
    let logger = LOGGER.get_or_init(|| Logger {
        color: std::io::stderr().is_terminal(),
    });
    let _ = log::set_logger(logger);
    log::set_max_level(LevelFilter::Info);
}

/// Prints a log line received from a dev host.
pub(crate) fn print_client(log: &ClientLog) {
    let level = match log.level {
        LogLevel::Error => Level::Error,
        LogLevel::Warn => Level::Warn,
        LogLevel::Info => Level::Info,
        LogLevel::Debug => Level::Debug,
        LogLevel::Trace => Level::Trace,
    };
    let color = LOGGER.get().is_some_and(|l| l.color);
    write_line(
        color,
        level,
        &format!("{} {}", log.device, log.target),
        &log.message,
    );
}

fn write_line(color: bool, level: Level, source: &str, message: &str) {
    let (tag, code) = match level {
        Level::Error => ("error", "31"),
        Level::Warn => ("warn", "33"),
        Level::Info => ("info", "32"),
        Level::Debug => ("debug", "36"),
        Level::Trace => ("trace", "90"),
    };
    let mut err = std::io::stderr().lock();
    let _ = if color {
        writeln!(
            err,
            "\x1b[{code}m{tag:>5}\x1b[0m \x1b[90m{source}\x1b[0m {message}"
        )
    } else {
        writeln!(err, "{tag:>5} {source} {message}")
    };
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        // Ours and the app's (`js`); dependencies (rolldown, axum, ...) only
        // when they are loud.
        let target = metadata.target();
        target.starts_with("nsh") || target == "js" || metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            let target = record.target();
            // The command line's own modules as `nsh::build`, other crates
            // as `bundler::...`.
            let source = match target.strip_prefix("nsh_cli") {
                Some(rest) => format!("nsh{rest}"),
                None => target.strip_prefix("nsh_").unwrap_or(target).to_owned(),
            };
            let source = source.as_str();
            write_line(
                self.color,
                record.level(),
                source,
                &record.args().to_string(),
            );
        }
    }

    fn flush(&self) {}
}
