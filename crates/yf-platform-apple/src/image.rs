//! Image loading: fetch (NSURLSession or file), downsample-decode with
//! ImageIO on a background queue at the view's pixel size, cache decoded
//! images in an `NSCache` (purged under memory pressure), coalesce
//! concurrent requests and cancel downloads nobody waits for.

use std::ptr::NonNull;

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueGlobalPriority, GlobalQueueIdentifier};
use objc2::{AnyThread, DefinedClass, define_class, msg_send, rc::Retained};
use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFString, CFType, CGFloat, kCFBooleanTrue,
};
use objc2_foundation::{
    NSCache, NSData, NSDataReadingOptions, NSError, NSObject, NSString, NSURL, NSURLResponse,
    NSURLSession, NSURLSessionDataTask,
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

/// A decoded image handed from a background queue to the main thread,
/// with the pixel size of its source (before downsampling: `onLoad`
/// reports it, whatever size the view decodes at).
#[derive(Clone)]
pub(crate) struct DecodedImage {
    pub(crate) image: Retained<UIImage>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

define_class!(
    /// A cache entry: [`DecodedImage`] as an object.
    #[unsafe(super(NSObject))]
    #[ivars = DecodedImage]
    struct CachedImage;
);

impl CachedImage {
    fn new(image: DecodedImage) -> Retained<Self> {
        let this = Self::alloc().set_ivars(image);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

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

/// Reads `asset:/…` files for the engine (fonts): synchronously, on the
/// engine thread.
pub(crate) struct AssetReader(pub(crate) Option<AssetBase>);

impl yf_engine::Assets for AssetReader {
    fn read(&self, uri: &str) -> Result<Vec<u8>, String> {
        let path = uri
            .strip_prefix("asset:/")
            .ok_or_else(|| format!("not an asset URI: {uri}"))?;
        match &self.0 {
            Some(AssetBase::Directory(dir)) => {
                std::fs::read(dir.join(path)).map_err(|e| e.to_string())
            }
            Some(AssetBase::Http(base)) => {
                let url = url(&format!("{base}{path}")).ok_or("bad asset URL")?;
                NSData::dataWithContentsOfURL_options_error(&url, NSDataReadingOptions::empty())
                    .map(|d| d.to_vec())
                    .map_err(|e| e.localizedDescription().to_string())
            }
            None => Err("no asset location configured".to_owned()),
        }
    }
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
    Ready(DecodedImage),
    /// Loading; [`ImageLoader::finish`] is called on the main thread.
    Pending,
}

pub(crate) struct ImageLoader {
    cache: Retained<NSCache<NSString, CachedImage>>,
    inflight: FxHashMap<String, Inflight>,
    asset_base: Option<AssetBase>,
}

impl ImageLoader {
    pub(crate) fn new(asset_base: Option<AssetBase>) -> Self {
        // SAFETY: a fresh cache with NSString keys and CachedImage values.
        let cache = unsafe { NSCache::new() };
        // SAFETY: plain setter.
        unsafe { cache.setTotalCostLimit(CACHE_BYTES) };
        Self {
            cache,
            inflight: FxHashMap::default(),
            asset_base,
        }
    }

    /// Cache key: the same URI decoded at different sizes (or blurred or
    /// filtered differently) is a different image.
    pub(crate) fn key(uri: &str, max_pixels: u32, look: Look) -> String {
        let mut key = format!("{max_pixels}@");
        if look.blur > 0.0 {
            key.push_str(&format!("b{}@", look.blur));
        }
        if let Some(m) = look.matrix {
            use std::hash::{Hash, Hasher};
            let mut h = std::hash::DefaultHasher::new();
            m.map(f32::to_bits).hash(&mut h);
            key.push_str(&format!("m{:x}@", h.finish()));
        }
        key.push_str(uri);
        key
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
        look: Look,
        done: impl Fn(String, Result<DecodedImage, String>) + Clone + Send + 'static,
    ) -> Request {
        let key = Self::key(uri, max_pixels, look);
        let decoding = Decoding {
            max_pixels,
            scale,
            blur: look.blur as f64 * scale,
            matrix: look.matrix,
        };
        // SAFETY: NSString key and CachedImage value, as declared.
        if let Some(cached) = unsafe { self.cache.objectForKey(&NSString::from_str(&key)) } {
            return Request::Ready(cached.ivars().clone());
        }
        if let Some(inflight) = self.inflight.get_mut(&key) {
            if !inflight.waiters.contains(&view) {
                inflight.waiters.push(view);
            }
            return Request::Pending;
        }
        let task = match self.resolve(uri) {
            Some(Source::Url(url)) => Some(fetch_url(&url, key.clone(), decoding, done)),
            Some(Source::File(path)) => {
                fetch_file(path, key.clone(), decoding, done);
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
        if let Ok(decoded) = result {
            let image = &decoded.image;
            // SAFETY: plain getters on a valid image.
            let (size, scale) = unsafe { (image.size(), image.scale()) };
            // Animated images repeat frames for long delays: count distinct
            // ones.
            // SAFETY: a plain getter.
            let frames = unsafe { image.images() }.map_or(1, |frames| {
                let mut seen: Vec<*const UIImage> =
                    frames.iter().map(|f| Retained::as_ptr(&f)).collect();
                seen.sort_unstable();
                seen.dedup();
                seen.len()
            });
            let cost = (size.width * scale * size.height * scale * 4.0) as usize * frames;
            // SAFETY: NSString key and CachedImage value, as declared.
            unsafe {
                self.cache.setObject_forKey_cost(
                    &CachedImage::new(decoded.clone()),
                    &NSString::from_str(key),
                    cost,
                )
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

/// How a decoded image is changed: `blurRadius` plus `blur()` filters (in
/// points), and the composed color matrix of the other filters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Look {
    pub(crate) blur: f32,
    pub(crate) matrix: Option<[f32; 20]>,
}

/// How to decode: the longer side in pixels (0: natural), the screen
/// scale, the blur radius in pixels, a color matrix.
#[derive(Clone, Copy)]
struct Decoding {
    max_pixels: u32,
    scale: f64,
    blur: f64,
    matrix: Option<[f32; 20]>,
}

fn fetch_url(
    url: &NSURL,
    key: String,
    decoding: Decoding,
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
                    decode(data, decoding).ok_or_else(|| "cannot decode image".to_owned())
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
    decoding: Decoding,
    done: impl Fn(String, Result<DecodedImage, String>) + Send + 'static,
) {
    let queue = DispatchQueue::global_queue(GlobalQueueIdentifier::Priority(
        DispatchQueueGlobalPriority::Default,
    ));
    queue.exec_async(move || {
        let result = match std::fs::read(&path) {
            Ok(bytes) => decode(&CFData::from_bytes(&bytes), decoding)
                .ok_or_else(|| format!("cannot decode {}", path.display())),
            Err(err) => Err(format!("{}: {err}", path.display())),
        };
        DispatchQueue::main().exec_async(move || done(key, result));
    });
}

/// Frames of an animated image decoded at most (longer ones play the
/// first ones).
const MAX_FRAMES: usize = 600;

/// Decodes (and downsamples, blurs) with ImageIO. Animated GIF / PNG /
/// WebP images become an animated `UIImage` whose frames repeat for their
/// delays. Thread-safe.
fn decode(data: &CFData, d: Decoding) -> Option<DecodedImage> {
    let Decoding {
        max_pixels,
        scale,
        blur,
        matrix,
    } = d;
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
        let frame = |i: usize| -> Option<Retained<UIImage>> {
            let image = source.thumbnail_at_index(i, Some(options))?;
            let image = match blur > 0.0 || matrix.is_some() {
                true => crate::blur::filtered(&image, blur, matrix).unwrap_or(image),
                false => image,
            };
            Some(UIImage::imageWithCGImage_scale_orientation(
                &image,
                scale as CGFloat,
                UIImageOrientation::Up,
            ))
        };
        let (width, height) = source_size(&source);
        let decoded = |image| DecodedImage {
            image,
            width,
            height,
        };
        let count = source.count().min(MAX_FRAMES);
        if count <= 1 {
            return frame(0).map(decoded);
        }
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let Some(image) = frame(i) else { break };
            frames.push((image, frame_delay(&source, i)));
        }
        animated(frames).map(decoded)
    }
}

/// The pixel size of the first image, as displayed (EXIF orientations 5–8
/// turn it a quarter).
fn source_size(source: &CGImageSource) -> (u32, u32) {
    use objc2_image_io::{
        kCGImagePropertyOrientation, kCGImagePropertyPixelHeight, kCGImagePropertyPixelWidth,
    };
    // SAFETY: a CoreFoundation dictionary of CFString keys, read with the
    // documented ImageIO keys and value types.
    unsafe {
        let Some(props) = source.properties_at_index(0, None) else {
            return (0, 0);
        };
        let props: &CFDictionary<CFString, CFType> = props.cast_unchecked();
        let number = |key: &CFString| {
            props
                .get(key)
                .and_then(|v| v.downcast_ref::<CFNumber>().and_then(|n| n.as_i64()))
                .unwrap_or(0)
                .max(0) as u32
        };
        let (w, h) = (
            number(kCGImagePropertyPixelWidth),
            number(kCGImagePropertyPixelHeight),
        );
        match number(kCGImagePropertyOrientation) {
            5..=8 => (h, w),
            _ => (w, h),
        }
    }
}

/// The delay of frame `i` in centiseconds (GIF's unit), at least 2 (as
/// browsers: shorter ones show for 10).
fn frame_delay(source: &CGImageSource, i: usize) -> u32 {
    use objc2_image_io::{
        kCGImagePropertyAPNGDelayTime, kCGImagePropertyAPNGUnclampedDelayTime,
        kCGImagePropertyGIFDelayTime, kCGImagePropertyGIFDictionary,
        kCGImagePropertyGIFUnclampedDelayTime, kCGImagePropertyPNGDictionary,
        kCGImagePropertyWebPDelayTime, kCGImagePropertyWebPDictionary,
        kCGImagePropertyWebPUnclampedDelayTime,
    };
    // SAFETY: CoreFoundation dictionaries of CFString keys, read with the
    // documented ImageIO keys and value types.
    let seconds = unsafe {
        source.properties_at_index(i, None).and_then(|props| {
            let props: &CFDictionary<CFString, CFType> = props.cast_unchecked();
            [
                (
                    kCGImagePropertyGIFDictionary,
                    kCGImagePropertyGIFUnclampedDelayTime,
                    kCGImagePropertyGIFDelayTime,
                ),
                (
                    kCGImagePropertyPNGDictionary,
                    kCGImagePropertyAPNGUnclampedDelayTime,
                    kCGImagePropertyAPNGDelayTime,
                ),
                (
                    kCGImagePropertyWebPDictionary,
                    kCGImagePropertyWebPUnclampedDelayTime,
                    kCGImagePropertyWebPDelayTime,
                ),
            ]
            .into_iter()
            .find_map(|(dict, unclamped, clamped)| {
                let format = props.get(dict)?;
                let format: &CFDictionary<CFString, CFType> =
                    format.downcast_ref::<CFDictionary>()?.cast_unchecked();
                let number = |key| {
                    format
                        .get(key)
                        .and_then(|v| v.downcast_ref::<CFNumber>().and_then(|n| n.as_cgfloat()))
                        .filter(|s| *s > 0.0)
                };
                number(unclamped).or_else(|| number(clamped))
            })
        })
    };
    match seconds {
        Some(s) if s >= 0.02 => (s * 100.0).round() as u32,
        _ => 10,
    }
}

/// An animated image from frames and their delays (centiseconds): each
/// frame repeats for its delay in units of their common divisor, as
/// `UIImage` animations have one frame duration.
fn animated(frames: Vec<(Retained<UIImage>, u32)>) -> Option<Retained<UIImage>> {
    fn gcd(a: u32, b: u32) -> u32 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let unit = frames.iter().fold(0, |g, &(_, d)| gcd(g, d)).max(1);
    let total: u32 = frames.iter().map(|&(_, d)| d).sum();
    // Very uneven delays: a coarser unit keeps the frame list short.
    let unit = unit.max(total / 2000);
    let mut sequence = Vec::new();
    for (image, delay) in &frames {
        for _ in 0..(delay / unit).max(1) {
            sequence.push(image.clone());
        }
    }
    let list = objc2_foundation::NSArray::from_retained_slice(&sequence);
    UIImage::animatedImageWithImages_duration(&list, total as f64 / 100.0)
}
