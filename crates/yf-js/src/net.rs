//! HTTP requests made by `fetch`, as handed to the platform's network stack.

/// Identifies a request between JS and the network stack.
pub type RequestId = u32;

/// A request made by `fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Upper-case method (`GET`, `POST`, ...).
    pub method: String,
    /// Absolute URL.
    pub url: String,
    /// Header names and values, in order; names may repeat.
    pub headers: Vec<(String, String)>,
    /// Request body (empty for none).
    pub body: Vec<u8>,
    /// Follow redirects (`redirect: "follow"`, the default).
    pub follow_redirects: bool,
}

/// A response to a [`HttpRequest`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpResponse {
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
    /// The whole body.
    pub body: Vec<u8>,
}

/// Requests and cancellations made by JS since the last
/// [`crate::Host::take_network`].
#[derive(Debug, Default)]
pub struct NetworkQueue {
    /// New requests.
    pub requests: Vec<(RequestId, HttpRequest)>,
    /// Requests aborted by JS (`AbortController`).
    pub cancels: Vec<RequestId>,
}

impl NetworkQueue {
    /// True if there is nothing to do.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty() && self.cancels.is_empty()
    }
}
