//! Applies mount batches to UIKit views and owns the per-surface native
//! state: views, animation graph, image loads and gesture recognizers.

use std::{cell::RefCell, sync::Arc};

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained, runtime::ProtocolObject, sel};
use objc2_core_foundation::{CGAffineTransform, CGFloat, CGPoint, CGRect, CGSize};
use objc2_ui_kit::{
    UIActivityIndicatorView, UIActivityIndicatorViewStyle, UIColor, UIControlEvents, UIDevice,
    UIGestureRecognizer, UIGestureRecognizerState, UIImage, UIImageView,
    UILongPressGestureRecognizer, UIPanGestureRecognizer, UIRefreshControl, UIScrollView,
    UIScrollViewContentInsetAdjustmentBehavior, UITapGestureRecognizer, UIUserInterfaceIdiom,
    UIView, UIViewContentMode,
};
use yf_anim::{AnimProp, AnimValue};
use yf_core::{
    Color, EventKind, EventMask, EventPayload, ImageSource, MountBatch, MountItem, NodeId, Rect,
    ResizeMode, ViewKind, ViewStyle,
};
use yf_engine::EngineControl;

use crate::{
    anim::Animator,
    image::{AssetBase, DecodedImage, ImageLoader, Request},
    target::EventTarget,
    text_view::TextView,
};

/// The mounter of one surface, shared with native callbacks.
pub(crate) type SharedMounter = Arc<MainThreadBound<RefCell<Option<Mounter>>>>;

enum Native {
    View(Retained<UIView>),
    Text(Retained<TextView>),
    Image(Retained<UIImageView>),
    Scroll(Retained<UIScrollView>),
}

impl Native {
    #[inline]
    fn view(&self) -> &UIView {
        match self {
            Native::View(v) => v,
            Native::Text(t) => t,
            Native::Image(i) => i,
            Native::Scroll(s) => s,
        }
    }
}

#[derive(Default)]
struct Gestures {
    tap: Option<Retained<UITapGestureRecognizer>>,
    long_press: Option<Retained<UILongPressGestureRecognizer>>,
    pan: Option<Retained<UIPanGestureRecognizer>>,
    refresh: Option<Refresh>,
}

/// Pull-to-refresh. `UIRefreshControl` throws under Mac Catalyst's Mac
/// idiom, so there the pull is detected in the scroll delegate and shown
/// with a spinner revealed by a content inset.
enum Refresh {
    Native(Retained<UIRefreshControl>),
    Custom {
        spinner: Retained<UIActivityIndicatorView>,
        refreshing: bool,
    },
}

/// How far past the top a release must be to trigger a custom refresh.
const REFRESH_PULL: CGFloat = 64.0;
/// Space kept above the content while a custom refresh runs.
const REFRESH_INSET: CGFloat = 56.0;

fn place_spinner(spinner: &UIActivityIndicatorView, width: f32) {
    spinner.setCenter(CGPoint::new(width as CGFloat / 2.0, -REFRESH_INSET / 2.0));
}

struct Slot {
    native: Native,
    style: ViewStyle,
    events: EventMask,
    gestures: Gestures,
    bindings: Vec<(AnimProp, yf_anim::NodeId)>,
    pan: (Option<yf_anim::NodeId>, Option<yf_anim::NodeId>),
    /// Values of the pan nodes when the drag began.
    pan_start: (f32, f32),
    scroll_value: Option<yf_anim::NodeId>,
    horizontal: bool,
    /// Image to show; loaded once the frame is known.
    image: Option<ImageSource>,
    /// The load in flight for this view.
    image_key: Option<String>,
}

impl Slot {
    fn new(native: Native) -> Self {
        Self {
            native,
            style: ViewStyle::default(),
            events: EventMask::default(),
            gestures: Gestures::default(),
            bindings: Vec::new(),
            pan: (None, None),
            pan_start: (0.0, 0.0),
            scroll_value: None,
            horizontal: false,
            image: None,
            image_key: None,
        }
    }
}

/// Applies mount batches to UIKit views. Main thread only.
pub(crate) struct Mounter {
    /// Indexed by node id (ids are dense).
    views: Vec<Option<Slot>>,
    /// Views with animated bindings, re-evaluated when values change.
    animated: Vec<NodeId>,
    /// Image views waiting for their frame before loading.
    pending_images: Vec<NodeId>,
    target: Retained<EventTarget>,
    animator: Animator,
    images: ImageLoader,
    control: EngineControl,
    shared: SharedMounter,
    /// Mac Catalyst in the Mac idiom (no `UIRefreshControl`).
    mac_idiom: bool,
    mtm: MainThreadMarker,
}

impl Mounter {
    pub(crate) fn new(
        mtm: MainThreadMarker,
        root: Retained<UIView>,
        control: EngineControl,
        shared: SharedMounter,
        asset_base: Option<AssetBase>,
    ) -> Self {
        let target = EventTarget::new(mtm, control.clone(), shared.clone());
        let mut views = Vec::with_capacity(256);
        views.resize_with(2, || None);
        root.setTag(yf_core::ROOT_ID as isize);
        views[yf_core::ROOT_ID as usize] = Some(Slot::new(Native::View(root)));
        Self {
            views,
            animated: Vec::new(),
            pending_images: Vec::new(),
            animator: Animator::new(&target),
            target,
            images: ImageLoader::new(asset_base),
            control,
            shared,
            mac_idiom: UIDevice::currentDevice(mtm).userInterfaceIdiom()
                == UIUserInterfaceIdiom::Mac,
            mtm,
        }
    }

    pub(crate) fn apply(&mut self, batch: MountBatch) {
        let mut anim_changed = false;
        for item in batch.items {
            anim_changed |= self.apply_item(item);
        }
        if anim_changed {
            self.apply_animations();
        }
        self.report_finished();
        self.start_image_loads();
    }

    #[inline]
    fn slot(&self, id: NodeId) -> Option<&Slot> {
        self.views.get(id as usize).and_then(Option::as_ref)
    }

    #[inline]
    fn slot_mut(&mut self, id: NodeId) -> Option<&mut Slot> {
        self.views.get_mut(id as usize).and_then(Option::as_mut)
    }

    /// Returns true if animation values or bindings changed.
    fn apply_item(&mut self, item: MountItem) -> bool {
        match item {
            MountItem::Create { id, kind } => {
                let native = match kind {
                    ViewKind::View => Native::View(UIView::new(self.mtm)),
                    ViewKind::Text => Native::Text(TextView::new(self.mtm)),
                    ViewKind::Image => {
                        let image = UIImageView::new(self.mtm);
                        image.setClipsToBounds(true);
                        image.setContentMode(UIViewContentMode::ScaleAspectFill);
                        Native::Image(image)
                    }
                    ViewKind::Scroll => {
                        let scroll = UIScrollView::new(self.mtm);
                        // Layout already accounts for safe areas.
                        scroll.setContentInsetAdjustmentBehavior(
                            UIScrollViewContentInsetAdjustmentBehavior::Never,
                        );
                        scroll.setAlwaysBounceVertical(true);
                        // SAFETY: the delegate (target) lives as long as the
                        // mounter, which owns the scroll view.
                        unsafe {
                            scroll.setDelegate(Some(ProtocolObject::from_ref(&*self.target)))
                        };
                        Native::Scroll(scroll)
                    }
                };
                native.view().setTag(id as isize);
                let idx = id as usize;
                if idx >= self.views.len() {
                    self.views
                        .resize_with((idx + 1).next_power_of_two(), || None);
                }
                self.views[idx] = Some(Slot::new(native));
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                if let (Some(p), Some(c)) = (self.slot(parent), self.slot(child)) {
                    let (p, c) = (p.native.view(), c.native.view());
                    match before.and_then(|b| self.slot(b)) {
                        Some(b) => p.insertSubview_belowSubview(c, b.native.view()),
                        None => p.addSubview(c),
                    }
                }
            }
            MountItem::Remove { id } => {
                if let Some(s) = self.slot(id) {
                    s.native.view().removeFromSuperview();
                }
            }
            MountItem::Delete { id } => {
                if let Some(s) = self.views.get_mut(id as usize).and_then(Option::take) {
                    s.native.view().removeFromSuperview();
                    if let Some(key) = &s.image_key {
                        self.images.cancel(id, key);
                    }
                    self.animated.retain(|&v| v != id);
                }
            }
            MountItem::Frame { id, frame } => {
                if let Some(s) = self.slot(id) {
                    set_frame(s.native.view(), frame);
                    if let Some(Refresh::Custom { spinner, .. }) = &s.gestures.refresh {
                        place_spinner(spinner, frame.width);
                    }
                }
            }
            MountItem::ViewStyle { id, style } => {
                let animator = &self.animator;
                if let Some(s) = self.views.get_mut(id as usize).and_then(Option::as_mut) {
                    s.style = style;
                    apply_static_style(s.native.view(), &style);
                    apply_dynamic(s, animator);
                }
            }
            MountItem::Text {
                id,
                layout,
                content,
                ..
            } => {
                if let Some(Slot {
                    native: Native::Text(view),
                    ..
                }) = self.slot(id)
                {
                    view.set_layout(layout, content);
                }
            }
            MountItem::Events { id, events } => self.set_events(id, events),
            MountItem::Image {
                id,
                source,
                resize_mode,
            } => self.set_image(id, source, resize_mode),
            MountItem::ScrollConfig {
                id,
                horizontal,
                shows_indicator,
                scroll_value,
            } => {
                if let Some(s) = self.slot_mut(id)
                    && let Native::Scroll(scroll) = &s.native
                {
                    scroll.setAlwaysBounceVertical(!horizontal);
                    scroll.setAlwaysBounceHorizontal(horizontal);
                    scroll.setShowsVerticalScrollIndicator(shows_indicator && !horizontal);
                    scroll.setShowsHorizontalScrollIndicator(shows_indicator && horizontal);
                    s.horizontal = horizontal;
                    s.scroll_value = scroll_value;
                }
            }
            MountItem::ScrollContent { id, size } => {
                if let Some(Slot {
                    native: Native::Scroll(scroll),
                    ..
                }) = self.slot(id)
                {
                    scroll
                        .setContentSize(CGSize::new(size.width as CGFloat, size.height as CGFloat));
                }
            }
            MountItem::ScrollBy { id, dx, dy } => {
                if let Some(Slot {
                    native: Native::Scroll(scroll),
                    ..
                }) = self.slot(id)
                {
                    let o = scroll.contentOffset();
                    scroll.setContentOffset(CGPoint::new(o.x + dx as CGFloat, o.y + dy as CGFloat));
                }
            }
            MountItem::Refreshing { id, refreshing } => self.set_refreshing(id, refreshing),
            MountItem::AnimBindings { id, bindings } => {
                if let Some(s) = self.slot_mut(id) {
                    // Properties no longer animated fall back to the static style.
                    s.bindings = bindings;
                    let bound = !s.bindings.is_empty();
                    self.animated.retain(|&v| v != id);
                    if bound {
                        self.animated.push(id);
                    } else {
                        let animator = &self.animator;
                        if let Some(s) = self.views.get(id as usize).and_then(Option::as_ref) {
                            apply_dynamic(s, animator);
                        }
                    }
                    return true;
                }
            }
            MountItem::PanValues { id, x, y } => {
                if let Some(s) = self.slot_mut(id) {
                    s.pan = (x, y);
                    let events = s.events;
                    self.set_events(id, events);
                }
            }
            MountItem::Anim(op) => {
                self.animator.apply(op);
                return true;
            }
        }
        false
    }

    fn set_image(&mut self, id: NodeId, source: Option<ImageSource>, resize_mode: ResizeMode) {
        let Some(s) = self.slot_mut(id) else { return };
        let Native::Image(view) = &s.native else {
            return;
        };
        view.setContentMode(match resize_mode {
            ResizeMode::Cover => UIViewContentMode::ScaleAspectFill,
            ResizeMode::Contain => UIViewContentMode::ScaleAspectFit,
            ResizeMode::Stretch => UIViewContentMode::ScaleToFill,
            ResizeMode::Center => UIViewContentMode::Center,
        });
        if s.image.as_ref().map(|src| &src.uri) == source.as_ref().map(|src| &src.uri) {
            return;
        }
        view.setImage(None);
        let old_key = s.image_key.take();
        s.image = source;
        let wants = s.image.is_some();
        if let Some(key) = old_key {
            self.images.cancel(id, &key);
        }
        if wants {
            self.pending_images.push(id);
        }
    }

    /// Starts loads queued during the batch, now that frames are known so
    /// images can be decoded at their display size.
    fn start_image_loads(&mut self) {
        let scale = self
            .slot(yf_core::ROOT_ID)
            .map_or(2.0, |s| s.native.view().contentScaleFactor());
        for id in std::mem::take(&mut self.pending_images) {
            let Some(s) = self.slot(id) else { continue };
            let (Some(source), Native::Image(view)) = (&s.image, &s.native) else {
                continue;
            };
            let size = view.bounds().size;
            let max_pixels = (size.width.max(size.height) * scale).ceil() as u32;
            let uri = source.uri.clone();
            let shared = self.shared.clone();
            let done = move |key: String, result: Result<DecodedImage, String>| {
                let mtm = MainThreadMarker::new().expect("main queue");
                if let Some(m) = shared.get(mtm).borrow_mut().as_mut() {
                    m.image_loaded(&key, result);
                }
            };
            match self.images.request(id, &uri, max_pixels, scale, done) {
                Request::Ready(image) => {
                    if let Some(s) = self.slot(id)
                        && let Native::Image(view) = &s.native
                    {
                        view.setImage(Some(&image));
                    }
                    self.image_event(id, Ok(&image));
                }
                Request::Pending => {
                    if let Some(s) = self.slot_mut(id) {
                        s.image_key = Some(ImageLoader::key(&uri, max_pixels));
                    }
                }
            }
        }
    }

    pub(crate) fn image_loaded(&mut self, key: &str, result: Result<DecodedImage, String>) {
        for id in self.images.finish(key, &result) {
            let Some(s) = self.slot_mut(id) else { continue };
            if s.image_key.as_deref() != Some(key) {
                continue;
            }
            s.image_key = None;
            match &result {
                Ok(DecodedImage(image)) => {
                    if let Native::Image(view) = &s.native {
                        view.setImage(Some(image));
                    }
                    self.image_event(id, Ok(image));
                }
                Err(message) => {
                    log::warn!("image failed: {message}");
                    self.image_event(id, Err(message));
                }
            }
        }
    }

    fn image_event(&self, id: NodeId, result: Result<&UIImage, &String>) {
        let Some(s) = self.slot(id) else { return };
        match result {
            Ok(image) if s.events.contains(EventKind::Load) => {
                // SAFETY: plain getter on a valid image.
                let size = unsafe { image.size() };
                self.control.dispatch_event(
                    id,
                    EventKind::Load,
                    EventPayload::Image {
                        width: size.width as f32,
                        height: size.height as f32,
                    },
                );
            }
            Err(message) if s.events.contains(EventKind::Error) => {
                self.control.dispatch_event(
                    id,
                    EventKind::Error,
                    EventPayload::Error {
                        message: message.clone(),
                    },
                );
            }
            _ => {}
        }
    }

    fn set_events(&mut self, id: NodeId, events: EventMask) {
        let target = self.target.clone();
        let (mtm, mac_idiom) = (self.mtm, self.mac_idiom);
        let Some(slot) = self.views.get_mut(id as usize).and_then(Option::as_mut) else {
            return;
        };
        slot.events = events;
        let wants_pan = slot.pan != (None, None)
            || events.contains(EventKind::PanStart)
            || events.contains(EventKind::PanEnd);
        let view = slot.native.view();
        let g = &mut slot.gestures;
        // SAFETY: every selector is implemented by `EventTarget`, which the
        // mounter keeps alive as long as the recognizers.
        unsafe {
            toggle(&mut g.tap, events.contains(EventKind::Press), view, || {
                UITapGestureRecognizer::initWithTarget_action(
                    UITapGestureRecognizer::alloc(mtm),
                    Some(&target),
                    Some(sel!(handleTap:)),
                )
            });
            toggle(
                &mut g.long_press,
                events.contains(EventKind::LongPress),
                view,
                || {
                    UILongPressGestureRecognizer::initWithTarget_action(
                        UILongPressGestureRecognizer::alloc(mtm),
                        Some(&target),
                        Some(sel!(handleLongPress:)),
                    )
                },
            );
            toggle(&mut g.pan, wants_pan, view, || {
                UIPanGestureRecognizer::initWithTarget_action(
                    UIPanGestureRecognizer::alloc(mtm),
                    Some(&target),
                    Some(sel!(handlePan:)),
                )
            });
        }
        if g.tap.is_some() || g.long_press.is_some() || g.pan.is_some() {
            view.setUserInteractionEnabled(true);
        }
        if let Native::Scroll(scroll) = &slot.native {
            let wants_refresh = events.contains(EventKind::Refresh);
            match (&slot.gestures.refresh, wants_refresh) {
                (None, true) if mac_idiom => {
                    let spinner = UIActivityIndicatorView::initWithActivityIndicatorStyle(
                        UIActivityIndicatorView::alloc(mtm),
                        UIActivityIndicatorViewStyle::Medium,
                    );
                    spinner.setHidesWhenStopped(true);
                    place_spinner(&spinner, scroll.bounds().size.width as f32);
                    scroll.addSubview(&spinner);
                    slot.gestures.refresh = Some(Refresh::Custom {
                        spinner,
                        refreshing: false,
                    });
                }
                (None, true) => {
                    let control = UIRefreshControl::new(mtm);
                    // SAFETY: `handleRefresh:` is implemented by the target.
                    unsafe {
                        control.addTarget_action_forControlEvents(
                            Some(&target),
                            sel!(handleRefresh:),
                            UIControlEvents::ValueChanged,
                        )
                    };
                    scroll.setRefreshControl(Some(&control));
                    slot.gestures.refresh = Some(Refresh::Native(control));
                }
                (Some(refresh), false) => {
                    match refresh {
                        Refresh::Native(_) => scroll.setRefreshControl(None),
                        Refresh::Custom { spinner, .. } => spinner.removeFromSuperview(),
                    }
                    slot.gestures.refresh = None;
                }
                _ => {}
            }
        }
    }

    /// A drag on a view: moves its pan values natively and reports start
    /// and end (with velocity) to JS.
    pub(crate) fn pan(
        &mut self,
        id: NodeId,
        state: UIGestureRecognizerState,
        t: CGPoint,
        v: CGPoint,
    ) {
        let Some(s) = self.slot(id) else { return };
        let (pan, events) = (s.pan, s.events);
        let payload = EventPayload::Pan {
            translation_x: t.x as f32,
            translation_y: t.y as f32,
            velocity_x: v.x as f32,
            velocity_y: v.y as f32,
        };
        if state == UIGestureRecognizerState::Began {
            // Grabbing an animating value stops it where it is.
            let start = (
                pan.0.map_or(0.0, |n| self.animator.value(n)),
                pan.1.map_or(0.0, |n| self.animator.value(n)),
            );
            if let Some(s) = self.slot_mut(id) {
                s.pan_start = start;
            }
            if events.contains(EventKind::PanStart) {
                self.control
                    .dispatch_event(id, EventKind::PanStart, payload.clone());
            }
        }
        if matches!(
            state,
            UIGestureRecognizerState::Began | UIGestureRecognizerState::Changed
        ) {
            let start = self.slot(id).map_or((0.0, 0.0), |s| s.pan_start);
            if let Some(n) = pan.0 {
                self.animator.set(n, start.0 + t.x as f32);
            }
            if let Some(n) = pan.1 {
                self.animator.set(n, start.1 + t.y as f32);
            }
            self.apply_animations();
        }
        if matches!(
            state,
            UIGestureRecognizerState::Ended | UIGestureRecognizerState::Cancelled
        ) && events.contains(EventKind::PanEnd)
        {
            self.control.dispatch_event(id, EventKind::PanEnd, payload);
        }
        self.report_finished();
    }

    fn set_refreshing(&mut self, id: NodeId, on: bool) {
        let mtm = self.mtm;
        let Some(s) = self.slot_mut(id) else { return };
        let Native::Scroll(scroll) = &s.native else {
            return;
        };
        match &mut s.gestures.refresh {
            Some(Refresh::Native(control)) if control.isRefreshing() != on => {
                if on {
                    control.beginRefreshing();
                } else {
                    control.endRefreshing();
                }
            }
            Some(Refresh::Custom {
                spinner,
                refreshing,
            }) if *refreshing != on => {
                *refreshing = on;
                let mut inset = scroll.contentInset();
                inset.top = if on { REFRESH_INSET } else { 0.0 };
                if on {
                    spinner.startAnimating();
                    scroll.setContentInset(inset);
                    // Reveal the spinner if the list is at the top. On the
                    // next turn: a release past the top has already started
                    // its bounce towards the old inset, which this replaces.
                    let scroll = MainThreadBound::new(scroll.clone(), mtm);
                    DispatchQueue::main().exec_async(move || {
                        let mtm = MainThreadMarker::new().expect("main queue");
                        let scroll = scroll.get(mtm);
                        let offset = scroll.contentOffset();
                        if offset.y <= 0.0 && !scroll.isDragging() {
                            scroll.setContentOffset_animated(
                                CGPoint::new(offset.x, -REFRESH_INSET),
                                true,
                            );
                        }
                    });
                } else {
                    spinner.stopAnimating();
                    let scroll = scroll.clone();
                    let block = block2::RcBlock::new(move || scroll.setContentInset(inset));
                    UIView::animateWithDuration_animations(0.25, &block, mtm);
                }
            }
            _ => {}
        }
    }

    /// The user released a drag: a far enough pull past the top starts a
    /// custom (Mac idiom) refresh.
    pub(crate) fn drag_ended(&mut self, id: NodeId, offset: CGPoint) {
        let Some(s) = self.slot(id) else { return };
        if let Some(Refresh::Custom {
            refreshing: false, ..
        }) = s.gestures.refresh
            && offset.y <= -REFRESH_PULL
        {
            self.set_refreshing(id, true);
            self.control
                .dispatch_event(id, EventKind::Refresh, EventPayload::None);
        }
    }

    /// A scroll view moved: feeds its scroll value in the same frame.
    pub(crate) fn scrolled(&mut self, id: NodeId, offset: CGPoint) {
        let Some(s) = self.slot(id) else { return };
        if let Some(node) = s.scroll_value {
            let value = if s.horizontal { offset.x } else { offset.y };
            self.animator.set(node, value as f32);
            self.apply_animations();
            self.report_finished();
        }
    }

    /// Display link frame.
    pub(crate) fn tick(&mut self) {
        if self.animator.tick() {
            self.apply_animations();
        }
        self.report_finished();
    }

    fn apply_animations(&mut self) {
        let animator = &self.animator;
        for &id in &self.animated {
            if let Some(s) = self.views.get(id as usize).and_then(Option::as_ref) {
                apply_dynamic(s, animator);
            }
        }
    }

    fn report_finished(&mut self) {
        let finished = self.animator.take_finished();
        if !finished.is_empty() {
            self.control.animations_finished(finished);
        }
    }
}

/// Adds or removes a gesture recognizer.
fn toggle<G: AsRef<UIGestureRecognizer>>(
    slot: &mut Option<Retained<G>>,
    wanted: bool,
    view: &UIView,
    make: impl FnOnce() -> Retained<G>,
) {
    match (slot.as_ref(), wanted) {
        (None, true) => {
            let recognizer = make();
            view.addGestureRecognizer((*recognizer).as_ref());
            *slot = Some(recognizer);
        }
        (Some(recognizer), false) => {
            view.removeGestureRecognizer((**recognizer).as_ref());
            *slot = None;
        }
        _ => {}
    }
}

/// Sets a frame through `bounds` + `center`: `frame` is undefined for
/// transformed views, and a scroll view's bounds origin is its scroll
/// offset, which must be kept.
fn set_frame(view: &UIView, frame: Rect) {
    let origin = view.bounds().origin;
    view.setBounds(CGRect::new(
        origin,
        CGSize::new(frame.width as CGFloat, frame.height as CGFloat),
    ));
    view.setCenter(CGPoint::new(
        (frame.x + frame.width / 2.0) as CGFloat,
        (frame.y + frame.height / 2.0) as CGFloat,
    ));
}

fn ui_color(color: Color) -> Retained<UIColor> {
    let (r, g, b, a) = color.to_f64();
    UIColor::colorWithRed_green_blue_alpha(r, g, b, a)
}

/// Properties that never animate natively. Image views always clip.
fn apply_static_style(view: &UIView, style: &ViewStyle) {
    let layer = view.layer();
    layer.setCornerRadius(style.border_radius as CGFloat);
    layer.setBorderWidth(style.border_width as CGFloat);
    if style.border_width > 0.0 {
        // SAFETY: the returned CGColor is retained for the call.
        let color = unsafe { ui_color(style.border_color).CGColor() };
        layer.setBorderColor(Some(&color));
    }
    if style.overflow_hidden {
        view.setClipsToBounds(true);
    }
}

/// Opacity, background color and transform: static values overridden by
/// animated bindings.
fn apply_dynamic(slot: &Slot, animator: &Animator) {
    let view = slot.native.view();
    let style = &slot.style;
    let mut opacity = style.opacity;
    let mut background = style.background_color;
    let mut t = style.transform;
    for &(prop, node) in &slot.bindings {
        let value = animator.eval(node);
        let n = value.number();
        match prop {
            AnimProp::Opacity => opacity = n.clamp(0.0, 1.0),
            AnimProp::TranslateX => t.translate_x = n,
            AnimProp::TranslateY => t.translate_y = n,
            AnimProp::Scale => (t.scale_x, t.scale_y) = (n, n),
            AnimProp::ScaleX => t.scale_x = n,
            AnimProp::ScaleY => t.scale_y = n,
            AnimProp::Rotate => t.rotate = n,
            AnimProp::BackgroundColor => {
                if let AnimValue::Color(c) = value {
                    background = Color(c);
                }
            }
        }
    }
    view.setAlpha(opacity as CGFloat);
    if background.is_transparent() {
        view.setBackgroundColor(None);
    } else {
        view.setBackgroundColor(Some(&ui_color(background)));
    }
    // Scale, then rotate, then translate (React Native order), around the
    // view's center.
    let (sin, cos) = (t.rotate as CGFloat).to_radians().sin_cos();
    let (sx, sy) = (t.scale_x as CGFloat, t.scale_y as CGFloat);
    view.setTransform(CGAffineTransform {
        a: sx * cos,
        b: sx * sin,
        c: -sy * sin,
        d: sy * cos,
        tx: t.translate_x as CGFloat,
        ty: t.translate_y as CGFloat,
    });
}
