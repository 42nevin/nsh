//! `fetch` on `NSURLSession`: the system's HTTP stack (HTTP/2 and 3, system
//! proxies, cookies, App Transport Security), shared by every surface.

use std::{
    ptr::NonNull,
    sync::{Arc, OnceLock},
};

use block2::RcBlock;
use objc2::{
    AnyThread, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, ProtocolObject},
};
use objc2_foundation::{
    NSData, NSError, NSHTTPURLResponse, NSMutableURLRequest, NSObjectProtocol, NSString, NSURL,
    NSURLRequest, NSURLResponse, NSURLSession, NSURLSessionConfiguration, NSURLSessionDataTask,
    NSURLSessionDelegate, NSURLSessionTask, NSURLSessionTaskDelegate,
};
use yf_engine::{CancelFetch, FetchDone, HttpRequest, HttpResponse, Network};

/// The process-wide network stack.
pub(crate) fn network() -> Arc<dyn Network> {
    static NETWORK: OnceLock<Arc<UrlSessionNetwork>> = OnceLock::new();
    NETWORK
        .get_or_init(|| Arc::new(UrlSessionNetwork::new()))
        .clone()
}

struct UrlSessionNetwork {
    /// Follows redirects (the default for `fetch`).
    follow: Retained<NSURLSession>,
    /// Stops at redirects (`redirect: "manual"`).
    manual: Retained<NSURLSession>,
}

// SAFETY: NSURLSession is thread-safe (documented); the sessions are never
// mutated after creation.
unsafe impl Send for UrlSessionNetwork {}
// SAFETY: as above.
unsafe impl Sync for UrlSessionNetwork {}

impl UrlSessionNetwork {
    fn new() -> Self {
        let delegate = NoRedirects::new();
        // SAFETY: a default configuration and a delegate implementing the
        // optional redirect method; the session keeps the delegate alive.
        let manual = unsafe {
            NSURLSession::sessionWithConfiguration_delegate_delegateQueue(
                &NSURLSessionConfiguration::defaultSessionConfiguration(),
                Some(ProtocolObject::from_ref(&*delegate)),
                None,
            )
        };
        Self {
            follow: NSURLSession::sharedSession(),
            manual,
        }
    }
}

/// A data task handle that may be cancelled from the engine thread.
struct Task(Retained<NSURLSessionDataTask>);

// SAFETY: NSURLSessionTask is thread-safe; only `cancel` is called.
unsafe impl Send for Task {}

impl Network for UrlSessionNetwork {
    fn fetch(&self, request: HttpRequest, done: FetchDone) -> CancelFetch {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&request.url)) else {
            done(Err(format!("invalid URL `{}`", request.url)));
            return Box::new(|| {});
        };
        let ns = NSMutableURLRequest::requestWithURL(&url);
        ns.setHTTPMethod(&NSString::from_str(&request.method));
        for (name, value) in &request.headers {
            ns.addValue_forHTTPHeaderField(&NSString::from_str(value), &NSString::from_str(name));
        }
        if !request.body.is_empty() {
            ns.setHTTPBody(Some(&NSData::with_bytes(&request.body)));
        }
        let requested = request.url;
        // NSURLSession calls the handler once; `done` is moved out on that call.
        let done = std::sync::Mutex::new(Some(done));
        let handler = RcBlock::new(
            move |data: *mut NSData, response: *mut NSURLResponse, error: *mut NSError| {
                let Some(done) = done.lock().ok().and_then(|mut d| d.take()) else {
                    return;
                };
                // SAFETY: the block arguments are valid (or null) for the call.
                let result = unsafe { complete(&requested, data, response, error) };
                done(result);
            },
        );
        let session = if request.follow_redirects {
            &self.follow
        } else {
            &self.manual
        };
        // SAFETY: the handler only captures `Send` data.
        let task = unsafe { session.dataTaskWithRequest_completionHandler(&ns, &handler) };
        task.resume();
        let task = Task(task);
        Box::new(move || task.0.cancel())
    }
}

/// Builds the result of a finished task.
///
/// # Safety
///
/// The pointers must be valid or null, as passed to a data task's handler.
unsafe fn complete(
    requested: &str,
    data: *mut NSData,
    response: *mut NSURLResponse,
    error: *mut NSError,
) -> Result<HttpResponse, String> {
    if let Some(error) = NonNull::new(error) {
        // SAFETY: a valid NSError for the duration of the call.
        return Err(unsafe { error.as_ref() }.localizedDescription().to_string());
    }
    let Some(response) = NonNull::new(response) else {
        return Err("no response".to_owned());
    };
    // SAFETY: valid for the duration of the call.
    let response = unsafe { response.as_ref() };
    let Some(http) = response.downcast_ref::<NSHTTPURLResponse>() else {
        return Err("not an HTTP response".to_owned());
    };
    let url = response
        .URL()
        .and_then(|u| u.absoluteString())
        .map_or_else(|| requested.to_owned(), |s| s.to_string());
    let (keys, values) = http.allHeaderFields().to_vecs();
    let headers = keys
        .iter()
        .zip(values.iter())
        .filter_map(|(k, v)| {
            let k = k.downcast_ref::<NSString>()?;
            let v = v.downcast_ref::<NSString>()?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();
    let body = NonNull::new(data)
        // SAFETY: valid for the duration of the call.
        .map(|d| unsafe { d.as_ref() }.to_vec())
        .unwrap_or_default();
    Ok(HttpResponse {
        status: http.statusCode() as u16,
        // HTTP/2 and 3 have no reason phrases; browsers report "" too.
        status_text: String::new(),
        redirected: url != requested,
        url,
        headers,
        body,
    })
}

define_class!(
    /// Session delegate that refuses redirects: the 3xx response itself is
    /// delivered (`redirect: "manual"`).
    #[unsafe(super(NSObject))]
    #[name = "YFNoRedirects"]
    struct NoRedirects;

    unsafe impl NSObjectProtocol for NoRedirects {}

    unsafe impl NSURLSessionDelegate for NoRedirects {}

    unsafe impl NSURLSessionTaskDelegate for NoRedirects {
        #[unsafe(method(URLSession:task:willPerformHTTPRedirection:newRequest:completionHandler:))]
        fn will_redirect(
            &self,
            _session: &NSURLSession,
            _task: &NSURLSessionTask,
            _response: &NSHTTPURLResponse,
            _request: &NSURLRequest,
            completion: &block2::DynBlock<dyn Fn(*mut NSURLRequest)>,
        ) {
            completion.call((std::ptr::null_mut(),));
        }
    }
);

impl NoRedirects {
    fn new() -> Retained<Self> {
        let this = Self::alloc().set_ivars(());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}
