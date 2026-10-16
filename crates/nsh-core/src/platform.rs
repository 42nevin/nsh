//! What the platform reports about a surface (its size, safe area,
//! keyboard, appearance, device) and the HTTP requests it carries out: the
//! data both Rust components and JS read, with or without a JS runtime.

/// Edge insets in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    /// Top.
    pub top: f32,
    /// Right.
    pub right: f32,
    /// Bottom.
    pub bottom: f32,
    /// Left.
    pub left: f32,
}

/// The system appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    /// Light mode.
    #[default]
    Light,
    /// Dark mode.
    Dark,
}

/// The operating system (`Platform.OS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// iOS, iPadOS, Mac Catalyst.
    Ios,
    /// Android.
    Android,
    /// macOS (a desktop backend).
    MacOs,
    /// Windows.
    Windows,
    /// Linux.
    Linux,
}

impl Os {
    /// Its `Platform.OS` name.
    pub fn name(self) -> &'static str {
        match self {
            Os::Ios => "ios",
            Os::Android => "android",
            Os::MacOs => "macos",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    /// A desktop OS (`desktop={{…}}` props, `Platform.select({ desktop })`).
    pub fn is_desktop(self) -> bool {
        matches!(self, Os::MacOs | Os::Windows | Os::Linux)
    }
}

impl Default for Os {
    /// The mobile OS of this build (`nsh trace` behaves as iOS); desktop
    /// backends set theirs.
    fn default() -> Self {
        if cfg!(target_os = "android") {
            Os::Android
        } else {
            Os::Ios
        }
    }
}

/// The kind of device (React Native's `Platform.isPad`, `isTV`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Idiom {
    /// A phone.
    #[default]
    Phone,
    /// A tablet.
    Pad,
    /// A Mac (Mac Catalyst).
    Mac,
    /// A TV.
    Tv,
    /// A desktop computer (a desktop backend; Mac Catalyst is [`Idiom::Mac`]).
    Desktop,
}

/// What the platform knows about the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    /// Size of the surface in points.
    pub window: (f32, f32),
    /// Insets not covered by system UI (notch, home indicator, title bar).
    pub safe_area: Insets,
    /// Height of the software keyboard over the surface; 0 when hidden.
    pub keyboard_height: f32,
    /// Duration of the keyboard's show / hide animation in seconds.
    pub keyboard_duration: f32,
    /// Light or dark appearance.
    pub color_scheme: ColorScheme,
    /// Preferred text size relative to the default (1.0).
    pub font_scale: f32,
    /// The user's language is written right to left.
    pub rtl: bool,
    /// The user asked for less motion (iOS Reduce Motion, Android's
    /// animator duration scale at 0).
    pub reduce_motion: bool,
    /// Device pixels per point.
    pub pixel_ratio: f32,
    /// A screen reader runs (VoiceOver, TalkBack).
    pub screen_reader: bool,
    /// The operating system.
    pub os: Os,
    /// The OS version: major and minor (iOS, desktops), Android's API level
    /// and 0.
    pub os_version: (u16, u16),
    /// The kind of device.
    pub idiom: Idiom,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            window: (0.0, 0.0),
            safe_area: Insets::default(),
            keyboard_height: 0.0,
            keyboard_duration: 0.25,
            color_scheme: ColorScheme::Light,
            font_scale: 1.0,
            rtl: false,
            reduce_motion: false,
            pixel_ratio: 1.0,
            screen_reader: false,
            os: Os::default(),
            os_version: (0, 0),
            idiom: Idiom::Phone,
        }
    }
}

/// An HTTP request for the platform's network stack (made by JS `fetch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Upper-case method (`GET`, `POST`, ...).
    pub method: String,
    /// Absolute URL.
    pub url: String,
    /// Header names and values, in order; names may repeat.
    pub headers: Vec<(String, String)>,
    /// Request body.
    pub body: HttpBody,
    /// Follow redirects (`redirect: "follow"`, the default).
    pub follow_redirects: bool,
}

/// The body of an [`HttpRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HttpBody {
    /// No body.
    #[default]
    Empty,
    /// All of it, known up front.
    Bytes(Vec<u8>),
    /// Produced while the request is sent (a `ReadableStream`), in chunks
    /// the stack asks for one at a time ([`FetchEvent::BodyWanted`]); sent
    /// with chunked transfer encoding.
    Stream,
}

impl HttpBody {
    /// No body at all.
    pub fn is_empty(&self) -> bool {
        match self {
            HttpBody::Empty => true,
            HttpBody::Bytes(b) => b.is_empty(),
            HttpBody::Stream => false,
        }
    }
}

/// The status line and headers of a response, before its body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpHead {
    /// Status code.
    pub status: u16,
    /// Reason phrase, if the stack reports one.
    pub status_text: String,
    /// Final URL, after redirects.
    pub url: String,
    /// True if a redirect was followed.
    pub redirected: bool,
    /// Header names and values.
    pub headers: Vec<(String, String)>,
}

/// What happens to a request, in order: (for a streamed body) any number
/// of `BodyWanted`; then `Head`, `Data`… and `End` — or `Failed` at any
/// point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchEvent {
    /// A streamed body's next chunk is wanted: first when the stack is
    /// ready to send, then each time the last chunk was handed on.
    BodyWanted,
    /// The response's status and headers arrived.
    Head(HttpHead),
    /// Bytes of the response body, in order.
    Data(Vec<u8>),
    /// The body is complete.
    End,
    /// The request failed (no response, or the body broke off).
    Failed(String),
}

/// What JS asks of a request it made (the JS host queues these for the
/// engine, which hands them to the stack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOp {
    /// Make the request.
    Start(HttpRequest),
    /// The next chunk of a streamed body (after [`FetchEvent::BodyWanted`]).
    Send(Vec<u8>),
    /// A streamed body is complete.
    Finish,
    /// Stop (`true`) or go on (`false`) reading the response body: nobody
    /// reads what arrived.
    Pause(bool),
    /// Abort the request (`AbortController`, or the body was cancelled).
    Cancel,
}
