//! Encodes a [`MountBatch`] for `Mounter.java`: a little-endian stream of
//! 32-bit ints and floats, plus an object table for strings and text
//! layouts. One JNI call per commit, no per-item crossings.
//!
//! Animation items (graph ops, bindings, drag and scroll values) are not
//! encoded: they go to the UI-thread animator ([`crate::anim`]).
//!
//! The opcodes must match `Mounter.java`.

use jni::objects::GlobalRef;
use yf_core::{
    AccessibilityRole, AutoCapitalize, AutoFill, BorderStyle, Checked, Color, Command,
    DialogButtonStyle, InputConfig, KeyboardDismissMode, KeyboardTaps, KeyboardType, LayoutCurve,
    LayoutProperty, LayoutTransition, LiveRegion, ModalAnimation, ModalPresentation, MountBatch,
    MountItem, NativeProps, OriginLen, PointerEvents, ResizeMode, ReturnKeyType, ScreenAnimation,
    ScreenConfig, SnapAlignment, StatusBarStyle, TextAlign, ViewKind, ViewProps, ViewStyle,
    WebSource,
};

use crate::{
    anim::view_transform,
    text::{AndroidLayout, argb},
};

const CREATE: i32 = 0;
const INSERT: i32 = 1;
const REMOVE: i32 = 2;
const DELETE: i32 = 3;
const FRAME: i32 = 4;
const VIEW_STYLE: i32 = 5;
const TEXT: i32 = 6;
const EVENTS: i32 = 7;
const IMAGE: i32 = 8;
const SCROLL_CONFIG: i32 = 9;
const SCROLL_CONTENT: i32 = 10;
const SCROLL_BY: i32 = 11;
const REFRESHING: i32 = 12;
const DYNAMIC: i32 = 13;
const GESTURES: i32 = 14;
const INPUT: i32 = 15;
const INPUT_VALUE: i32 = 16;
const INPUT_CONTENT: i32 = 17;
const COMMAND: i32 = 18;
const VIEW_PROPS: i32 = 19;
const LAYOUT_ANIMATION: i32 = 20;
const SCREEN_CONFIG: i32 = 21;
const MODAL_CONFIG: i32 = 22;
const NATIVE_PROPS: i32 = 23;
const STICKY_CELLS: i32 = 24;
const SCROLL_BEHAVIOR: i32 = 25;
const LAYOUT_DIRECTION: i32 = 26;
const TRACK_STATES: i32 = 27;

/// An entry of the object table.
pub(crate) enum Obj {
    Str(String),
    Layout(GlobalRef),
}

/// An encoded batch.
#[derive(Default)]
pub(crate) struct Encoded {
    pub(crate) bytes: Vec<u8>,
    pub(crate) objects: Vec<Obj>,
    /// Items for the UI-thread animator, in order.
    pub(crate) anim: Vec<MountItem>,
}

impl Encoded {
    #[inline]
    fn int(&mut self, v: i32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    #[inline]
    fn float(&mut self, v: f32) {
        self.bytes.extend_from_slice(&v.to_le_bytes());
    }

    #[inline]
    fn bool(&mut self, v: bool) {
        self.int(v as i32);
    }

    fn string(&mut self, s: String) {
        let index = self.objects.len() as i32;
        self.objects.push(Obj::Str(s));
        self.int(index);
    }

    /// An optional string: its object index, or -1.
    fn opt_string(&mut self, s: Option<&str>) {
        match s {
            Some(s) => self.string(s.to_owned()),
            None => self.int(-1),
        }
    }
}

pub(crate) fn encode(batch: MountBatch) -> Encoded {
    let mut e = Encoded {
        bytes: Vec::with_capacity(batch.items.len() * 16),
        ..Encoded::default()
    };
    for item in batch.items {
        match item {
            MountItem::Create { id, kind } => {
                e.int(CREATE);
                e.int(id as i32);
                e.int(match kind {
                    ViewKind::View => 0,
                    ViewKind::Text => 1,
                    ViewKind::Image => 2,
                    ViewKind::Scroll => 3,
                    ViewKind::Input => 4,
                    ViewKind::Stack => 5,
                    ViewKind::Screen => 6,
                    ViewKind::Modal => 7,
                    ViewKind::HeaderItem => 8,
                    ViewKind::Switch => 9,
                    ViewKind::Slider => 10,
                    ViewKind::ActivityIndicator => 11,
                    ViewKind::WebView => 12,
                    ViewKind::Video => 13,
                });
            }
            MountItem::Insert {
                parent,
                child,
                before,
            } => {
                e.int(INSERT);
                e.int(parent as i32);
                e.int(child as i32);
                e.int(before.map_or(-1, |b| b as i32));
            }
            MountItem::Remove { id } => {
                e.int(REMOVE);
                e.int(id as i32);
            }
            MountItem::Delete { id } => {
                e.int(DELETE);
                e.int(id as i32);
                // The animator forgets the view too.
                e.anim.push(MountItem::Delete { id });
            }
            MountItem::Frame { id, frame } => {
                e.int(FRAME);
                e.int(id as i32);
                for v in [frame.x, frame.y, frame.width, frame.height] {
                    e.float(v);
                }
            }
            MountItem::ViewStyle { id, style } => {
                e.int(VIEW_STYLE);
                e.int(id as i32);
                view_style(&mut e, &style);
                // The animator composes animated values with the static ones.
                e.anim.push(MountItem::ViewStyle { id, style });
            }
            MountItem::Text {
                id,
                layout,
                content,
                text,
            } => {
                let Some(AndroidLayout(layout)) = layout.downcast::<AndroidLayout>() else {
                    continue;
                };
                e.int(TEXT);
                e.int(id as i32);
                e.int(e.objects.len() as i32);
                e.objects.push(Obj::Layout(layout.clone()));
                for v in [content.x, content.y, content.width, content.height] {
                    e.float(v);
                }
                // Pressable spans: start, end, node, events (low, high).
                e.int(text.links.len() as i32);
                for l in &text.links {
                    e.int(l.start as i32);
                    e.int(l.end as i32);
                    e.int(l.node as i32);
                    e.int(l.events.bits() as u32 as i32);
                    e.int((l.events.bits() >> 32) as u32 as i32);
                }
            }
            MountItem::Events { id, events } => {
                e.int(EVENTS);
                e.int(id as i32);
                // A bit set indexed by `EventKind as u8`, as a Java long.
                let bits = events.bits();
                e.int(bits as u32 as i32);
                e.int((bits >> 32) as u32 as i32);
            }
            MountItem::Image {
                id,
                source,
                resize_mode,
                options,
            } => {
                e.int(IMAGE);
                e.int(id as i32);
                match source {
                    Some(source) => e.string(source.uri.to_string()),
                    None => e.int(-1),
                }
                e.int(match resize_mode {
                    ResizeMode::Cover => 0,
                    ResizeMode::Contain => 1,
                    ResizeMode::Stretch => 2,
                    ResizeMode::Center => 3,
                });
                e.opt_string(options.placeholder.as_ref().map(|p| &*p.uri));
                e.float(options.blur_radius);
                e.float(options.fade_ms.unwrap_or(-1.0));
            }
            MountItem::ScrollConfig {
                id,
                horizontal,
                shows_indicator,
                scroll_value,
                keyboard_dismiss,
                mirrored,
            } => {
                e.int(SCROLL_CONFIG);
                e.int(id as i32);
                e.bool(horizontal);
                e.bool(shows_indicator);
                e.int(match keyboard_dismiss {
                    KeyboardDismissMode::None => 0,
                    KeyboardDismissMode::OnDrag => 1,
                    KeyboardDismissMode::Interactive => 2,
                });
                e.bool(mirrored);
                e.anim.push(MountItem::ScrollConfig {
                    id,
                    horizontal,
                    shows_indicator,
                    scroll_value,
                    keyboard_dismiss,
                    mirrored,
                });
            }
            MountItem::ScrollContent { id, size } => {
                e.int(SCROLL_CONTENT);
                e.int(id as i32);
                e.float(size.width);
                e.float(size.height);
            }
            MountItem::StickyCells { id, cells } => {
                e.int(STICKY_CELLS);
                e.int(id as i32);
                e.int(cells.len() as i32);
                for c in &cells {
                    e.int(c.cell as i32);
                    e.float(c.start);
                    e.float(c.end);
                }
            }
            MountItem::LayoutDirection { rtl } => {
                e.int(LAYOUT_DIRECTION);
                e.bool(rtl);
            }
            MountItem::TrackStates { id, states } => {
                e.int(TRACK_STATES);
                e.int(id as i32);
                e.int(states as i32);
            }
            MountItem::ScrollBehavior { id, behavior } => {
                let b = &*behavior;
                e.int(SCROLL_BEHAVIOR);
                e.int(id as i32);
                e.bool(b.paging);
                e.float(b.snap_interval);
                e.int(b.snap_offsets.len() as i32);
                for &o in &b.snap_offsets {
                    e.float(o);
                }
                e.int(match b.snap_alignment {
                    SnapAlignment::Start => 0,
                    SnapAlignment::Center => 1,
                    SnapAlignment::End => 2,
                });
                e.float(b.deceleration);
                e.bool(b.enabled);
                e.bool(b.bounces);
                let i = b.content_inset;
                e.float(i.top);
                e.float(i.right);
                e.float(i.bottom);
                e.float(i.left);
                e.int(match b.keyboard_taps {
                    KeyboardTaps::Never => 0,
                    KeyboardTaps::Always => 1,
                    KeyboardTaps::Handled => 2,
                });
                e.bool(b.one_step);
            }
            MountItem::ScrollBy { id, dx, dy } => {
                e.int(SCROLL_BY);
                e.int(id as i32);
                e.float(dx);
                e.float(dy);
            }
            MountItem::Refreshing { id, refreshing } => {
                e.int(REFRESHING);
                e.int(id as i32);
                e.bool(refreshing);
            }
            MountItem::AnimBindings { id, bindings } => {
                // Java restores the static style; the animator then writes
                // the bound properties.
                e.int(DYNAMIC);
                e.int(id as i32);
                e.anim.push(MountItem::AnimBindings { id, bindings });
            }
            MountItem::Gestures { id, values } => {
                e.int(GESTURES);
                e.int(id as i32);
                e.bool(values.pan_x.is_some() || values.pan_y.is_some());
                // Axis lock: a pan with only one value starts on movement
                // along that axis, so it coexists with scrolling.
                e.int(match (values.pan_x.is_some(), values.pan_y.is_some()) {
                    (true, false) => 1,
                    (false, true) => 2,
                    _ => 0,
                });
                e.bool(values.pinch.is_some());
                e.bool(values.rotation.is_some());
                e.anim.push(MountItem::Gestures { id, values });
            }
            MountItem::ViewProps { id, props } => {
                e.int(VIEW_PROPS);
                e.int(id as i32);
                view_props(&mut e, &props);
            }
            MountItem::ScreenConfig { id, config } => {
                e.int(SCREEN_CONFIG);
                e.int(id as i32);
                screen_config(&mut e, &config);
            }
            MountItem::NativeProps { id, props } => {
                e.int(NATIVE_PROPS);
                e.int(id as i32);
                native_props(&mut e, &props);
            }
            MountItem::ModalConfig { id, config } => {
                e.int(MODAL_CONFIG);
                e.int(id as i32);
                e.int(match config.animation {
                    ModalAnimation::Slide => 0,
                    ModalAnimation::Fade => 1,
                    ModalAnimation::None => 2,
                });
                e.int(match config.presentation {
                    ModalPresentation::FullScreen => 0,
                    ModalPresentation::PageSheet => 1,
                    ModalPresentation::FormSheet => 2,
                });
                e.bool(config.transparent);
            }
            MountItem::LayoutAnimation(animation) => {
                e.int(LAYOUT_ANIMATION);
                for t in [animation.create, animation.update, animation.delete] {
                    layout_transition(&mut e, t);
                }
            }
            item @ MountItem::Anim(_) => e.anim.push(item),
            MountItem::Input { id, config } => {
                e.int(INPUT);
                e.int(id as i32);
                input(&mut e, &config);
            }
            MountItem::InputValue { id, text, count } => {
                e.int(INPUT_VALUE);
                e.int(id as i32);
                e.string(text);
                e.int(count as i32);
            }
            MountItem::InputContent { id, content } => {
                e.int(INPUT_CONTENT);
                e.int(id as i32);
                for v in [content.x, content.y, content.width, content.height] {
                    e.float(v);
                }
            }
            MountItem::Command { id, command } => {
                e.int(COMMAND);
                e.int(id as i32);
                match command {
                    Command::Focus => e.int(0),
                    Command::Blur => e.int(1),
                    Command::ScrollTo { x, y, animated } => {
                        e.int(2);
                        e.float(x);
                        e.float(y);
                        e.bool(animated);
                    }
                    Command::ExitApp => e.int(3),
                    Command::Reload => e.int(4),
                    Command::GoBack => e.int(5),
                    Command::GoForward => e.int(6),
                    Command::StopLoading => e.int(7),
                    Command::InjectJavaScript(js) => {
                        e.int(8);
                        e.string(js);
                    }
                    Command::PostMessage(data) => {
                        e.int(9);
                        e.string(data);
                    }
                    Command::Seek(seconds) => {
                        e.int(10);
                        e.float(seconds as f32);
                    }
                    Command::StatusBar(c) => {
                        e.int(11);
                        e.int(match c.style {
                            StatusBarStyle::Default => 0,
                            StatusBarStyle::Light => 1,
                            StatusBarStyle::Dark => 2,
                        });
                        e.bool(c.hidden);
                        e.bool(c.animated);
                        opt_color(&mut e, c.background);
                    }
                    Command::Dialog(d) => {
                        e.int(12);
                        e.int(d.id as i32);
                        e.opt_string(d.title.as_deref());
                        e.opt_string(d.message.as_deref());
                        e.bool(d.cancelable);
                        e.bool(d.sheet);
                        e.int(d.buttons.len() as i32);
                        for b in d.buttons {
                            e.string(b.text);
                            e.int(match b.style {
                                DialogButtonStyle::Default => 0,
                                DialogButtonStyle::Cancel => 1,
                                DialogButtonStyle::Destructive => 2,
                            });
                        }
                    }
                    Command::Toast { text, long } => {
                        e.int(13);
                        e.string(text);
                        e.bool(long);
                    }
                    Command::SetJsResponder { block } => {
                        e.int(14);
                        e.bool(block);
                    }
                    Command::ClearJsResponder => e.int(15),
                    Command::SetSelection { start, end } => {
                        e.int(16);
                        e.int(start as i32);
                        e.int(end as i32);
                    }
                    // Turned into a mount item / `ScrollTo` by the core.
                    Command::LayoutAnimation(_)
                    | Command::ScrollToIndex { .. }
                    | Command::ScrollToEnd { .. } => e.int(-1),
                }
            }
        }
    }
    e
}

/// The event kind with index `index` (`EventKind as u8`, the order of the
/// constants in `Native.java`).
pub(crate) fn kind_from(index: i32) -> Option<yf_core::EventKind> {
    yf_core::EventKind::ALL
        .get(usize::try_from(index).ok()?)
        .copied()
}

fn view_style(e: &mut Encoded, s: &ViewStyle) {
    e.int(argb(s.background_color));
    let radii = s.radii();
    for r in radii.to_array() {
        e.float(r);
    }
    let b = &s.borders;
    for w in b.widths {
        e.float(w);
    }
    for c in b.colors {
        e.int(argb(c));
    }
    e.int(match b.style {
        BorderStyle::Solid => 0,
        BorderStyle::Dashed => 1,
        BorderStyle::Dotted => 2,
    });
    e.float(s.opacity);
    e.bool(s.overflow_hidden);
    let shadow = s.shadow;
    e.int(argb(shadow.color));
    e.float(shadow.offset_x);
    e.float(shadow.offset_y);
    e.float(if shadow.is_visible() {
        shadow.opacity
    } else {
        0.0
    });
    e.float(shadow.radius);
    for v in view_transform(&s.transform, |_| None) {
        e.float(v);
    }
    e.bool(s.tint_color.is_some());
    e.int(s.tint_color.map_or(0, argb));
    e.float(s.elevation);
    // Transform origin: kind (0 points, 1 fraction) and value per axis.
    for v in [s.transform_origin.x, s.transform_origin.y] {
        match v {
            OriginLen::Px(p) => {
                e.int(0);
                e.float(p);
            }
            OriginLen::Fraction(f) => {
                e.int(1);
                e.float(f);
            }
        }
    }
    e.bool(s.backface_hidden);
    effects(e, s);
}

/// The effects of `Mounter.java` `style`: shadows, gradients, filters,
/// blend mode, outline, visibility, cursor, transition, animation.
fn effects(e: &mut Encoded, s: &ViewStyle) {
    use yf_core::{BlendMode, Cursor, Filter, GradientKind, GradientSize, Keyframes};
    let fx = s.effects.as_deref();
    let shadows = fx.map_or(&[][..], |f| &f.box_shadows[..]);
    e.int(shadows.len() as i32);
    for sh in shadows {
        for v in [sh.offset_x, sh.offset_y, sh.blur, sh.spread] {
            e.float(v);
        }
        e.int(argb(sh.color));
        e.bool(sh.inset);
    }
    let gradients = fx.map_or(&[][..], |f| &f.background_images[..]);
    e.int(gradients.len() as i32);
    for g in gradients {
        match g.kind {
            GradientKind::Linear { angle } => {
                e.int(0);
                e.float(angle);
                e.int(0);
                e.int(0);
            }
            GradientKind::LinearCorner { right, bottom } => {
                e.int(1);
                e.float(0.0);
                e.bool(right);
                e.bool(bottom);
            }
            GradientKind::Radial { circle, size, x, y } => {
                e.int(2);
                e.bool(circle);
                e.int(match size {
                    GradientSize::ClosestSide => 0,
                    GradientSize::ClosestCorner => 1,
                    GradientSize::FarthestSide => 2,
                    GradientSize::FarthestCorner => 3,
                });
                e.float(x);
                e.float(y);
            }
            GradientKind::Conic { from, x, y } => {
                e.int(3);
                e.float(from);
                e.float(x);
                e.float(y);
            }
        }
        let stops = g.resolved_stops();
        e.int(stops.len() as i32);
        for (c, p) in stops {
            e.int(argb(c));
            e.float(p);
        }
    }
    let filter_list = |e: &mut Encoded, list: &[Filter]| {
        // Color filters as one composed matrix, then blur, then the drop
        // shadow (what Android's render effects take).
        let matrix = yf_core::compose_color_matrices(list);
        e.bool(matrix.is_some());
        for v in matrix.unwrap_or([0.0; 20]) {
            e.float(v);
        }
        e.float(yf_core::blur_radius(list));
        let shadow = list.iter().find_map(|f| match *f {
            Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => Some((offset_x, offset_y, blur, color)),
            _ => None,
        });
        e.bool(shadow.is_some());
        let (x, y, b, c) = shadow.unwrap_or((0.0, 0.0, 0.0, Color::TRANSPARENT));
        e.float(x);
        e.float(y);
        e.float(b);
        e.int(argb(c));
    };
    filter_list(e, fx.map_or(&[][..], |f| &f.filters[..]));
    filter_list(e, fx.map_or(&[][..], |f| &f.backdrop_filters[..]));
    e.int(match s.blend_mode {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::ColorDodge => 6,
        BlendMode::ColorBurn => 7,
        BlendMode::HardLight => 8,
        BlendMode::SoftLight => 9,
        BlendMode::Difference => 10,
        BlendMode::Exclusion => 11,
        BlendMode::Hue => 12,
        BlendMode::Saturation => 13,
        BlendMode::Color => 14,
        BlendMode::Luminosity => 15,
        BlendMode::PlusDarker => 16,
        BlendMode::PlusLighter => 17,
    });
    let o = s.outline;
    e.float(if o.is_visible() { o.width } else { 0.0 });
    e.int(argb(o.color));
    e.int(match o.style {
        BorderStyle::Solid => 0,
        BorderStyle::Dashed => 1,
        BorderStyle::Dotted => 2,
    });
    e.float(o.offset);
    e.bool(s.hidden);
    e.int(match s.cursor {
        Cursor::Auto => 0,
        Cursor::Default => 1,
        Cursor::Pointer => 2,
        Cursor::Text => 3,
        Cursor::Wait => 4,
        Cursor::Progress => 5,
        Cursor::Help => 6,
        Cursor::Crosshair => 7,
        Cursor::Move => 8,
        Cursor::Grab => 9,
        Cursor::Grabbing => 10,
        Cursor::NotAllowed => 11,
        Cursor::ZoomIn => 12,
        Cursor::ZoomOut => 13,
        Cursor::ColResize => 14,
        Cursor::RowResize => 15,
        Cursor::None => 16,
    });
    match s.transition.filter(|t| t.duration > 0.0) {
        Some(t) => {
            e.int(t.properties.0 as i32);
            e.float(t.duration);
            e.float(t.delay);
            for v in t.easing {
                e.float(v);
            }
        }
        None => {
            e.int(0);
            for _ in 0..6 {
                e.float(0.0);
            }
        }
    }
    match s.animation {
        Some(a) => {
            e.int(match a.keyframes {
                Keyframes::Spin => 1,
                Keyframes::Ping => 2,
                Keyframes::Pulse => 3,
                Keyframes::Bounce => 4,
            });
            e.float(a.duration);
            e.float(a.delay);
            for v in a.easing {
                e.float(v);
            }
            e.int(a.iterations.map_or(-1, |n| n as i32));
        }
        None => {
            e.int(0);
            for _ in 0..6 {
                e.float(0.0);
            }
            e.int(0);
        }
    }
}

fn opt_color(e: &mut Encoded, c: Option<Color>) {
    e.bool(c.is_some());
    e.int(c.map_or(0, argb));
}

/// `Mounter.java` `nativeProps`: a kind, then its fields.
fn native_props(e: &mut Encoded, p: &NativeProps) {
    match p {
        NativeProps::Switch(s) => {
            e.int(0);
            e.bool(s.value);
            e.bool(s.disabled);
            opt_color(e, s.track_on);
            opt_color(e, s.track_off);
            opt_color(e, s.thumb);
        }
        NativeProps::Slider(s) => {
            e.int(1);
            for v in [s.value, s.min, s.max, s.step] {
                e.float(v);
            }
            e.bool(s.disabled);
            opt_color(e, s.min_track);
            opt_color(e, s.max_track);
            opt_color(e, s.thumb);
        }
        NativeProps::Activity(a) => {
            e.int(2);
            e.bool(a.animating);
            opt_color(e, a.color);
            e.bool(a.large);
            e.bool(a.hides_when_stopped);
        }
        NativeProps::WebView(w) => {
            e.int(3);
            match &w.source {
                WebSource::None => e.int(0),
                WebSource::Uri(uri) => {
                    e.int(1);
                    e.string(uri.to_string());
                }
                WebSource::Html { html, base_url } => {
                    e.int(2);
                    e.string(html.to_string());
                    e.opt_string(base_url.as_deref());
                }
            }
            e.opt_string(w.injected_js.as_deref());
            e.opt_string(w.injected_js_before.as_deref());
            e.opt_string(w.user_agent.as_deref());
            e.bool(w.scroll_enabled);
            e.bool(w.javascript_enabled);
        }
        NativeProps::Video(v) => {
            e.int(4);
            e.opt_string(v.source.as_deref());
            e.bool(v.paused);
            e.bool(v.muted);
            e.float(v.volume);
            e.float(v.rate);
            e.bool(v.repeat);
            e.int(match v.resize_mode {
                ResizeMode::Cover => 0,
                ResizeMode::Stretch => 2,
                _ => 1,
            });
            e.bool(v.controls);
            e.float(v.progress_interval_ms);
        }
    }
}

fn screen_config(e: &mut Encoded, c: &ScreenConfig) {
    e.opt_string(c.title.as_deref());
    e.bool(c.header_shown);
    e.bool(c.large_title);
    e.bool(c.header_transparent);
    opt_color(e, c.tint_color);
    opt_color(e, c.background_color);
    opt_color(e, c.title_color);
    e.bool(c.back_visible);
    e.bool(c.gesture_enabled);
    e.int(match c.animation {
        ScreenAnimation::Default => 0,
        ScreenAnimation::Fade => 1,
        ScreenAnimation::None => 2,
    });
    for id in [c.left, c.right, c.title_view] {
        e.int(id.map_or(-1, |i| i as i32));
    }
    e.float(c.header_height);
}

fn view_props(e: &mut Encoded, p: &ViewProps) {
    e.int(match p.pointer_events {
        PointerEvents::Auto => 0,
        PointerEvents::None => 1,
        PointerEvents::BoxNone => 2,
        PointerEvents::BoxOnly => 3,
    });
    let h = p.hit_slop;
    for v in [h.top, h.right, h.bottom, h.left] {
        e.float(v);
    }
    e.float(p.active_opacity.unwrap_or(f32::NAN));
    let a = &p.accessibility;
    e.int(match a.accessible {
        None => -1,
        Some(false) => 0,
        Some(true) => 1,
    });
    e.opt_string(a.label.as_deref());
    e.opt_string(a.hint.as_deref());
    e.int(role_index(a.role));
    let s = a.state;
    let mut flags = 0;
    if s.disabled {
        flags |= 1;
    }
    if s.selected {
        flags |= 2;
    }
    if s.busy {
        flags |= 4;
    }
    match s.checked {
        Some(Checked::False) => flags |= 8,
        Some(Checked::True) => flags |= 8 | 16,
        Some(Checked::Mixed) => flags |= 8 | 32,
        None => {}
    }
    match s.expanded {
        Some(false) => flags |= 64,
        Some(true) => flags |= 64 | 128,
        None => {}
    }
    e.int(flags);
    match &a.value {
        Some(v) => {
            e.int(1);
            for n in [v.min, v.max, v.now] {
                e.float(n.unwrap_or(f32::NAN));
            }
            e.opt_string(v.text.as_deref());
        }
        None => e.int(0),
    }
    e.bool(a.hidden);
    e.int(match a.live_region {
        LiveRegion::None => 0,
        LiveRegion::Polite => 1,
        LiveRegion::Assertive => 2,
    });
    e.int(a.actions.len() as i32);
    for action in &a.actions {
        e.string(action.name.to_string());
        e.opt_string(action.label.as_deref());
    }
    e.opt_string(p.test_id.as_deref());
    e.float(p.long_press_delay.unwrap_or(-1.0));
    match p.ripple {
        Some(r) => {
            e.int(1 | (r.borderless as i32) << 1 | (r.foreground as i32) << 2);
            opt_color(e, r.color);
            e.float(r.radius.unwrap_or(-1.0));
        }
        None => e.int(0),
    }
}

/// Index of `role` in `A11y.java`'s class name table.
fn role_index(role: AccessibilityRole) -> i32 {
    use AccessibilityRole::*;
    match role {
        None => 0,
        Button => 1,
        Link => 2,
        Search => 3,
        Image => 4,
        ImageButton => 5,
        KeyboardKey => 6,
        Text => 7,
        Adjustable => 8,
        Header => 9,
        Summary => 10,
        Alert => 11,
        Checkbox => 12,
        Combobox => 13,
        Menu => 14,
        Menubar => 15,
        Menuitem => 16,
        Progressbar => 17,
        Radio => 18,
        Radiogroup => 19,
        Scrollbar => 20,
        Spinbutton => 21,
        Switch => 22,
        Tab => 23,
        Tablist => 24,
        Timer => 25,
        Toolbar => 26,
        List => 27,
        Grid => 28,
    }
}

fn layout_transition(e: &mut Encoded, t: Option<LayoutTransition>) {
    let Some(t) = t else {
        return e.int(0);
    };
    e.int(1);
    let (curve, damping) = match t.curve {
        LayoutCurve::Linear => (0, 0.0),
        LayoutCurve::EaseIn => (1, 0.0),
        LayoutCurve::EaseOut => (2, 0.0),
        LayoutCurve::EaseInEaseOut => (3, 0.0),
        LayoutCurve::Spring { damping } => (4, damping),
    };
    e.int(curve);
    e.float(damping);
    e.int(match t.property {
        LayoutProperty::Opacity => 0,
        LayoutProperty::Scale => 1,
    });
    e.float(t.duration_ms);
    e.float(t.delay_ms);
}

fn input(e: &mut Encoded, c: &InputConfig) {
    let s = &c.style;
    e.float(s.font_size);
    e.int(s.font_weight as i32);
    e.bool(s.italic);
    e.int(argb(s.color));
    match s.font_family {
        Some(f) => e.string(f.name().to_string()),
        None => e.int(-1),
    }
    e.float(s.letter_spacing);
    e.int(match c.align {
        TextAlign::Auto | TextAlign::Left | TextAlign::Justify => 0,
        TextAlign::Center => 1,
        TextAlign::Right => 2,
    });
    e.string(c.placeholder.clone());
    e.bool(c.placeholder_color.is_some());
    e.int(c.placeholder_color.map_or(0, argb));
    e.bool(c.secure);
    e.int(match c.keyboard {
        KeyboardType::Default => 0,
        KeyboardType::Number => 1,
        KeyboardType::Decimal => 2,
        KeyboardType::Email => 3,
        KeyboardType::Phone => 4,
        KeyboardType::Url => 5,
    });
    e.int(match c.return_key {
        ReturnKeyType::Default => 0,
        ReturnKeyType::Done => 1,
        ReturnKeyType::Go => 2,
        ReturnKeyType::Next => 3,
        ReturnKeyType::Search => 4,
        ReturnKeyType::Send => 5,
    });
    e.int(match c.auto_capitalize {
        AutoCapitalize::None => 0,
        AutoCapitalize::Sentences => 1,
        AutoCapitalize::Words => 2,
        AutoCapitalize::Characters => 3,
    });
    e.bool(c.auto_correct);
    e.bool(c.multiline);
    e.int(c.max_length.map_or(-1, |m| m as i32));
    e.bool(c.editable);
    e.bool(c.auto_focus);
    e.bool(c.selection_color.is_some());
    e.int(c.selection_color.map_or(0, argb));
    e.bool(c.blur_on_submit);
    // Android autofill hints ("" for none).
    e.opt_string(match c.auto_fill {
        AutoFill::Default => None,
        AutoFill::Off => Some(""),
        AutoFill::Username => Some("username"),
        AutoFill::Password => Some("password"),
        AutoFill::NewPassword => Some("newPassword"),
        AutoFill::OneTimeCode => Some("smsOTPCode"),
        AutoFill::Email => Some("emailAddress"),
        AutoFill::Tel => Some("phone"),
        AutoFill::Name => Some("personName"),
        AutoFill::GivenName => Some("personGivenName"),
        AutoFill::FamilyName => Some("personFamilyName"),
        AutoFill::MiddleName => Some("personMiddleName"),
        AutoFill::Nickname => Some("nickname"),
        AutoFill::Organization => Some("organization"),
        AutoFill::StreetAddress => Some("postalAddress"),
        AutoFill::AddressLine1 => Some("streetAddress"),
        AutoFill::AddressLine2 => Some("extendedAddress"),
        AutoFill::City => Some("addressLocality"),
        AutoFill::Region => Some("addressRegion"),
        AutoFill::PostalCode => Some("postalCode"),
        AutoFill::Country => Some("addressCountry"),
        AutoFill::CreditCardNumber => Some("creditCardNumber"),
        AutoFill::CreditCardExpiration => Some("creditCardExpirationDate"),
        AutoFill::CreditCardSecurityCode => Some("creditCardSecurityCode"),
        AutoFill::CreditCardName => Some("creditCardHolderName"),
        AutoFill::Url => Some("url"),
        AutoFill::Birthdate => Some("birthDateFull"),
    });
    e.bool(c.rtl);
}
