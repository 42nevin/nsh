//! Dev-server wire protocol (WebSocket at [`WS_PATH`]).
//!
//! Control messages are JSON text frames. A bundle is sent as a
//! [`ServerMessage::Bundle`] text frame immediately followed by one binary
//! frame with the payload and, if announced, one binary frame with the
//! source map (JSON).

use serde::{Deserialize, Serialize};

/// Protocol version; bump on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// WebSocket endpoint path.
pub const WS_PATH: &str = "/ws";

/// Default dev-server port.
pub const DEFAULT_PORT: u16 = 8790;

/// Messages from a dev host to the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting.
    Hello {
        /// [`PROTOCOL_VERSION`] of the client.
        protocol: u32,
        /// Bytecode format the host's engine can load; the server falls back
        /// to JS source when it differs from its own.
        bytecode_version: String,
        /// Human-readable device name, shown in logs.
        device: String,
    },
    /// A log line from the host (JS console or native).
    Log {
        /// Severity.
        level: LogLevel,
        /// Source, e.g. `js` or a Rust module path.
        target: String,
        /// The message.
        message: String,
    },
}

/// Messages from the server to dev hosts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Reply to [`ClientMessage::Hello`].
    Hello {
        /// [`PROTOCOL_VERSION`] of the server.
        protocol: u32,
    },
    /// A rebuild started.
    Building,
    /// The build failed; the host should show the error.
    BuildFailed {
        /// Compiler output.
        message: String,
    },
    /// A new bundle; the next binary frame is its payload.
    Bundle {
        /// Monotonic build number.
        version: u64,
        /// Payload format.
        format: BundleFormat,
        /// A second binary frame with the source map follows.
        #[serde(default)]
        source_map: bool,
    },
}

/// Format of a bundle payload.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleFormat {
    /// QuickJS bytecode.
    Bytecode,
    /// UTF-8 JS source.
    Source,
}

/// Log severity.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Errors.
    Error,
    /// Warnings.
    Warn,
    /// Informational.
    Info,
    /// Debugging.
    Debug,
    /// Very verbose.
    Trace,
}

impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Error => LogLevel::Error,
            log::Level::Warn => LogLevel::Warn,
            log::Level::Info => LogLevel::Info,
            log::Level::Debug => LogLevel::Debug,
            log::Level::Trace => LogLevel::Trace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format_is_stable() {
        let msg = ServerMessage::Bundle {
            version: 3,
            format: BundleFormat::Bytecode,
            source_map: true,
        };
        assert_eq!(
            serde_json::to_string(&msg).unwrap(),
            r#"{"type":"bundle","version":3,"format":"bytecode","source_map":true}"#
        );
        let hello: ClientMessage = serde_json::from_str(
            r#"{"type":"hello","protocol":1,"bytecode_version":"x","device":"Mac"}"#,
        )
        .unwrap();
        assert!(matches!(hello, ClientMessage::Hello { protocol: 1, .. }));
    }
}
