//! `fetch` within the app's capabilities: requests the app may not make
//! fail without reaching the network, and redirects are followed by the
//! engine (not by the platform's stack) so that every hop is checked again.
//! Hops are issued on the engine thread, like the first request.

use nsh_core::access::Access;
use nsh_js::{HttpRequest, HttpResponse};

/// Redirects followed at most (as browsers).
const MAX_REDIRECTS: usize = 20;

/// The message of a refused request.
pub(crate) fn refused(method: &str, url: &str) -> String {
    let host = nsh_core::access::parse_url(url).map_or_else(|| url.to_owned(), |u| u.host);
    format!("fetch {method} {url} is not granted: add `{host}` to capabilities.network in nsh.json")
}

/// A request the engine follows through its redirects.
#[derive(Debug)]
pub(crate) struct Followed {
    /// The request as JS made it (the URL of the current hop).
    pub(crate) request: HttpRequest,
    /// Redirects followed so far.
    pub(crate) hops: usize,
}

/// What to do with a hop's response.
pub(crate) enum Next {
    /// The response is the answer.
    Done(Result<HttpResponse, String>),
    /// Send this request next (checked against the capabilities).
    Hop(HttpRequest),
}

/// After a hop of `followed` answered `result`: the final result, or the
/// next hop.
pub(crate) fn next(
    access: &Access,
    followed: &mut Followed,
    result: Result<HttpResponse, String>,
) -> Next {
    let response = match result {
        Ok(response) if is_redirect(response.status) => response,
        Ok(mut response) => {
            response.redirected |= followed.hops > 0;
            return Next::Done(Ok(response));
        }
        err => return Next::Done(err),
    };
    let location = response
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("location"))
        .map(|(_, v)| v.clone());
    let Some(location) = location else {
        return Next::Done(Ok(response));
    };
    if followed.hops >= MAX_REDIRECTS {
        return Next::Done(Err("too many redirects".to_owned()));
    }
    let request = &mut followed.request;
    let base = if response.url.is_empty() {
        request.url.clone()
    } else {
        response.url.clone()
    };
    let url = resolve(&base, &location);
    // Fetch: 303 (and 301 / 302 after a POST) become GET without a body.
    if response.status == 303 || (matches!(response.status, 301 | 302) && request.method == "POST")
    {
        request.method = "GET".to_owned();
        request.body.clear();
        request.headers.retain(|(k, _)| {
            !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("content-length")
        });
    }
    if !access.allows_http(&request.method, &url) {
        return Next::Done(Err(refused(&request.method, &url)));
    }
    request.url = url;
    followed.hops += 1;
    Next::Hop(hop(request))
}

/// The request actually sent for a hop: the platform must not follow
/// redirects itself.
pub(crate) fn hop(request: &HttpRequest) -> HttpRequest {
    let mut hop = request.clone();
    hop.follow_redirects = false;
    hop
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// `location` against `base` (absolute, scheme-relative, absolute-path or
/// relative references).
fn resolve(base: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_owned();
    }
    let Some((scheme, rest)) = base.split_once("://") else {
        return location.to_owned();
    };
    if let Some(net) = location.strip_prefix("//") {
        return format!("{scheme}://{net}");
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let origin = format!("{scheme}://{}", &rest[..authority_end]);
    if location.starts_with('/') {
        return format!("{origin}{location}");
    }
    let path = &rest[authority_end..];
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    format!("{origin}{dir}/{location}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_redirect_locations() {
        assert_eq!(
            resolve("https://a.com/x/y?q", "https://b.com/"),
            "https://b.com/"
        );
        assert_eq!(resolve("https://a.com/x/y", "//c.com/z"), "https://c.com/z");
        assert_eq!(
            resolve("https://a.com:8443/x/y", "/z"),
            "https://a.com:8443/z"
        );
        assert_eq!(resolve("https://a.com/x/y?q", "z"), "https://a.com/x/z");
    }

    #[test]
    fn every_hop_is_checked() {
        let access = Access {
            declared: nsh_core::access::Capabilities::from_json(
                &serde_json::json!({ "network": { "hosts": ["a.com"] } }),
            )
            .unwrap(),
            granted: None,
        };
        let request = HttpRequest {
            method: "POST".into(),
            url: "https://a.com/start".into(),
            headers: vec![("Content-Type".into(), "text/plain".into())],
            body: b"x".to_vec(),
            follow_redirects: true,
        };
        let mut followed = Followed { request, hops: 0 };
        let redirect = |status, location: &str| {
            Ok(HttpResponse {
                status,
                url: String::new(),
                headers: vec![("Location".into(), location.into())],
                ..HttpResponse::default()
            })
        };
        // Within the grant: the next hop, as a GET after a 302 POST.
        let Next::Hop(hop) = next(&access, &mut followed, redirect(302, "/next")) else {
            panic!("expected a hop")
        };
        assert_eq!(
            (hop.method.as_str(), hop.url.as_str()),
            ("GET", "https://a.com/next")
        );
        assert!(!hop.follow_redirects && hop.body.is_empty() && hop.headers.is_empty());
        // Out of it: refused.
        let Next::Done(Err(err)) = next(&access, &mut followed, redirect(307, "https://evil.io/"))
        else {
            panic!("expected a refusal")
        };
        assert!(err.contains("`evil.io`"));
    }
}
