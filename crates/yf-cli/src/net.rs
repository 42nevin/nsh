//! Network and assets for headless runs (`yf trace`): `fetch` on ureq (one
//! thread per request), assets from the source directory.

use std::time::Duration;

use ureq::{Agent, ResponseExt, http};
use yf_engine::{Assets, CancelFetch, FetchDone, HttpRequest, HttpResponse, Network};

/// `asset:/…` files relative to the app's source directory.
pub(crate) struct DirectoryAssets(pub(crate) std::path::PathBuf);

impl Assets for DirectoryAssets {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        let path = uri
            .strip_prefix("asset:/")
            .ok_or_else(|| format!("not an asset URI: {uri}"))?;
        std::fs::read(self.0.join(path)).map_err(|e| e.to_string())
    }
}

/// Bodies above this are reported as errors rather than buffered.
const MAX_BODY: u64 = 64 << 20;

pub(crate) struct UreqNetwork {
    follow: Agent,
    manual: Agent,
}

impl UreqNetwork {
    pub(crate) fn new() -> Self {
        let agent = |redirects| {
            Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(redirects)
                .max_redirects_will_error(false)
                .timeout_global(Some(Duration::from_secs(60)))
                .build()
                .into()
        };
        Self {
            follow: agent(10),
            manual: agent(0),
        }
    }
}

impl Network for UreqNetwork {
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch {
        let agent = if request.follow_redirects {
            self.follow.clone()
        } else {
            self.manual.clone()
        };
        std::thread::spawn(move || done(run(&agent, request).map_err(|e| e.to_string())));
        // Blocking requests cannot be interrupted; the engine ignores late results.
        Box::new(|| {})
    }
}

fn run(agent: &Agent, request: HttpRequest) -> Result<HttpResponse, Box<dyn std::error::Error>> {
    let mut builder = http::Request::builder()
        .method(request.method.as_str())
        .uri(request.url.as_str());
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    let mut response = agent.run(builder.body(request.body)?)?;
    let url = response.get_uri().to_string();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let status = response.status();
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY)
        .read_to_vec()?;
    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or_default().to_owned(),
        redirected: url != request.url,
        url,
        headers,
        body,
    })
}
