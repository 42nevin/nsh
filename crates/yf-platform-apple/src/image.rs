//! Image loading: fetch (NSURLSession or file), downsample-decode with
//! ImageIO on a background queue at the view's pixel size, cache decoded
//! images in an `NSCache` (purged under memory pressure), coalesce
//! concurrent requests and cancel downloads nobody waits for.

use std::ptr::NonNull;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueGlobalPriority, GlobalQueueIdentifier};
use objc2::rc::Retained;
use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFString, CFType, CGFloat, kCFBooleanTrue,
};
use objc2_foundation::{
    NSCache, NSData, NSError, NSString, NSURL, NSURLResponse, NSURLSession, NSURLSessionDataTask,
};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceShouldCacheImmediately,
    kCGImageSourceThumbnailMaxPixelSize,
};
use objc2_ui_kit::{UIImage, UIImageOrientation};
use rustc_hash::FxHashMap;
use yf_core::NodeId;

/// Decoded images kept in memory (bytes), before `NSCache` evicts.
const CACHE_BYTES: usize = 96 << 20;

/// A decoded image handed from a background queue to the main thread.
pub(crate) struct DecodedImage(pub(crate) Retained<UIImage>);

// SAFETY: UIImage is immutable and documented as safe to use from any
// thread; it is only created off the main thread and then moved.
unsafe impl Send for DecodedImage {}

/// Where an `asset:/path` URI points.
#[derive(Debug, Clone)]
pub enum AssetBase {
    /// Served over HTTP (the dev server): `http://host:port/assets/`.
    Http(String),
    /// A directory on disk (built app resources).
    Directory(std::path::PathBuf),
}

enum Source {
    Url(Retained<NSURL>),
    File(std::path::PathBuf),
}

struct Inflight {
    task: Option<Retained<NSURLSessionDataTask>>,
    waiters: Vec<NodeId>,
}

/// Result of a request.
pub(crate) enum Request {
    /// Already decoded.
    Ready(Retained<UIImage>),
    /// Loading; [`ImageLoader::finish`] is called on the main thread.
    Pending,
}

pub(crate) struct ImageLoader {
    cache: Retained<NSCache<NSString, UIImage>>,
    inflight: FxHashMap<String, Inflight>,
    asset_base: Option<AssetBase>,
}

impl ImageLoader {
    pub(crate) fn new(asset_base: Option<AssetBase>) -> Self {
        // SAFETY: a fresh cache with NSString keys and UIImage values.
        let cache = unsafe { NSCache::new() };
        // SAFETY: plain setter.
        unsafe { cache.setTotalCostLimit(CACHE_BYTES) };
        Self {
            cache,
            inflight: FxHashMap::default(),
            asset_base,
        }
    }

    /// Cache key: the same URI decoded at different sizes is a different
    /// image.
    pub(crate) fn key(uri: &str, max_pixels: u32) -> String {
        format!("{max_pixels}@{uri}")
    }

    /// Requests `uri` for `view`, decoded to at most `max_pixels` on its
    /// longer side (0 = natural size). `done` delivers completions to the
    /// main thread.
    pub(crate) fn request(
        &mut self,
        view: NodeId,
        uri: &str,
        max_pixels: u32,
        scale: f64,
        done: impl Fn(String, Result<DecodedImage, String>) + Clone + Send + 'static,
    ) -> Request {
        let key = Self::key(uri, max_pixels);
        // SAFETY: NSString key and UIImage value, as declared.
        if let Some(image) = unsafe { self.cache.objectForKey(&NSString::from_str(&key)) } {
            return Request::Ready(image);
        }
        if let Some(inflight) = self.inflight.get_mut(&key) {
            if !inflight.waiters.contains(&view) {
                inflight.waiters.push(view);
            }
            return Request::Pending;
        }
        let task = match self.resolve(uri) {
            Some(Source::Url(url)) => Some(fetch_url(&url, key.clone(), max_pixels, scale, done)),
            Some(Source::File(path)) => {
                fetch_file(path, key.clone(), max_pixels, scale, done);
                None
            }
            None => {
                let message = format!("unsupported image source `{uri}`");
                DispatchQueue::main().exec_async(move || done(key, Err(message)));
                return Request::Pending;
            }
        };
        self.inflight.insert(
            key,
            Inflight {
                task,
                waiters: vec![view],
            },
        );
        Request::Pending
    }

    /// `view` no longer wants `key`; cancels the download if nobody does.
    pub(crate) fn cancel(&mut self, view: NodeId, key: &str) {
        let Some(inflight) = self.inflight.get_mut(key) else {
            return;
        };
        inflight.waiters.retain(|&v| v != view);
        if inflight.waiters.is_empty() {
            if let Some(task) = &inflight.task {
                task.cancel();
            }
            self.inflight.remove(key);
        }
    }

    /// Completes a load (main thread). Returns the views waiting for it.
    pub(crate) fn finish(
        &mut self,
        key: &str,
        result: &Result<DecodedImage, String>,
    ) -> Vec<NodeId> {
        let waiters = self
            .inflight
            .remove(key)
            .map(|i| i.waiters)
            .unwrap_or_default();
        if let Ok(DecodedImage(image)) = result {
            // SAFETY: plain getters on a valid image.
            let (size, scale) = unsafe { (image.size(), image.scale()) };
            let cost = (size.width * scale * size.height * scale * 4.0) as usize;
            // SAFETY: NSString key and UIImage value, as declared.
            unsafe {
                self.cache
                    .setObject_forKey_cost(image, &NSString::from_str(key), cost)
            };
        }
        waiters
    }

    fn resolve(&self, uri: &str) -> Option<Source> {
        if let Some(path) = uri.strip_prefix("asset:/") {
            return match self.asset_base.as_ref()? {
                AssetBase::Http(base) => url(&format!("{base}{path}")).map(Source::Url),
                AssetBase::Directory(dir) => Some(Source::File(dir.join(path))),
            };
        }
        if uri.starts_with("http://") || uri.starts_with("https://") {
            return url(uri).map(Source::Url);
        }
        let path = uri.strip_prefix("file://").unwrap_or(uri);
        path.starts_with('/').then(|| Source::File(path.into()))
    }
}

fn url(s: &str) -> Option<Retained<NSURL>> {
    NSURL::URLWithString(&NSString::from_str(s))
}

fn fetch_url(
    url: &NSURL,
    key: String,
    max_pixels: u32,
    scale: f64,
    done: impl Fn(String, Result<DecodedImage, String>) + Clone + Send + 'static,
) -> Retained<NSURLSessionDataTask> {
    let handler = RcBlock::new(
        move |data: *mut NSData, _response: *mut NSURLResponse, error: *mut NSError| {
            // Runs on the session's background queue: decode here.
            let result = match (NonNull::new(data), NonNull::new(error)) {
                (Some(data), _) => {
                    // SAFETY: NSData is toll-free bridged with CFData; the block
                    // argument is valid for the duration of the call.
                    let data = unsafe { &*data.as_ptr().cast::<CFData>() };
                    decode(data, max_pixels, scale).ok_or_else(|| "cannot decode image".to_owned())
                }
                // SAFETY: a valid NSError for the duration of the call.
                (None, Some(error)) => {
                    Err(unsafe { error.as_ref() }.localizedDescription().to_string())
                }
                (None, None) => Err("empty response".to_owned()),
            };
            let (key, done) = (key.clone(), done.clone());
            DispatchQueue::main().exec_async(move || done(key, result));
        },
    );
    let session = NSURLSession::sharedSession();
    // SAFETY: the handler only captures `Send` data.
    let task = unsafe { session.dataTaskWithURL_completionHandler(url, &handler) };
    task.resume();
    task
}

fn fetch_file(
    path: std::path::PathBuf,
    key: String,
    max_pixels: u32,
    scale: f64,
    done: impl Fn(String, Result<DecodedImage, String>) + Send + 'static,
) {
    let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::Priority(
        DispatchQueueGlobalPriority::Default,
    ));
    queue.exec_async(move || {
        let result = match std::fs::read(&path) {
            Ok(bytes) => decode(&CFData::from_bytes(&bytes), max_pixels, scale)
                .ok_or_else(|| format!("cannot decode {}", path.display())),
            Err(err) => Err(format!("{}: {err}", path.display())),
        };
        DispatchQueue::main().exec_async(move || done(key, result));
    });
}

/// Decodes (and downsamples) with ImageIO. Thread-safe.
fn decode(data: &CFData, max_pixels: u32, scale: f64) -> Option<DecodedImage> {
    // SAFETY: valid data and option dictionaries of the documented types.
    unsafe {
        let source = CGImageSource::with_data(data, None)?;
        let yes: &CFType = kCFBooleanTrue?;
        let max = CFNumber::new_cgfloat(max_pixels as CGFloat);
        let mut keys: Vec<&CFString> = vec![
            kCGImageSourceCreateThumbnailFromImageAlways,
            kCGImageSourceCreateThumbnailWithTransform,
            kCGImageSourceShouldCacheImmediately,
        ];
        let mut values: Vec<&CFType> = vec![yes, yes, yes];
        if max_pixels > 0 {
            keys.push(kCGImageSourceThumbnailMaxPixelSize);
            values.push(&max);
        }
        let options = CFDictionary::from_slices(&keys, &values);
        let options: &CFDictionary = options.as_opaque();
        let image = source.thumbnail_at_index(0, Some(options))?;
        let image = UIImage::imageWithCGImage_scale_orientation(
            &image,
            scale as CGFloat,
            UIImageOrientation::Up,
        );
        Some(DecodedImage(image))
    }
}
