//! Applies mount batches to UIKit views and owns the per-surface native
//! state: views, animation graph, image loads and gesture recognizers.

use std::{cell::RefCell, sync::Arc};

use dispatch2::{DispatchQueue, MainThreadBound};
use objc2::{
    MainThreadMarker, MainThreadOnly, Message, rc::Retained, runtime::ProtocolObject, sel,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_quartz_core::CATransform3D;
use objc2_ui_kit::{
    UIActivityIndicatorView, UIActivityIndicatorViewStyle, UIControlEvents, UIDevice,
    UIGestureRecognizer, UIGestureRecognizerState, UIImage, UIPanGestureRecognizer,
    UIPinchGestureRecognizer, UIRefreshControl, UIRotationGestureRecognizer, UIScrollView,
    UIScrollViewContentInsetAdjustmentBehavior, UIScrollViewKeyboardDismissMode,
    UIUserInterfaceIdiom, UIView, UIViewAnimationOptions,
};
use yf_anim::{AnimProp, AnimValue};
use yf_core::{
    Color, Command, EventKind, EventMask, EventPayload, GestureValues, ImageSource,
    KeyboardDismissMode, LayoutAnimation, LayoutCurve, LayoutProperty, LayoutTransition,
    MountBatch, MountItem, NodeId, PayloadValue, Rect, ResizeMode, StickyCell, ViewKind, ViewProps,
    ViewStyle,
};
use yf_engine::EngineControl;

use crate::{
    anim::Animator,
    decor::{self, Decor},
    image::{AssetBase, DecodedImage, ImageLoader, Request},
    input::Input,
    native_view::YFView,
    navigation::{ModalHost, ScreenHost, StackHost},
    press::PressRecognizer,
    target::EventTarget,
    text_view::TextView,
    video::VideoHost,
    web::WebHost,
};

/// The mounter of one surface, shared with native callbacks.
pub(crate) type SharedMounter = Arc<MainThreadBound<RefCell<Option<Mounter>>>>;

enum Native {
    /// The surface root (owned by the platform).
    Root(Retained<UIView>),
    View(Retained<YFView>),
    Text(Retained<TextView>),
    /// The image itself is in [`Decor`]'s image layer.
    Image(Retained<YFView>),
    Scroll(Retained<UIScrollView>),
    Input(Box<Input>),
    Stack(Box<StackHost>),
    Screen(Box<ScreenHost>),
    Modal(Box<ModalHost>),
    HeaderItem(Retained<YFView>),
    Switch(Retained<objc2_ui_kit::UISwitch>),
    /// With its step.
    Slider(Retained<objc2_ui_kit::UISlider>, f32),
    Activity(Retained<UIActivityIndicatorView>),
    Web(Box<WebHost>),
    Video(Box<VideoHost>),
}

impl Native {
    #[inline]
    fn view(&self) -> &UIView {
        match self {
            Native::Root(v) => v,
            Native::View(v) => v,
            Native::Text(t) => t,
            Native::Image(i) => i,
            Native::Scroll(s) => s,
            Native::Input(i) => i.view(),
            Native::Stack(s) => &s.container,
            Native::Screen(s) => &s.view,
            Native::Modal(m) => &m.view,
            Native::HeaderItem(v) => v,
            Native::Switch(s) => s,
            Native::Slider(s, _) => s,
            Native::Activity(a) => a,
            Native::Web(w) => &w.view,
            Native::Video(v) => &v.view,
        }
    }

    /// The view as a `YFView`, for views with app touch and accessibility
    /// properties.
    fn yf(&self) -> Option<&YFView> {
        match self {
            Native::View(v) | Native::Image(v) | Native::HeaderItem(v) => Some(v),
            Native::Text(t) => Some(t),
            Native::Stack(s) => Some(&s.container),
            Native::Screen(s) => Some(&s.view),
            Native::Modal(m) => Some(&m.view),
            Native::Web(w) => Some(&w.view),
            Native::Video(v) => Some(&v.view),
            _ => None,
        }
    }

    fn kind(&self) -> decor::Kind {
        match self {
            Native::Root(_)
            | Native::View(_)
            | Native::Input(_)
            | Native::Stack(_)
            | Native::Screen(_)
            | Native::Modal(_)
            | Native::HeaderItem(_)
            | Native::Switch(_)
            | Native::Slider(..)
            | Native::Activity(_)
            | Native::Web(_)
            | Native::Video(_) => decor::Kind::View,
            Native::Text(_) => decor::Kind::Text,
            Native::Image(_) => decor::Kind::Image,
            Native::Scroll(_) => decor::Kind::Scroll,
        }
    }
}

#[derive(Default)]
struct Gestures {
    press: Option<Retained<PressRecognizer>>,
    pan: Option<Retained<UIPanGestureRecognizer>>,
    pinch: Option<Retained<UIPinchGestureRecognizer>>,
    rotation: Option<Retained<UIRotationGestureRecognizer>>,
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

fn ui_insets(i: yf_core::EdgeInsets) -> objc2_ui_kit::UIEdgeInsets {
    objc2_ui_kit::UIEdgeInsets {
        top: i.top as CGFloat,
        left: i.left as CGFloat,
        bottom: i.bottom as CGFloat,
        right: i.right as CGFloat,
    }
}

fn set_bottom_inset(scroll: &UIScrollView, bottom: CGFloat, duration: f64, mtm: MainThreadMarker) {
    let scroll = scroll.retain();
    let block = block2::RcBlock::new(move || {
        let mut inset = scroll.contentInset();
        inset.bottom = bottom;
        scroll.setContentInset(inset);
        let mut indicators = scroll.verticalScrollIndicatorInsets();
        indicators.bottom = bottom;
        scroll.setVerticalScrollIndicatorInsets(indicators);
    });
    UIView::animateWithDuration_animations(duration, &block, mtm);
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
    decor: Decor,
    events: EventMask,
    gestures: Gestures,
    bindings: Vec<(AnimProp, yf_anim::NodeId)>,
    values: GestureValues,
    /// Values of the gesture nodes when the gesture began: pan x, pan y,
    /// pinch, rotation.
    gesture_start: [f32; 4],
    /// `activeOpacity`: opacity while pressed.
    active_opacity: Option<f32>,
    pressed: bool,
    scroll_value: Option<yf_anim::NodeId>,
    /// How a scroll view scrolls (scroll views only).
    behavior: Option<Box<yf_core::ScrollBehavior>>,
    horizontal: bool,
    /// Image to show; loaded once the frame is known.
    image: Option<ImageSource>,
    /// The load in flight for this view.
    image_key: Option<String>,
}

impl Slot {
    fn new(native: Native) -> Self {
        Self {
            decor: match native {
                Native::Image(_) => Decor::for_image(),
                _ => Decor::default(),
            },
            native,
            style: ViewStyle::default(),
            events: EventMask::default(),
            gestures: Gestures::default(),
            bindings: Vec::new(),
            values: GestureValues::default(),
            gesture_start: [0.0; 4],
            active_opacity: None,
            pressed: false,
            scroll_value: None,
            behavior: None,
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
    /// The focused input.
    focused: Option<NodeId>,
    /// Height of the software keyboard over the surface.
    keyboard: f32,
    /// The scroll view given a bottom inset to clear the keyboard.
    avoiding: Option<NodeId>,
    /// Animates the frames, insertions and removals of the current batch.
    layout_animation: Option<LayoutAnimation>,
    /// Views created by the current batch (they appear, not move).
    created: Vec<NodeId>,
    /// Views fading out before their removal.
    disappearing: rustc_hash::FxHashSet<NodeId>,
    /// One-shot display link for `requestAnimationFrame`.
    frame_link: Retained<objc2_quartz_core::CADisplayLink>,
    /// Parents of screens, header items and modals (which are not
    /// subviews of their parent).
    hosts: rustc_hash::FxHashMap<NodeId, NodeId>,
    /// Stacks whose screens changed in the current batch.
    dirty_stacks: Vec<NodeId>,
    /// Stacks whose screens changed during a push or pop transition: they
    /// catch up when it ends (UIKit breaks transitions that start while
    /// another one is running).
    deferred_stacks: Vec<NodeId>,
    /// Screens whose header changed in the current batch.
    dirty_headers: Vec<NodeId>,
    /// Modals to present or dismiss at the end of the current batch.
    dirty_modals: Vec<NodeId>,
    /// Where `asset:/` URIs of videos resolve.
    asset_base: Option<AssetBase>,
    /// Sticky cells of lists, by list.
    sticky: rustc_hash::FxHashMap<NodeId, Vec<StickyCell>>,
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
        let target_for_link = target.clone();
        let mut views = Vec::with_capacity(256);
        views.resize_with(2, || None);
        root.setTag(yf_core::ROOT_ID as isize);
        views[yf_core::ROOT_ID as usize] = Some(Slot::new(Native::Root(root)));
        Self {
            views,
            animated: Vec::new(),
            pending_images: Vec::new(),
            animator: Animator::new(&target),
            target,
            images: ImageLoader::new(asset_base.clone()),
            control,
            shared,
            mac_idiom: UIDevice::currentDevice(mtm).userInterfaceIdiom()
                == UIUserInterfaceIdiom::Mac,
            focused: None,
            keyboard: 0.0,
            avoiding: None,
            layout_animation: None,
            created: Vec::new(),
            disappearing: Default::default(),
            frame_link: crate::anim::frame_link(&target_for_link),
            hosts: Default::default(),
            dirty_stacks: Vec::new(),
            deferred_stacks: Vec::new(),
            dirty_headers: Vec::new(),
            dirty_modals: Vec::new(),
            asset_base: asset_base.clone(),
            sticky: Default::default(),
            mtm,
        }
    }

    pub(crate) fn apply(&mut self, batch: MountBatch) {
        let mut anim_changed = false;
        for item in batch.items {
            anim_changed |= self.apply_item(item);
        }
        if let Some(animation) = self.layout_animation.take() {
            self.animate_appearing(&animation);
        }
        self.created.clear();
        self.finish_navigation();
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
                    ViewKind::View => Native::View(YFView::new(self.mtm)),
                    ViewKind::Text => Native::Text(TextView::new(self.mtm)),
                    ViewKind::Image => Native::Image(YFView::new(self.mtm)),
                    ViewKind::Input => {
                        Native::Input(Box::new(Input::new(self.mtm, id, &self.target)))
                    }
                    ViewKind::Stack => {
                        Native::Stack(Box::new(StackHost::new(self.mtm, id, self.shared.clone())))
                    }
                    ViewKind::Screen => Native::Screen(Box::new(ScreenHost::new(
                        self.mtm,
                        id,
                        self.control.clone(),
                    ))),
                    ViewKind::Modal => {
                        Native::Modal(Box::new(ModalHost::new(self.mtm, id, self.control.clone())))
                    }
                    ViewKind::HeaderItem => Native::HeaderItem(YFView::new(self.mtm)),
                    ViewKind::Switch => {
                        Native::Switch(crate::controls::new_switch(self.mtm, &self.target))
                    }
                    ViewKind::Slider => {
                        Native::Slider(crate::controls::new_slider(self.mtm, &self.target), 0.0)
                    }
                    ViewKind::ActivityIndicator => {
                        Native::Activity(crate::controls::new_activity(self.mtm))
                    }
                    ViewKind::WebView => {
                        Native::Web(Box::new(WebHost::new(self.mtm, id, self.control.clone())))
                    }
                    ViewKind::Video => Native::Video(Box::new(VideoHost::new(
                        self.mtm,
                        id,
                        self.control.clone(),
                        self.asset_base.clone(),
                    ))),
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
                        // Taps that dismiss the keyboard
                        // (`keyboardShouldPersistTaps`).
                        let tap = crate::target::keyboard_tap_recognizer(self.mtm, &self.target);
                        scroll.addGestureRecognizer(&tap);
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
                if self.layout_animation.is_some() {
                    self.created.push(id);
                }
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                if self.insert_hosted(parent, child, before) {
                    return false;
                }
                if self.disappearing.remove(&child)
                    && let Some(c) = self.slot(child)
                {
                    // Back before its removal finished animating.
                    c.native.view().layer().removeAllAnimations();
                    apply_dynamic(c, &self.animator);
                }
                if let (Some(p), Some(c)) = (self.slot(parent), self.slot(child)) {
                    let (p, c) = (p.decor.container(p.native.view()), c.native.view());
                    match before.and_then(|b| self.slot(b)) {
                        Some(b) => p.insertSubview_belowSubview(c, b.native.view()),
                        None => p.addSubview(c),
                    }
                }
                if self.sticky.contains_key(&parent) {
                    // New cells must not cover the pinned ones.
                    self.front_sticky(parent);
                }
            }
            MountItem::Remove { id } => {
                if self.remove_hosted(id) {
                    return false;
                }
                if let Some(s) = self.slot(id) {
                    match self.layout_animation.and_then(|a| a.delete) {
                        Some(t) => {
                            disappear(s.native.view(), id, &t, self.shared.clone(), self.mtm);
                            self.disappearing.insert(id);
                        }
                        None => s.native.view().removeFromSuperview(),
                    }
                }
            }
            MountItem::Delete { id } => {
                self.remove_hosted(id);
                self.sticky.remove(&id);
                self.target.scroll_deleted(id);
                if let Some(s) = self.views.get_mut(id as usize).and_then(Option::take) {
                    s.native.view().removeFromSuperview();
                    if let Some(key) = &s.image_key {
                        self.images.cancel(id, key);
                    }
                    self.animated.retain(|&v| v != id);
                }
            }
            MountItem::Frame { id, frame } => {
                if let Some(Slot {
                    native: Native::HeaderItem(view),
                    ..
                }) = self.slot(id)
                {
                    // The header bar positions it; only its size is ours.
                    let origin = view.frame().origin;
                    view.setFrame(CGRect::new(
                        origin,
                        CGSize::new(frame.width as CGFloat, frame.height as CGFloat),
                    ));
                    return false;
                }
                if let Some(s) = self.slot(id) {
                    let view = s.native.view();
                    let update = self.layout_animation.and_then(|a| a.update);
                    match update {
                        // Views that already had a frame move; new ones appear.
                        Some(t)
                            if !self.created.contains(&id)
                                && view.bounds().size != CGSize::ZERO =>
                        {
                            let view = view.retain();
                            animate(&t, self.mtm, move || set_frame(&view, frame));
                        }
                        _ => set_frame(view, frame),
                    }
                    if let Some(Refresh::Custom { spinner, .. }) = &s.gestures.refresh {
                        place_spinner(spinner, frame.width);
                    }
                    if let Native::Video(video) = &s.native {
                        video.layout();
                    }
                    if s.decor.sized() {
                        self.decorate(id);
                    }
                }
            }
            MountItem::ViewStyle { id, style } => {
                if let Some(s) = self.slot_mut(id) {
                    s.style = *style;
                    self.decorate(id);
                    if let Some(s) = self.slot(id) {
                        apply_dynamic(s, &self.animator);
                    }
                }
            }
            MountItem::Text {
                id,
                text,
                layout,
                content,
            } => {
                if let Some(Slot {
                    native: Native::Text(view),
                    ..
                }) = self.slot(id)
                {
                    view.set_layout(text, layout, content);
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
                keyboard_dismiss,
            } => {
                if let Some(s) = self.slot_mut(id)
                    && let Native::Scroll(scroll) = &s.native
                {
                    scroll.setKeyboardDismissMode(match keyboard_dismiss {
                        KeyboardDismissMode::None => UIScrollViewKeyboardDismissMode::None,
                        KeyboardDismissMode::OnDrag => UIScrollViewKeyboardDismissMode::OnDrag,
                        KeyboardDismissMode::Interactive => {
                            UIScrollViewKeyboardDismissMode::Interactive
                        }
                    });
                    let bounces = s.behavior.as_ref().is_none_or(|b| b.bounces);
                    scroll.setAlwaysBounceVertical(bounces && !horizontal);
                    scroll.setAlwaysBounceHorizontal(bounces && horizontal);
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
                    self.target.scroll_applied(id);
                    let o = scroll.contentOffset();
                    scroll.setContentOffset(CGPoint::new(o.x + dx as CGFloat, o.y + dy as CGFloat));
                }
            }
            MountItem::StickyCells { id, cells } => {
                let identity = CATransform3D::new_translation(0.0, 0.0, 0.0);
                for old in self.sticky.remove(&id).unwrap_or_default() {
                    if !cells.iter().any(|c| c.cell == old.cell)
                        && let Some(s) = self.slot(old.cell)
                    {
                        s.native.view().layer().setTransform(identity);
                    }
                }
                if !cells.is_empty() {
                    self.sticky.insert(id, cells);
                    self.front_sticky(id);
                    self.pin_sticky(id);
                }
            }
            MountItem::ScrollBehavior { id, behavior } => {
                if let Some(s) = self.slot_mut(id)
                    && let Native::Scroll(scroll) = &s.native
                {
                    let b = &behavior;
                    // Native paging pages by the viewport; other snapping is
                    // done when a drag ends (`snap_target`).
                    scroll.setPagingEnabled(b.paging && !b.snaps());
                    let rate = if b.deceleration > 0.0 {
                        b.deceleration
                    } else if b.snaps() {
                        0.99
                    } else {
                        0.998
                    };
                    scroll.setDecelerationRate(rate as CGFloat);
                    scroll.setScrollEnabled(b.enabled);
                    scroll.setBounces(b.bounces);
                    scroll.setAlwaysBounceVertical(b.bounces && !s.horizontal);
                    scroll.setAlwaysBounceHorizontal(b.bounces && s.horizontal);
                    // The keyboard and refreshing add to the content inset.
                    let old = ui_insets(
                        s.behavior
                            .as_ref()
                            .map(|b| b.content_inset)
                            .unwrap_or_default(),
                    );
                    let new = ui_insets(b.content_inset);
                    let mut inset = scroll.contentInset();
                    inset.top += new.top - old.top;
                    inset.left += new.left - old.left;
                    inset.bottom += new.bottom - old.bottom;
                    inset.right += new.right - old.right;
                    scroll.setContentInset(inset);
                    s.behavior = Some(behavior);
                }
            }
            MountItem::Refreshing { id, refreshing } => self.set_refreshing(id, refreshing),
            MountItem::AnimBindings { id, bindings } => {
                if let Some(s) = self.slot_mut(id) {
                    // Properties no longer animated fall back to the static style.
                    s.bindings = bindings;
                    let bound = !s.bindings.is_empty();
                    self.animated.retain(|&v| v != id);
                    // An animated background changes how the shadow is drawn.
                    self.decorate(id);
                    if bound {
                        self.animated.push(id);
                    } else if let Some(s) = self.slot(id) {
                        apply_dynamic(s, &self.animator);
                    }
                    return true;
                }
            }
            MountItem::Gestures { id, values } => {
                if let Some(s) = self.slot_mut(id) {
                    s.values = values;
                    let events = s.events;
                    self.set_events(id, events);
                }
            }
            MountItem::ViewProps { id, props } => self.set_view_props(id, *props),
            MountItem::LayoutAnimation(animation) => self.layout_animation = Some(animation),
            MountItem::ScreenConfig { id, config } => {
                if let Some(Slot {
                    native: Native::Screen(screen),
                    ..
                }) = self.slot_mut(id)
                {
                    screen.config = *config;
                    self.dirty_headers.push(id);
                    if let Some(&stack) = self.hosts.get(&id) {
                        self.dirty_stacks.push(stack);
                    }
                }
            }
            MountItem::NativeProps { id, props } => self.set_native_props(id, props),
            MountItem::ModalConfig { id, config } => {
                if let Some(Slot {
                    native: Native::Modal(modal),
                    ..
                }) = self.slot_mut(id)
                {
                    modal.config = config;
                }
            }
            MountItem::Anim(op) => {
                self.animator.apply(op);
                return true;
            }
            MountItem::Input { id, config } => {
                let (target, mtm) = (self.target.clone(), self.mtm);
                if let Some(Slot {
                    native: Native::Input(input),
                    ..
                }) = self.slot_mut(id)
                {
                    input.configure(*config, &target, mtm);
                }
            }
            MountItem::InputValue { id, text, count } => {
                if let Some(Slot {
                    native: Native::Input(input),
                    ..
                }) = self.slot_mut(id)
                {
                    input.set_value(&text, count);
                }
            }
            MountItem::InputContent { id, content } => {
                if let Some(Slot {
                    native: Native::Input(input),
                    ..
                }) = self.slot_mut(id)
                {
                    input.set_content(content);
                }
            }
            MountItem::Command { id, command } => self.command(id, command),
        }
        false
    }

    fn command(&mut self, id: NodeId, command: Command) {
        let Some(s) = self.slot(id) else { return };
        match (command, &s.native) {
            (
                c @ (Command::Reload
                | Command::GoBack
                | Command::GoForward
                | Command::StopLoading
                | Command::InjectJavaScript(_)
                | Command::PostMessage(_)),
                Native::Web(web),
            ) => web.command(&c),
            (Command::Seek(seconds), Native::Video(video)) => video.seek(seconds),
            (Command::Focus, Native::Input(input)) => {
                input.editor_view().becomeFirstResponder();
            }
            (Command::Blur, Native::Input(input)) => {
                input.editor_view().resignFirstResponder();
            }
            // On the root: whatever is focused.
            (Command::Blur, native) if id == yf_core::ROOT_ID => {
                native.view().endEditing(true);
            }
            (Command::ScrollTo { x, y, animated }, Native::Scroll(scroll)) => {
                let inset = scroll.adjustedContentInset();
                let size = scroll.contentSize();
                let bounds = scroll.bounds().size;
                let max_x = (size.width + inset.right - bounds.width).max(-inset.left);
                let max_y = (size.height + inset.bottom - bounds.height).max(-inset.top);
                let offset = CGPoint::new(
                    (x as CGFloat).clamp(-inset.left, max_x),
                    (y as CGFloat).clamp(-inset.top, max_y),
                );
                self.target.scroll_applied(id);
                scroll.setContentOffset_animated(offset, animated);
            }
            (Command::StatusBar(config), native) if id == yf_core::ROOT_ID => {
                crate::system::set_status_bar(config, native.view());
            }
            (Command::Dialog(dialog), native) if id == yf_core::ROOT_ID => {
                crate::system::show_dialog(&dialog, native.view(), &self.control);
            }
            // Toasts are Android only.
            (Command::Toast { .. }, _) => {}
            (command, _) => log::warn!("{command:?} does not apply to view {id}"),
        }
    }

    /// The user edited an input. Always reported: the engine keeps the text
    /// for layout and stale-value checks even without a JS listener.
    pub(crate) fn input_changed(&mut self, id: NodeId, text: String) {
        let Some(Slot {
            native: Native::Input(input),
            ..
        }) = self.slot_mut(id)
        else {
            return;
        };
        input.count += 1;
        input.update_placeholder();
        let count = input.count;
        self.control.dispatch_event(
            id,
            EventKind::ChangeText,
            EventPayload::Text { text, count },
        );
    }

    pub(crate) fn input_focused(&mut self, id: NodeId, focused: bool) {
        let Some(s) = self.slot(id) else { return };
        let event = if focused {
            EventKind::Focus
        } else {
            EventKind::Blur
        };
        if s.events.contains(event) {
            self.control.dispatch_event(id, event, EventPayload::None);
        }
        if focused {
            self.focused = Some(id);
            // Moving between fields with the keyboard up.
            if self.keyboard > 0.0 {
                self.avoid_keyboard(0.25);
            }
        } else if self.focused == Some(id) {
            self.focused = None;
        }
    }

    /// Return pressed. Single-line inputs end editing, as in React Native.
    pub(crate) fn input_submitted(&mut self, id: NodeId, text: String) {
        let Some(Slot {
            native: Native::Input(input),
            events,
            ..
        }) = self.slot(id)
        else {
            return;
        };
        if events.contains(EventKind::SubmitEditing) {
            let count = input.count;
            self.control.dispatch_event(
                id,
                EventKind::SubmitEditing,
                EventPayload::Text { text, count },
            );
        }
        if !input.multiline() && input.blur_on_submit() {
            input.editor_view().resignFirstResponder();
        }
    }

    /// The software keyboard now covers `height` points of the surface.
    pub(crate) fn keyboard_changed(&mut self, height: f32, duration: f64) {
        self.keyboard = height;
        self.avoid_keyboard(duration);
    }

    /// Keeps the focused input visible: gives its scroll view a bottom inset
    /// as high as the part the keyboard covers, and scrolls to the input.
    fn avoid_keyboard(&mut self, duration: f64) {
        let scroll = self.focused.and_then(|id| self.scroll_around(id));
        if let Some(previous) = self.avoiding.take()
            && (Some(previous) != scroll.as_ref().map(|(id, _)| *id) || self.keyboard <= 0.0)
            && let Some(Slot {
                native: Native::Scroll(view),
                ..
            }) = self.slot(previous)
        {
            let base = self.base_inset(previous).bottom;
            set_bottom_inset(view, base, duration, self.mtm);
        }
        let (Some((scroll_id, scroll)), Some(root)) = (scroll, self.slot(yf_core::ROOT_ID)) else {
            return;
        };
        if self.keyboard <= 0.0 {
            return;
        }
        let root = root.native.view();
        let Some(parent) = scroll.superview() else {
            return;
        };
        // In window coordinates: the scroll view may be in a modal, which
        // is not inside the surface's root view.
        let frame = parent.convertRect_toView(scroll.frame(), None);
        let root_bottom =
            root.convertPoint_toView(CGPoint::new(0.0, root.bounds().size.height), None);
        let keyboard_top = root_bottom.y - self.keyboard as CGFloat;
        let covered = (frame.origin.y + frame.size.height - keyboard_top).max(0.0);
        let base = self.base_inset(scroll_id).bottom;
        set_bottom_inset(&scroll, base + covered, duration, self.mtm);
        self.avoiding = Some(scroll_id);
        if let Some(Slot {
            native: Native::Input(input),
            ..
        }) = self.focused.and_then(|id| self.slot(id))
        {
            let editor = input.editor_view();
            let rect = editor.convertRect_toView(editor.bounds(), Some(&scroll));
            let margin = 12.0;
            scroll.scrollRectToVisible_animated(
                CGRect::new(
                    CGPoint::new(rect.origin.x, rect.origin.y - margin),
                    CGSize::new(rect.size.width, rect.size.height + 2.0 * margin),
                ),
                true,
            );
        }
    }

    /// The nearest app scroll view containing view `id`.
    /// The `contentInset` the app set on scroll view `id`.
    fn base_inset(&self, id: NodeId) -> objc2_ui_kit::UIEdgeInsets {
        ui_insets(
            self.slot(id)
                .and_then(|s| s.behavior.as_ref())
                .map(|b| b.content_inset)
                .unwrap_or_default(),
        )
    }

    /// The nearest scroll view around `view` (or `view` itself).
    fn scroll_of(&self, view: &UIView) -> Option<(&Slot, &UIScrollView)> {
        let mut view = Some(view.retain());
        while let Some(v) = view {
            if let Some(
                s @ Slot {
                    native: Native::Scroll(scroll),
                    ..
                },
            ) = self.slot(v.tag() as NodeId)
                && std::ptr::eq::<UIView>(scroll.as_ref(), &*v)
            {
                return Some((s, scroll));
            }
            view = v.superview();
        }
        None
    }

    /// How a touch on `view` treats the keyboard, when it is up and the
    /// touch is outside the focused input: its scroll view's
    /// `keyboardShouldPersistTaps`.
    pub(crate) fn keyboard_tap(&self, view: &UIView) -> Option<yf_core::KeyboardTaps> {
        if self.keyboard <= 0.0 {
            return None;
        }
        let Some(Slot {
            native: Native::Input(input),
            ..
        }) = self.focused.and_then(|id| self.slot(id))
        else {
            return None;
        };
        if view.isDescendantOfView(input.view()) {
            return None;
        }
        let (scroll, _) = self.scroll_of(view)?;
        Some(
            scroll
                .behavior
                .as_ref()
                .map_or_else(Default::default, |b| b.keyboard_taps),
        )
    }

    pub(crate) fn dismiss_keyboard(&self) {
        if let Some(root) = self.slot(yf_core::ROOT_ID) {
            root.native.view().window().map(|w| w.endEditing(true));
        }
    }

    /// Where a fling of scroll view `id` ends when it snaps: `target` is
    /// UIKit's proposal, `velocity` in points per millisecond.
    pub(crate) fn snap_target(
        &self,
        id: NodeId,
        velocity: CGPoint,
        target: CGPoint,
    ) -> Option<CGPoint> {
        let Some(Slot {
            native: Native::Scroll(scroll),
            behavior: Some(b),
            horizontal,
            ..
        }) = self.slot(id)
        else {
            return None;
        };
        if !b.snaps() {
            return None;
        }
        let offset = scroll.contentOffset();
        let bounds = scroll.bounds().size;
        let content = scroll.contentSize();
        let (at, projected, v, viewport, length) = if *horizontal {
            (offset.x, target.x, velocity.x, bounds.width, content.width)
        } else {
            (
                offset.y,
                target.y,
                velocity.y,
                bounds.height,
                content.height,
            )
        };
        let snapped = b.snap_target(
            at as f32,
            projected as f32,
            v as f32,
            viewport as f32,
            length as f32,
        )? as CGFloat;
        Some(if *horizontal {
            CGPoint::new(snapped, target.y)
        } else {
            CGPoint::new(target.x, snapped)
        })
    }

    fn scroll_around(&self, id: NodeId) -> Option<(NodeId, Retained<UIScrollView>)> {
        let mut view = self.slot(id)?.native.view().superview();
        while let Some(v) = view {
            if let Some(Slot {
                native: Native::Scroll(scroll),
                ..
            }) = self.slot(v.tag() as NodeId)
                && std::ptr::eq::<UIView>(scroll.as_ref(), &*v)
            {
                return Some((v.tag() as NodeId, scroll.clone()));
            }
            view = v.superview();
        }
        None
    }

    /// Reshapes a view's background, border, corners, clipping and shadow.
    fn decorate(&mut self, id: NodeId) {
        let (mtm, animator) = (self.mtm, &self.animator);
        let Some(s) = self.views.get_mut(id as usize).and_then(Option::as_mut) else {
            return;
        };
        let mut background = s.style.background_color;
        let mut animated = false;
        for &(prop, node) in &s.bindings {
            if prop == AnimProp::BackgroundColor {
                animated = true;
                if let AnimValue::Color(c) = animator.eval(node) {
                    background = Color(c);
                }
            }
        }
        let kind = s.native.kind();
        s.decor
            .apply(s.native.view(), kind, &s.style, background, animated, mtm);
    }

    fn set_image(&mut self, id: NodeId, source: Option<ImageSource>, resize_mode: ResizeMode) {
        let Some(s) = self.slot_mut(id) else { return };
        if !matches!(s.native, Native::Image(_)) {
            return;
        }
        s.decor.set_resize_mode(resize_mode);
        if s.image.as_ref().map(|src| &src.uri) == source.as_ref().map(|src| &src.uri) {
            return;
        }
        s.decor.set_image(None);
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
                    if let Some(s) = self.slot_mut(id) {
                        s.decor.set_image(Some(&image));
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
                    s.decor.set_image(Some(image));
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
        let control = self.control.clone();
        let (mtm, mac_idiom) = (self.mtm, self.mac_idiom);
        let Some(slot) = self.views.get_mut(id as usize).and_then(Option::as_mut) else {
            return;
        };
        slot.events = events;
        let v = slot.values;
        let any = |kinds: &[EventKind]| kinds.iter().any(|&k| events.contains(k));
        let wants_press = any(&[
            EventKind::Press,
            EventKind::LongPress,
            EventKind::PressIn,
            EventKind::PressOut,
        ]) || slot.active_opacity.is_some();
        let wants_pan = v.pan_x.is_some()
            || v.pan_y.is_some()
            || any(&[EventKind::PanStart, EventKind::PanEnd]);
        let wants_pinch = v.pinch.is_some() || any(&[EventKind::PinchStart, EventKind::PinchEnd]);
        let wants_rotation =
            v.rotation.is_some() || any(&[EventKind::RotationStart, EventKind::RotationEnd]);
        let view = slot.native.view();
        let g = &mut slot.gestures;
        let delegate = ProtocolObject::from_ref(&*target);
        toggle(&mut g.press, wants_press, view, || {
            PressRecognizer::new(mtm, &target)
        });
        // SAFETY: every selector is implemented by `EventTarget`, which the
        // mounter keeps alive as long as the recognizers; so is the delegate.
        unsafe {
            toggle(&mut g.pan, wants_pan, view, || {
                let r = UIPanGestureRecognizer::initWithTarget_action(
                    UIPanGestureRecognizer::alloc(mtm),
                    Some(&target),
                    Some(sel!(handlePan:)),
                );
                r.setDelegate(Some(delegate));
                r
            });
            toggle(&mut g.pinch, wants_pinch, view, || {
                let r = UIPinchGestureRecognizer::initWithTarget_action(
                    UIPinchGestureRecognizer::alloc(mtm),
                    Some(&target),
                    Some(sel!(handlePinch:)),
                );
                r.setDelegate(Some(delegate));
                r
            });
            toggle(&mut g.rotation, wants_rotation, view, || {
                let r = UIRotationGestureRecognizer::initWithTarget_action(
                    UIRotationGestureRecognizer::alloc(mtm),
                    Some(&target),
                    Some(sel!(handleRotation:)),
                );
                r.setDelegate(Some(delegate));
                r
            });
        }
        if let Some(press) = &g.press {
            press.set_long_press(events.contains(EventKind::LongPress));
        }
        if wants_press || wants_pan || wants_pinch || wants_rotation {
            view.setUserInteractionEnabled(true);
        }
        if let Some(yf) = slot.native.yf() {
            yf.set_pressable(events.contains(EventKind::Press), &control);
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

    /// Screens, header items and modals are not subviews of their parent:
    /// records them for [`Mounter::finish_navigation`]. Returns true if
    /// `child` is one of them.
    fn insert_hosted(&mut self, parent: NodeId, child: NodeId, before: Option<NodeId>) -> bool {
        let Some(c) = self.slot(child) else {
            return false;
        };
        match &c.native {
            Native::Screen(_) => {
                if let Some(Slot {
                    native: Native::Stack(stack),
                    ..
                }) = self.slot_mut(parent)
                {
                    stack.screens.retain(|&s| s != child);
                    let pos = before
                        .and_then(|b| stack.screens.iter().position(|&s| s == b))
                        .unwrap_or(stack.screens.len());
                    stack.screens.insert(pos, child);
                    self.dirty_stacks.push(parent);
                }
            }
            Native::HeaderItem(_) => self.dirty_headers.push(parent),
            Native::Modal(_) => self.dirty_modals.push(child),
            _ => return false,
        }
        self.hosts.insert(child, parent);
        true
    }

    /// The counterpart of [`Mounter::insert_hosted`] for removals.
    fn remove_hosted(&mut self, id: NodeId) -> bool {
        let Some(parent) = self.hosts.remove(&id) else {
            return false;
        };
        match self.slot(id).map(|s| &s.native) {
            Some(Native::Screen(_)) => {
                if let Some(Slot {
                    native: Native::Stack(stack),
                    ..
                }) = self.slot_mut(parent)
                {
                    stack.screens.retain(|&s| s != id);
                    stack.popped.retain(|&s| s != id);
                    self.dirty_stacks.push(parent);
                }
            }
            Some(Native::HeaderItem(view)) => {
                view.removeFromSuperview();
                self.dirty_headers.push(parent);
            }
            Some(Native::Modal(_)) => self.dirty_modals.push(id),
            _ => {}
        }
        true
    }

    /// Applies the navigation changes of a batch once all its views exist
    /// and are laid out: pushes and pops screens, updates headers, presents
    /// and dismisses modals.
    fn finish_navigation(&mut self) {
        let mut stacks = std::mem::take(&mut self.dirty_stacks);
        stacks.sort_unstable();
        stacks.dedup();
        for id in stacks {
            let Some(Slot {
                native: Native::Stack(stack),
                ..
            }) = self.slot(id)
            else {
                continue;
            };
            if stack.nav.transitionCoordinator().is_some() {
                if !self.deferred_stacks.contains(&id) {
                    self.deferred_stacks.push(id);
                }
                continue;
            }
            let screens: Vec<_> = stack
                .screens
                .iter()
                .filter(|s| !stack.popped.contains(s))
                .filter_map(|&s| match self.slot(s).map(|s| &s.native) {
                    Some(Native::Screen(screen)) => {
                        Some((s, screen.controller.clone(), screen.config.animation))
                    }
                    _ => None,
                })
                .collect();
            let leaving = stack
                .nav
                .topViewController()
                .and_then(|c| c.downcast::<crate::navigation::ScreenController>().ok())
                .and_then(|c| match self.slot(c.screen_id()).map(|s| &s.native) {
                    Some(Native::Screen(screen)) => Some(screen.config.animation),
                    _ => None,
                });
            stack.reconcile(&screens, leaving);
            if let Some(Slot {
                native: Native::Stack(stack),
                ..
            }) = self.slot_mut(id)
            {
                stack.contain();
            }
        }
        let mut headers = std::mem::take(&mut self.dirty_headers);
        headers.sort_unstable();
        headers.dedup();
        for id in headers {
            let Some(Slot {
                native: Native::Screen(screen),
                ..
            }) = self.slot(id)
            else {
                continue;
            };
            screen.apply_header(|item| match self.slot(item).map(|s| &s.native) {
                Some(Native::HeaderItem(v)) => Some(Retained::into_super(v.clone())),
                _ => None,
            });
            if screen.config.large_title {
                // Large titles collapse as the content scrolls under them.
                if let Some(scroll) = first_scroll_view(&screen.view) {
                    scroll.setContentInsetAdjustmentBehavior(
                        UIScrollViewContentInsetAdjustmentBehavior::Automatic,
                    );
                }
            }
        }
        let modals = std::mem::take(&mut self.dirty_modals);
        let root = self
            .slot(yf_core::ROOT_ID)
            .map(|s| s.native.view().retain());
        for id in modals {
            let shown = self.hosts.contains_key(&id);
            let control = self.control.clone();
            let Some(Slot {
                native: Native::Modal(modal),
                ..
            }) = self.slot_mut(id)
            else {
                continue;
            };
            if shown {
                if let Some(root) = &root {
                    modal.present(root);
                }
            } else {
                modal.dismiss(move || {
                    control.dispatch_event(id, EventKind::Dismiss, EventPayload::None)
                });
            }
        }
    }

    /// A stack is about to show a screen: its header options apply.
    pub(crate) fn stack_will_show(
        &mut self,
        nav: &objc2_ui_kit::UINavigationController,
        controller: &objc2_ui_kit::UIViewController,
        animated: bool,
    ) {
        let Some(screen) = controller.downcast_ref::<crate::navigation::ScreenController>() else {
            return;
        };
        let Some(Slot {
            native: Native::Screen(screen),
            ..
        }) = self.slot(screen.screen_id())
        else {
            return;
        };
        let c = &screen.config;
        nav.setNavigationBarHidden_animated(!c.header_shown, animated);
        let bar = nav.navigationBar();
        bar.setPrefersLargeTitles(c.large_title);
        let tint = c.tint_color.map(|t| {
            let (r, g, b, a) = t.to_f64();
            objc2_ui_kit::UIColor::colorWithRed_green_blue_alpha(r, g, b, a)
        });
        // SAFETY: a plain setter; `None` restores the default tint.
        unsafe { bar.setTintColor(tint.as_deref()) };
    }

    /// A stack finished a transition: screens the user popped (back button,
    /// swipe) are reported so the app removes them.
    pub(crate) fn stack_did_show(&mut self, id: NodeId) {
        self.report_popped(id);
        // Changes that arrived during the transition, once it is over (the
        // transition coordinator outlives this callback).
        if let Some(i) = self.deferred_stacks.iter().position(|&s| s == id) {
            self.deferred_stacks.swap_remove(i);
            let shared = self.shared.clone();
            dispatch2::DispatchQueue::main().exec_async(move || {
                let mtm = MainThreadMarker::new().expect("main queue");
                if let Some(m) = shared.get(mtm).borrow_mut().as_mut() {
                    m.dirty_stacks.push(id);
                    m.finish_navigation();
                }
            });
        }
    }

    /// Reports the screens of stack `id` that the user popped natively.
    fn report_popped(&mut self, id: NodeId) {
        let Some(Slot {
            native: Native::Stack(stack),
            ..
        }) = self.slot(id)
        else {
            return;
        };
        let shown = stack.nav.viewControllers();
        // Screens the controller had and lost; not those still waiting for
        // a transition to end.
        let gone: Vec<NodeId> = stack
            .applied
            .borrow()
            .iter()
            .copied()
            .filter(|s| stack.screens.contains(s))
            .filter(|s| !stack.popped.contains(s))
            .filter(|&s| match self.slot(s).map(|s| &s.native) {
                Some(Native::Screen(screen)) => !shown.iter().any(|c| {
                    std::ptr::eq::<objc2_ui_kit::UIViewController>(&*c, screen.controller.as_ref())
                }),
                _ => false,
            })
            .collect();
        if gone.is_empty() {
            return;
        }
        if let Some(Slot {
            native: Native::Stack(stack),
            ..
        }) = self.slot_mut(id)
        {
            stack.popped.extend(&gone);
        }
        for screen in gone {
            self.control
                .dispatch_event(screen, EventKind::Dismiss, EventPayload::None);
        }
    }

    /// Whether the user may pop the top screen of stack `id`.
    pub(crate) fn stack_can_pop(&self, id: NodeId) -> bool {
        let Some(Slot {
            native: Native::Stack(stack),
            ..
        }) = self.slot(id)
        else {
            return false;
        };
        let top = stack
            .screens
            .iter()
            .rev()
            .find(|s| !stack.popped.contains(s))
            .and_then(|&s| match self.slot(s).map(|s| &s.native) {
                Some(Native::Screen(screen)) => Some(screen.config.gesture_enabled),
                _ => None,
            });
        // Never while a push or pop is still animating.
        stack.nav.viewControllers().count() > 1
            && stack.nav.transitionCoordinator().is_none()
            && top.unwrap_or(true)
    }

    /// Properties of a switch, slider, activity indicator, web view or video.
    fn set_native_props(&mut self, id: NodeId, props: yf_core::NativeProps) {
        use yf_core::NativeProps as P;
        let Some(s) = self.slot_mut(id) else { return };
        match (&mut s.native, props) {
            (Native::Switch(v), P::Switch(p)) => crate::controls::configure_switch(v, &p),
            (Native::Slider(v, step), P::Slider(p)) => {
                *step = p.step;
                crate::controls::configure_slider(v, &p);
            }
            (Native::Activity(v), P::Activity(p)) => crate::controls::configure_activity(v, &p),
            (Native::Web(v), P::WebView(p)) => v.configure(*p),
            (Native::Video(v), P::Video(p)) => v.configure(*p),
            _ => {}
        }
    }

    /// The user flipped a switch.
    pub(crate) fn switch_changed(&mut self, id: NodeId) {
        if let Some(Slot {
            native: Native::Switch(s),
            events,
            ..
        }) = self.slot(id)
            && events.contains(EventKind::ValueChange)
        {
            self.control.dispatch_event(
                id,
                EventKind::ValueChange,
                EventPayload::Value(PayloadValue::Bool(s.isOn())),
            );
        }
    }

    /// A slider moved (`end`: the user let go).
    pub(crate) fn slider_changed(&mut self, id: NodeId, end: bool) {
        let Some(Slot {
            native: Native::Slider(s, step),
            events,
            ..
        }) = self.slot(id)
        else {
            return;
        };
        let value = crate::controls::snapped(s, *step);
        let event = if end {
            EventKind::SlidingComplete
        } else {
            EventKind::ValueChange
        };
        // The core keeps the value even without a listener.
        if events.contains(event) || !end {
            self.control.dispatch_event(
                id,
                event,
                EventPayload::Value(PayloadValue::Number(value as f64)),
            );
        }
    }

    /// Touch, accessibility and test properties.
    fn set_view_props(&mut self, id: NodeId, props: ViewProps) {
        let (control, mtm) = (self.control.clone(), self.mtm);
        let Some(s) = self.slot_mut(id) else { return };
        let active = props.active_opacity;
        match s.native.yf() {
            Some(view) => view.set_props(props, &control),
            None => {
                // Scroll views and inputs: only what UIKit offers directly.
                use objc2_ui_kit::{NSObjectUIAccessibility, UIAccessibilityIdentification};
                let view = s.native.view();
                let test_id = props
                    .test_id
                    .as_deref()
                    .map(objc2_foundation::NSString::from_str);
                view.setAccessibilityIdentifier(test_id.as_deref());
                let label = props
                    .accessibility
                    .label
                    .as_deref()
                    .map(objc2_foundation::NSString::from_str);
                view.setAccessibilityLabel(label.as_deref(), mtm);
                view.setAccessibilityElementsHidden(props.accessibility.hidden, mtm);
            }
        }
        if s.active_opacity != active {
            s.active_opacity = active;
            let events = s.events;
            self.set_events(id, events);
        }
    }

    /// A finger went down on (or left) a pressable view.
    pub(crate) fn press_changed(&mut self, id: NodeId, pressed: bool) {
        let Some(s) = self.slot_mut(id) else { return };
        s.pressed = pressed;
        let event = if pressed {
            EventKind::PressIn
        } else {
            EventKind::PressOut
        };
        if s.events.contains(event) {
            self.control.dispatch_event(id, event, EventPayload::None);
        }
        let Some(s) = self.slot(id) else { return };
        if s.active_opacity.is_some() {
            // Native feedback: no JS round trip.
            let view = s.native.view().retain();
            let alpha = dynamic_opacity(s, &self.animator);
            let block = block2::RcBlock::new(move || view.setAlpha(alpha as CGFloat));
            let duration = if pressed { 0.1 } else { 0.25 };
            UIView::animateWithDuration_delay_options_animations_completion(
                duration,
                0.0,
                UIViewAnimationOptions::BeginFromCurrentState
                    | UIViewAnimationOptions::AllowUserInteraction,
                &block,
                None,
                self.mtm,
            );
        }
    }

    /// The finger lifted on a pressable view.
    pub(crate) fn pressed(&mut self, id: NodeId) {
        if let Some(s) = self.slot(id)
            && s.events.contains(EventKind::Press)
        {
            self.control
                .dispatch_event(id, EventKind::Press, EventPayload::None);
        }
    }

    /// A press was held for half a second.
    pub(crate) fn long_pressed(&mut self, id: NodeId) {
        if let Some(s) = self.slot(id)
            && s.events.contains(EventKind::LongPress)
        {
            self.control
                .dispatch_event(id, EventKind::LongPress, EventPayload::None);
        }
    }

    /// Whether a drag on view `id` moving at `velocity` may start: a pan
    /// with only a horizontal (or vertical) value waits for a mostly
    /// horizontal (vertical) movement, so it coexists with scrolling.
    pub(crate) fn pan_should_begin(&self, id: NodeId, velocity: CGPoint) -> bool {
        let Some(s) = self.slot(id) else { return true };
        match (s.values.pan_x.is_some(), s.values.pan_y.is_some()) {
            (true, false) => velocity.x.abs() > velocity.y.abs(),
            (false, true) => velocity.y.abs() > velocity.x.abs(),
            _ => true,
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
        let (values, events) = (s.values, s.events);
        let payload = EventPayload::Pan {
            translation_x: t.x as f32,
            translation_y: t.y as f32,
            velocity_x: v.x as f32,
            velocity_y: v.y as f32,
        };
        if state == UIGestureRecognizerState::Began {
            // Grabbing an animating value stops it where it is.
            let x = values.pan_x.map_or(0.0, |n| self.animator.value(n));
            let y = values.pan_y.map_or(0.0, |n| self.animator.value(n));
            if let Some(s) = self.slot_mut(id) {
                (s.gesture_start[0], s.gesture_start[1]) = (x, y);
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
            let start = self.slot(id).map_or([0.0; 4], |s| s.gesture_start);
            if let Some(n) = values.pan_x {
                self.animator.set(n, start[0] + t.x as f32);
            }
            if let Some(n) = values.pan_y {
                self.animator.set(n, start[1] + t.y as f32);
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

    /// A pinch (`rotation` false, `amount` the scale) or a two-finger
    /// rotation (`amount` in radians) on a view.
    pub(crate) fn transform_gesture(
        &mut self,
        id: NodeId,
        rotation: bool,
        state: UIGestureRecognizerState,
        amount: f64,
        velocity: f64,
    ) {
        let Some(s) = self.slot(id) else { return };
        let (node, events) = if rotation {
            (s.values.rotation, s.events)
        } else {
            (s.values.pinch, s.events)
        };
        let slot_index = if rotation { 3 } else { 2 };
        // Degrees for rotations, like `transform: [{ rotate }]`.
        let (amount, velocity) = if rotation {
            (amount.to_degrees(), velocity.to_degrees())
        } else {
            (amount, velocity)
        };
        let (start_event, end_event, key) = if rotation {
            (EventKind::RotationStart, EventKind::RotationEnd, "rotation")
        } else {
            (EventKind::PinchStart, EventKind::PinchEnd, "scale")
        };
        let payload = || {
            EventPayload::Map(vec![
                (key, PayloadValue::Number(amount)),
                ("velocity", PayloadValue::Number(velocity)),
            ])
        };
        if state == UIGestureRecognizerState::Began {
            let start = node.map_or(if rotation { 0.0 } else { 1.0 }, |n| self.animator.value(n));
            if let Some(s) = self.slot_mut(id) {
                s.gesture_start[slot_index] = start;
            }
            if events.contains(start_event) {
                self.control.dispatch_event(id, start_event, payload());
            }
        }
        if matches!(
            state,
            UIGestureRecognizerState::Began | UIGestureRecognizerState::Changed
        ) && let Some(n) = node
        {
            let start = self.slot(id).map_or(0.0, |s| s.gesture_start[slot_index]);
            let value = if rotation {
                start + amount as f32
            } else {
                start * amount as f32
            };
            self.animator.set(n, value);
            self.apply_animations();
        }
        if matches!(
            state,
            UIGestureRecognizerState::Ended | UIGestureRecognizerState::Cancelled
        ) && events.contains(end_event)
        {
            self.control.dispatch_event(id, end_event, payload());
        }
        self.report_finished();
    }

    /// JS waits for a display frame (`requestAnimationFrame`).
    pub(crate) fn request_frame(&self) {
        self.frame_link.setPaused(false);
    }

    /// The frame link fired.
    pub(crate) fn frame(&self) {
        self.frame_link.setPaused(true);
        self.control.frame();
    }

    /// Fades or grows in the views a layout animation created.
    fn animate_appearing(&mut self, animation: &LayoutAnimation) {
        let Some(t) = animation.create else { return };
        for &id in &self.created {
            let Some(s) = self.slot(id) else { continue };
            let view = s.native.view();
            if view.superview().is_none() {
                continue;
            }
            match t.property {
                LayoutProperty::Opacity => view.setAlpha(0.0),
                LayoutProperty::Scale => view
                    .layer()
                    .setTransform(scaled_transform(&view.layer().transform(), 0.01)),
            }
            let (view, alpha, transform) = (
                view.retain(),
                dynamic_opacity(s, &self.animator) as CGFloat,
                dynamic_transform(s, &self.animator),
            );
            animate(&t, self.mtm, move || {
                view.setAlpha(alpha);
                view.layer().setTransform(transform);
            });
        }
    }

    /// The removal animation of view `id` finished.
    pub(crate) fn disappeared(&mut self, id: NodeId) {
        if self.disappearing.remove(&id)
            && let Some(s) = self.slot(id)
        {
            let view = s.native.view();
            view.removeFromSuperview();
            view.layer().removeAllAnimations();
            apply_dynamic(s, &self.animator);
        }
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
                let base = s
                    .behavior
                    .as_ref()
                    .map_or(0.0, |b| b.content_inset.top as CGFloat);
                inset.top = base + if on { REFRESH_INSET } else { 0.0 };
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

    /// Brings a list's sticky cells above its other cells.
    fn front_sticky(&self, list: NodeId) {
        let (Some(cells), Some(l)) = (self.sticky.get(&list), self.slot(list)) else {
            return;
        };
        let container = l.native.view();
        for c in cells {
            if let Some(s) = self.slot(c.cell) {
                container.bringSubviewToFront(s.native.view());
            }
        }
    }

    /// Moves a list's sticky cells with its scroll offset: each one stays
    /// at the top of the viewport until the next pushes it out.
    fn pin_sticky(&self, list: NodeId) {
        let (Some(cells), Some(l)) = (self.sticky.get(&list), self.slot(list)) else {
            return;
        };
        let Native::Scroll(scroll) = &l.native else {
            return;
        };
        let offset = scroll.contentOffset();
        let offset = if l.horizontal { offset.x } else { offset.y } as f32;
        for c in cells {
            let Some(s) = self.slot(c.cell) else { continue };
            let t = (offset - c.start).clamp(0.0, c.end - c.start) as CGFloat;
            let (x, y) = if l.horizontal { (t, 0.0) } else { (0.0, t) };
            s.native
                .view()
                .layer()
                .setTransform(CATransform3D::new_translation(x, y, 0.0));
        }
    }

    /// A scroll view moved: feeds its scroll value in the same frame.
    pub(crate) fn scrolled(&mut self, id: NodeId, offset: CGPoint) {
        self.pin_sticky(id);
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

impl Drop for Mounter {
    fn drop(&mut self) {
        // The link retains its target; invalidating breaks the cycle.
        self.frame_link.invalidate();
    }
}

/// The first scroll view in `view`'s subtree (breadth first).
fn first_scroll_view(view: &UIView) -> Option<Retained<UIScrollView>> {
    let mut queue = vec![view.retain()];
    while !queue.is_empty() {
        let mut next = Vec::new();
        for v in queue {
            for child in v.subviews().iter() {
                if let Ok(scroll) = child.clone().downcast::<UIScrollView>() {
                    return Some(scroll);
                }
                next.push(child);
            }
        }
        queue = next;
    }
    None
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

/// The opacity to show: the style's or its animated value, dimmed while
/// pressed with `activeOpacity`.
fn dynamic_opacity(slot: &Slot, animator: &Animator) -> f32 {
    let mut opacity = slot.style.opacity;
    for &(prop, node) in &slot.bindings {
        if prop == AnimProp::Opacity {
            opacity = animator.eval(node).number().clamp(0.0, 1.0);
        }
    }
    match slot.active_opacity {
        Some(active) if slot.pressed => opacity * active,
        _ => opacity,
    }
}

/// The transform list with animated entries, as a Core Animation matrix.
fn dynamic_transform(slot: &Slot, animator: &Animator) -> CATransform3D {
    let mut overrides = [None; yf_core::MAX_TRANSFORMS];
    for &(prop, node) in &slot.bindings {
        if let AnimProp::Transform(i) = prop
            && let Some(o) = overrides.get_mut(i as usize)
        {
            *o = Some(animator.eval(node).number());
        }
    }
    let m = slot.style.transform.matrix(|i| overrides[i]).0;
    CATransform3D {
        m11: m[0],
        m12: m[1],
        m13: m[2],
        m14: m[3],
        m21: m[4],
        m22: m[5],
        m23: m[6],
        m24: m[7],
        m31: m[8],
        m32: m[9],
        m33: m[10],
        m34: m[11],
        m41: m[12],
        m42: m[13],
        m43: m[14],
        m44: m[15],
    }
}

/// `t` scaled by `f` around the layer's center.
fn scaled_transform(t: &CATransform3D, f: CGFloat) -> CATransform3D {
    let mut out = *t;
    for v in [
        &mut out.m11,
        &mut out.m12,
        &mut out.m13,
        &mut out.m21,
        &mut out.m22,
        &mut out.m23,
    ] {
        *v *= f;
    }
    out
}

/// Opacity, background color and transform: static values overridden by
/// animated bindings.
fn apply_dynamic(slot: &Slot, animator: &Animator) {
    let view = slot.native.view();
    let mut background = slot.style.background_color;
    for &(prop, node) in &slot.bindings {
        if prop == AnimProp::BackgroundColor
            && let AnimValue::Color(c) = animator.eval(node)
        {
            background = Color(c);
        }
    }
    view.setAlpha(dynamic_opacity(slot, animator) as CGFloat);
    slot.decor.set_background(view, background);
    view.layer().setTransform(dynamic_transform(slot, animator));
}

/// Runs `f` in a UIKit animation following `t`.
fn animate(t: &LayoutTransition, mtm: MainThreadMarker, f: impl Fn() + 'static) {
    let block = block2::RcBlock::new(f);
    let (duration, delay) = (t.duration_ms as f64 / 1000.0, t.delay_ms as f64 / 1000.0);
    let base = UIViewAnimationOptions::BeginFromCurrentState
        | UIViewAnimationOptions::AllowUserInteraction;
    match t.curve {
        LayoutCurve::Spring { damping } => {
            UIView::animateWithDuration_delay_usingSpringWithDamping_initialSpringVelocity_options_animations_completion(
                duration,
                delay,
                damping as CGFloat,
                0.0,
                base,
                &block,
                None,
                mtm,
            )
        }
        curve => {
            let curve = match curve {
                LayoutCurve::Linear => UIViewAnimationOptions::CurveLinear,
                LayoutCurve::EaseIn => UIViewAnimationOptions::CurveEaseIn,
                LayoutCurve::EaseOut => UIViewAnimationOptions::CurveEaseOut,
                _ => UIViewAnimationOptions::CurveEaseInOut,
            };
            UIView::animateWithDuration_delay_options_animations_completion(
                duration,
                delay,
                base | curve,
                &block,
                None,
                mtm,
            )
        }
    }
}

/// Fades or shrinks `view` out, then removes it (unless it came back).
fn disappear(
    view: &UIView,
    id: NodeId,
    t: &LayoutTransition,
    shared: SharedMounter,
    mtm: MainThreadMarker,
) {
    let target = view.retain();
    let property = t.property;
    let block = block2::RcBlock::new(move || match property {
        LayoutProperty::Opacity => target.setAlpha(0.0),
        LayoutProperty::Scale => target
            .layer()
            .setTransform(scaled_transform(&target.layer().transform(), 0.01)),
    });
    let done = block2::RcBlock::new(move |_finished: objc2::runtime::Bool| {
        if let Some(m) = shared
            .get(MainThreadMarker::new().expect("main"))
            .borrow_mut()
            .as_mut()
        {
            m.disappeared(id);
        }
    });
    UIView::animateWithDuration_delay_options_animations_completion(
        t.duration_ms as f64 / 1000.0,
        t.delay_ms as f64 / 1000.0,
        UIViewAnimationOptions::BeginFromCurrentState,
        &block,
        Some(&done),
        mtm,
    );
}
