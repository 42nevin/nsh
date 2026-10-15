//! Decoder for the flat op buffer filled by `js/runtime/index.ts`.
//!
//! The buffer is a plain JS array of `[opcode, args...]` records. Keeping
//! the JS side to `array.push` of raw values is deliberate: QuickJS is the
//! slowest stage, so all parsing (tags, style objects, event names) happens
//! here in Rust.

use rquickjs::{Array, Ctx, Exception, Object, Type, Value};
use rustc_hash::FxHashSet;
use yf_anim::{
    AnimOp, AnimProp, Curve, Driver, EaseMode, Easing, Extrapolate, MathOp, Operand, Output,
};
use yf_core::{
    AccessibilityActionDef, AccessibilityProp, AccessibilityRole, AccessibilityState,
    AccessibilityValue, AutoCapitalize, AutoFill, CellSlot, Checked, Color, Command, Dialog,
    DialogButton, DialogButtonStyle, EdgeInsets, EllipsizeMode, EventKind, HeaderSlot, ImageProp,
    ImageSource, KeyboardDismissMode, KeyboardTaps, KeyboardType, LayoutAnimation, LayoutCurve,
    LayoutProperty, LayoutTransition, ListContentStyle, LiveRegion, ModalAnimation, ModalConfig,
    ModalPresentation, ModalProp, NativeProp, NodeId, Op, PointerEvents, Prop, ResizeMode,
    ReturnKeyType, ScreenAnimation, ScreenProp, ScrollProp, SnapAlignment, StatusBarConfig,
    StatusBarStyle, Style, StyleDecl, StyleError, StylePatch, StyleValue, Tag, TransformKind,
    Viewability, WebSource,
};

// Must match the constants in `js/runtime/index.ts`.
const OP_CREATE: i32 = 0;
const OP_CREATE_TEXT: i32 = 1;
const OP_SET_TEXT: i32 = 2;
const OP_SET_PROP: i32 = 3;
const OP_INSERT: i32 = 4;
const OP_REMOVE: i32 = 5;
const OP_RELEASE: i32 = 6;
const OP_ANIM: i32 = 7;
const OP_COMMAND: i32 = 8;

// Sub-opcodes of `OP_ANIM`.
const ANIM_VALUE: i32 = 0;
const ANIM_INTERPOLATE: i32 = 1;
const ANIM_MATH: i32 = 2;
const ANIM_SET: i32 = 3;
const ANIM_START: i32 = 4;
const ANIM_STOP: i32 = 5;
const ANIM_RELEASE: i32 = 6;
const ANIM_DIFF_CLAMP: i32 = 7;
const ANIM_START_COMPOSITE: i32 = 8;
const ANIM_STOP_ANIMATION: i32 = 9;
const ANIM_RESET_COMPOSITE: i32 = 10;
const ANIM_RELEASE_COMPOSITE: i32 = 11;

/// Reports each distinct problem once, so a bad style in a list does not
/// flood the log.
#[derive(Default)]
pub(crate) struct Warnings {
    seen: FxHashSet<String>,
}

impl Warnings {
    fn warn(&mut self, message: String) {
        if self.seen.len() < 1024 && self.seen.insert(message.clone()) {
            log::warn!("{message}");
        }
    }
}

struct Reader<'a, 'js> {
    ctx: &'a Ctx<'js>,
    buf: &'a Array<'js>,
    pos: usize,
}

impl<'js> Reader<'_, 'js> {
    #[inline]
    fn value(&mut self) -> rquickjs::Result<Value<'js>> {
        let v = self.buf.get::<Value>(self.pos)?;
        self.pos += 1;
        Ok(v)
    }

    #[inline]
    fn int(&mut self) -> rquickjs::Result<i32> {
        let v = self.value()?;
        v.as_int()
            .or_else(|| v.as_float().map(|f| f as i32))
            .ok_or_else(|| Exception::throw_type(self.ctx, "yf op buffer: expected an integer"))
    }

    #[inline]
    fn id(&mut self) -> rquickjs::Result<NodeId> {
        Ok(self.int()? as NodeId)
    }

    #[inline]
    fn number(&mut self) -> rquickjs::Result<f32> {
        let v = self.value()?;
        v.as_number()
            .map(|n| n as f32)
            .ok_or_else(|| Exception::throw_type(self.ctx, "yf op buffer: expected a number"))
    }

    fn numbers(&mut self) -> rquickjs::Result<Vec<f32>> {
        let v = self.value()?;
        let Some(items) = v.as_array() else {
            return Err(Exception::throw_type(
                self.ctx,
                "yf op buffer: expected an array",
            ));
        };
        items.iter::<f64>().map(|v| v.map(|v| v as f32)).collect()
    }

    /// A math operand: a number or a native value.
    fn operand(&mut self) -> rquickjs::Result<Operand> {
        let v = self.value()?;
        match (v.as_number(), native_value(&v)) {
            (Some(n), _) => Ok(Operand::Const(n as f32)),
            (_, Some(node)) => Ok(Operand::Node(node)),
            _ => Err(Exception::throw_type(
                self.ctx,
                "yf op buffer: expected a number or native value",
            )),
        }
    }

    #[inline]
    fn string(&mut self) -> rquickjs::Result<String> {
        let v = self.value()?;
        match v.as_string() {
            Some(s) => s.to_string(),
            None => Err(Exception::throw_type(
                self.ctx,
                "yf op buffer: expected a string",
            )),
        }
    }
}

/// Decodes the whole buffer into `out`.
pub(crate) fn decode<'js>(
    ctx: &Ctx<'js>,
    buf: &Array<'js>,
    out: &mut Vec<Op>,
    warnings: &mut Warnings,
) -> rquickjs::Result<()> {
    let len = buf.len();
    let mut r = Reader { ctx, buf, pos: 0 };
    while r.pos < len {
        let op = match r.int()? {
            OP_CREATE => {
                let id = r.id()?;
                let name = r.string()?;
                let tag = Tag::from_name(&name).unwrap_or_else(|| {
                    warnings.warn(format!("unknown element <{name}>, rendering it as <view>"));
                    Tag::View
                });
                Op::Create { id, tag }
            }
            OP_CREATE_TEXT => Op::CreateText {
                id: r.id()?,
                text: r.string()?,
            },
            OP_SET_TEXT => Op::SetText {
                id: r.id()?,
                text: r.string()?,
            },
            OP_SET_PROP => {
                let id = r.id()?;
                let name = r.string()?;
                let value = r.value()?;
                match decode_prop(id, &name, value, warnings)? {
                    Some(op) => op,
                    None => continue,
                }
            }
            OP_INSERT => {
                let parent = r.id()?;
                let child = r.id()?;
                let before = r.id()?;
                Op::Insert {
                    parent,
                    child,
                    before: (before != 0).then_some(before),
                }
            }
            OP_REMOVE => Op::Remove {
                parent: r.id()?,
                child: r.id()?,
            },
            OP_RELEASE => Op::Release { id: r.id()? },
            OP_ANIM => Op::Anim(decode_anim(&mut r)?),
            OP_COMMAND => {
                let id = r.id()?;
                let name = r.string()?;
                let arg = r.value()?;
                let command = match name.as_str() {
                    "focus" => Command::Focus,
                    "blur" => Command::Blur,
                    "exitApp" => Command::ExitApp,
                    "reload" => Command::Reload,
                    "goBack" => Command::GoBack,
                    "goForward" => Command::GoForward,
                    "stopLoading" => Command::StopLoading,
                    "injectJavaScript" => Command::InjectJavaScript(string(&arg)?),
                    "postMessage" => Command::PostMessage(string(&arg)?),
                    "seek" => Command::Seek(arg.as_number().unwrap_or(0.0)),
                    "statusBar" => {
                        let o = arg.as_object();
                        let flag = |key: &str| {
                            o.and_then(|o| o.get::<_, Option<bool>>(key).ok().flatten())
                                .unwrap_or(false)
                        };
                        let text = |key: &str| {
                            o.and_then(|o| o.get::<_, Option<String>>(key).ok().flatten())
                        };
                        Command::StatusBar(StatusBarConfig {
                            style: match text("barStyle").as_deref() {
                                Some("light-content") => StatusBarStyle::Light,
                                Some("dark-content") => StatusBarStyle::Dark,
                                _ => StatusBarStyle::Default,
                            },
                            hidden: flag("hidden"),
                            animated: flag("animated"),
                            background: text("backgroundColor").and_then(|c| Color::parse(&c)),
                        })
                    }
                    "dialog" => Command::Dialog(Box::new(decode_dialog(&arg)?)),
                    "setJsResponder" => Command::SetJsResponder {
                        block: arg.as_bool().unwrap_or(true),
                    },
                    "clearJsResponder" => Command::ClearJsResponder,
                    "setSelection" => {
                        let field = |key: &str| {
                            arg.as_object()
                                .and_then(|o| o.get::<_, Option<f64>>(key).ok().flatten())
                                .unwrap_or(0.0)
                                .max(0.0) as u32
                        };
                        Command::SetSelection {
                            start: field("start"),
                            end: field("end"),
                        }
                    }
                    "announce" => Command::Announce(
                        arg.as_string()
                            .map(|s| s.to_string())
                            .transpose()?
                            .unwrap_or_default(),
                    ),
                    "accessibilityFocus" => Command::AccessibilityFocus,
                    "toast" => {
                        let o = arg.as_object();
                        Command::Toast {
                            text: o
                                .and_then(|o| o.get::<_, Option<String>>("text").ok().flatten())
                                .unwrap_or_default(),
                            long: o
                                .and_then(|o| o.get::<_, Option<bool>>("long").ok().flatten())
                                .unwrap_or(false),
                        }
                    }
                    "scrollToIndex" => {
                        let o = arg.as_object();
                        let f = |key: &str| -> rquickjs::Result<Option<f32>> {
                            o.map_or(Ok(None), |o| field_f32(o, key))
                        };
                        Command::ScrollToIndex {
                            index: f("index")?.unwrap_or(0.0).max(0.0) as u32,
                            animated: o
                                .and_then(|o| o.get::<_, Option<bool>>("animated").ok().flatten())
                                .unwrap_or(true),
                            view_position: f("viewPosition")?.unwrap_or(0.0),
                            view_offset: f("viewOffset")?.unwrap_or(0.0),
                        }
                    }
                    "scrollToEnd" => Command::ScrollToEnd {
                        animated: arg
                            .as_object()
                            .and_then(|o| o.get::<_, Option<bool>>("animated").ok().flatten())
                            .unwrap_or(true),
                    },
                    "layoutAnimation" => Command::LayoutAnimation(decode_layout_animation(&arg)?),
                    "scrollTo" => {
                        let field = |key: &str| {
                            arg.as_object()
                                .and_then(|o| o.get::<_, Option<f64>>(key).ok().flatten())
                        };
                        Command::ScrollTo {
                            x: field("x").unwrap_or(0.0) as f32,
                            y: field("y").unwrap_or(0.0) as f32,
                            animated: arg
                                .as_object()
                                .and_then(|o| o.get::<_, Option<bool>>("animated").ok().flatten())
                                .unwrap_or(true),
                        }
                    }
                    other => {
                        warnings.warn(format!("unknown command `{other}`"));
                        continue;
                    }
                };
                Op::Command { id, command }
            }
            other => {
                return Err(Exception::throw_type(
                    ctx,
                    &format!("yf op buffer: unknown opcode {other}"),
                ));
            }
        };
        out.push(op);
    }
    Ok(())
}

/// Reads the node id of a native value object (`{ __nv: id }`).
fn native_value(value: &Value<'_>) -> Option<yf_anim::NodeId> {
    value
        .as_object()?
        .get::<_, Option<u32>>("__nv")
        .ok()
        .flatten()
}

fn number(value: &Value<'_>) -> Option<f32> {
    value.as_number().map(|n| n as f32)
}

/// A string prop; anything else (null, undefined) is the empty string.
fn string(value: &Value<'_>) -> rquickjs::Result<String> {
    match value.as_string() {
        Some(s) => s.to_string(),
        None => Ok(String::new()),
    }
}

/// A string prop; `null` / `undefined` / `""` are `None`.
fn opt_string(value: &Value<'_>) -> rquickjs::Result<Option<std::sync::Arc<str>>> {
    Ok(match value.as_string() {
        Some(s) => {
            let s = s.to_string()?;
            (!s.is_empty()).then(|| s.into())
        }
        None => None,
    })
}

/// The native value in field `key` of an object prop (`pinch={{ scale }}`).
fn field_value(value: &Value<'_>, key: &str) -> Option<yf_anim::NodeId> {
    value
        .as_object()
        .and_then(|o| o.get::<_, Value>(key).ok())
        .and_then(|v| native_value(&v))
}

fn field_f32(obj: &Object<'_>, key: &str) -> rquickjs::Result<Option<f32>> {
    Ok(obj.get::<_, Option<f64>>(key)?.map(|v| v as f32))
}

/// A list's `contentContainerStyle`: the padding properties (numbers) and
/// `gap` / `rowGap` / `columnGap`.
fn list_content(value: &Value<'_>) -> rquickjs::Result<ListContentStyle> {
    let Some(o) = value.as_object() else {
        return Ok(ListContentStyle::default());
    };
    let all = field_f32(o, "padding")?.unwrap_or(0.0);
    let horizontal = field_f32(o, "paddingHorizontal")?.unwrap_or(all);
    let vertical = field_f32(o, "paddingVertical")?.unwrap_or(all);
    let gap = field_f32(o, "gap")?.unwrap_or(0.0);
    Ok(ListContentStyle {
        padding: EdgeInsets {
            top: field_f32(o, "paddingTop")?.unwrap_or(vertical),
            right: field_f32(o, "paddingRight")?.unwrap_or(horizontal),
            bottom: field_f32(o, "paddingBottom")?.unwrap_or(vertical),
            left: field_f32(o, "paddingLeft")?.unwrap_or(horizontal),
        },
        row_gap: field_f32(o, "rowGap")?.unwrap_or(gap),
        column_gap: field_f32(o, "columnGap")?.unwrap_or(gap),
    })
}

/// `{ top, right, bottom, left }` (missing edges are 0).
fn insets(value: &Value<'_>) -> rquickjs::Result<EdgeInsets> {
    let Some(o) = value.as_object() else {
        return Ok(EdgeInsets::default());
    };
    Ok(EdgeInsets {
        top: field_f32(o, "top")?.unwrap_or(0.0),
        right: field_f32(o, "right")?.unwrap_or(0.0),
        bottom: field_f32(o, "bottom")?.unwrap_or(0.0),
        left: field_f32(o, "left")?.unwrap_or(0.0),
    })
}

fn a11y_state(value: &Value<'_>) -> rquickjs::Result<AccessibilityState> {
    let Some(o) = value.as_object() else {
        return Ok(AccessibilityState::default());
    };
    let flag = |key: &str| -> rquickjs::Result<bool> {
        Ok(o.get::<_, Option<bool>>(key)
            .ok()
            .flatten()
            .unwrap_or(false))
    };
    let checked: Value = o.get("checked")?;
    Ok(AccessibilityState {
        disabled: flag("disabled")?,
        selected: flag("selected")?,
        checked: match checked.as_bool() {
            Some(true) => Some(Checked::True),
            Some(false) => Some(Checked::False),
            None if checked
                .as_string()
                .and_then(|s| s.to_string().ok())
                .as_deref()
                == Some("mixed") =>
            {
                Some(Checked::Mixed)
            }
            None => None,
        },
        busy: flag("busy")?,
        expanded: o.get::<_, Option<bool>>("expanded").ok().flatten(),
    })
}

fn a11y_value(value: &Value<'_>) -> rquickjs::Result<Option<AccessibilityValue>> {
    let Some(o) = value.as_object() else {
        return Ok(None);
    };
    let text: Value = o.get("text")?;
    Ok(Some(AccessibilityValue {
        min: field_f32(o, "min")?,
        max: field_f32(o, "max")?,
        now: field_f32(o, "now")?,
        text: opt_string(&text)?,
    }))
}

fn a11y_actions(value: &Value<'_>) -> rquickjs::Result<Vec<AccessibilityActionDef>> {
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter::<Object>() {
        let item = item?;
        let name: Value = item.get("name")?;
        let label: Value = item.get("label")?;
        if let Some(name) = opt_string(&name)? {
            out.push(AccessibilityActionDef {
                name,
                label: opt_string(&label)?,
            });
        }
    }
    Ok(out)
}

/// `LayoutAnimation.configureNext(config)`: `{ duration, create, update,
/// delete }`, each part `{ type, property, duration, delay, springDamping }`.
fn decode_layout_animation(config: &Value<'_>) -> rquickjs::Result<LayoutAnimation> {
    let Some(c) = config.as_object() else {
        return Ok(LayoutAnimation {
            create: None,
            update: None,
            delete: None,
        });
    };
    let duration = field_f32(c, "duration")?.unwrap_or(300.0);
    let part = |key: &str,
                default_property: LayoutProperty|
     -> rquickjs::Result<Option<LayoutTransition>> {
        let Some(p) = c.get::<_, Option<Object>>(key)? else {
            return Ok(None);
        };
        let kind: Option<String> = p.get("type")?;
        let property: Option<String> = p.get("property")?;
        Ok(Some(LayoutTransition {
            curve: match kind.as_deref() {
                Some("linear") => LayoutCurve::Linear,
                Some("easeIn") => LayoutCurve::EaseIn,
                Some("easeOut") => LayoutCurve::EaseOut,
                Some("spring") => LayoutCurve::Spring {
                    damping: field_f32(&p, "springDamping")?
                        .unwrap_or(0.7)
                        .clamp(0.01, 1.0),
                },
                _ => LayoutCurve::EaseInEaseOut,
            },
            property: match property.as_deref() {
                Some("scaleXY") | Some("scale") => LayoutProperty::Scale,
                Some("opacity") => LayoutProperty::Opacity,
                _ => default_property,
            },
            duration_ms: field_f32(&p, "duration")?.unwrap_or(duration),
            delay_ms: field_f32(&p, "delay")?.unwrap_or(0.0),
        }))
    };
    Ok(LayoutAnimation {
        create: part("create", LayoutProperty::Opacity)?,
        update: part("update", LayoutProperty::Opacity)?,
        delete: part("delete", LayoutProperty::Opacity)?,
    })
}

fn prop_color(value: &Value<'_>) -> rquickjs::Result<Option<Color>> {
    Ok(value
        .as_string()
        .map(|s| s.to_string())
        .transpose()?
        .and_then(|s| Color::parse(&s)))
}

fn decode_prop<'js>(
    id: NodeId,
    name: &str,
    value: Value<'js>,
    warnings: &mut Warnings,
) -> rquickjs::Result<Option<Op>> {
    let prop = match name {
        "style" => {
            let mut entries = Vec::new();
            style_entries(&value, &mut entries)?;
            let (style, patch) = decode_style(entries, warnings)?;
            return Ok(Some(Op::SetStyle { id, style, patch }));
        }
        "src" => Prop::Source(decode_source(&value)?),
        "defaultSource" => Prop::Image(ImageProp::DefaultSource(decode_source(&value)?)),
        "blurRadius" => Prop::Image(ImageProp::BlurRadius(number(&value).unwrap_or(0.0))),
        "fadeDuration" => Prop::Image(ImageProp::FadeDuration(
            number(&value).map(|ms| ms.max(0.0)),
        )),
        "resizeMode" => Prop::ResizeMode(
            match value
                .as_string()
                .map(|s| s.to_string())
                .transpose()?
                .as_deref()
            {
                Some("contain") => ResizeMode::Contain,
                Some("stretch") => ResizeMode::Stretch,
                Some("center") => ResizeMode::Center,
                _ => ResizeMode::Cover,
            },
        ),
        "horizontal" => Prop::Horizontal(value.as_bool().unwrap_or(false)),
        "showsIndicator" => Prop::ShowsIndicator(value.as_bool().unwrap_or(true)),
        "refreshing" => Prop::Refreshing(value.as_bool().unwrap_or(false)),
        "count" => Prop::Count(number(&value).unwrap_or(0.0).max(0.0) as u32),
        "estimatedItemSize" => Prop::EstimatedItemSize(number(&value).unwrap_or(50.0)),
        "slot" => Prop::Slot(match value.as_string() {
            Some(role) => match role.to_string()?.as_str() {
                "header" => CellSlot::Header,
                "footer" => CellSlot::Footer,
                _ => CellSlot::Empty,
            },
            None => CellSlot::Item(number(&value).unwrap_or(0.0) as u32),
        }),
        "splice" => {
            let at = |i: u32| {
                value
                    .as_array()
                    .and_then(|a| a.get::<f64>(i as usize).ok())
                    .unwrap_or(0.0)
                    .max(0.0) as u32
            };
            Prop::Splice(at(0), at(1), at(2))
        }
        "numColumns" => Prop::Columns(number(&value).unwrap_or(1.0).max(1.0) as u32),
        "inverted" => Prop::Inverted(value.as_bool().unwrap_or(false)),
        "stickyHeaderIndices" => Prop::StickyIndices(match value.as_array() {
            Some(a) => a
                .iter::<f64>()
                .filter_map(|v| v.ok())
                .filter(|v| *v >= 0.0)
                .map(|v| v as u32)
                .collect(),
            None => Vec::new(),
        }),
        "contentContainerStyle" => Prop::ListContent(list_content(&value)?),
        "viewabilityConfig" => Prop::Viewability(match value.as_object() {
            Some(o) => Viewability {
                view_area_percent: field_f32(o, "viewAreaCoveragePercentThreshold")?,
                item_visible_percent: field_f32(o, "itemVisiblePercentThreshold")?,
            },
            None => Viewability::default(),
        }),
        "endReachedThreshold" => Prop::EndReachedThreshold(number(&value).unwrap_or(1.0)),
        "value" => Prop::Value(string(&value)?),
        "disabled" => Prop::Native(NativeProp::Disabled(value.as_bool().unwrap_or(false))),
        "source" => Prop::Native(NativeProp::Source(web_source(&value)?)),
        "injectedJavaScript" => Prop::Native(NativeProp::InjectedJavaScript(opt_string(&value)?)),
        "injectedJavaScriptBeforeContentLoaded" => {
            Prop::Native(NativeProp::InjectedJavaScriptBefore(opt_string(&value)?))
        }
        "userAgent" => Prop::Native(NativeProp::UserAgent(opt_string(&value)?)),
        "scrollEnabled" => Prop::Native(NativeProp::ScrollEnabled(value.as_bool().unwrap_or(true))),
        "javaScriptEnabled" => Prop::Native(NativeProp::JavaScriptEnabled(
            value.as_bool().unwrap_or(true),
        )),
        "allowsBackForwardNavigationGestures" => Prop::Native(NativeProp::BackForwardGestures(
            value.as_bool().unwrap_or(false),
        )),
        "paused" => Prop::Native(NativeProp::Paused(value.as_bool().unwrap_or(false))),
        "muted" => Prop::Native(NativeProp::Muted(value.as_bool().unwrap_or(false))),
        "volume" => Prop::Native(NativeProp::Volume(number(&value).unwrap_or(1.0))),
        "rate" => Prop::Native(NativeProp::Rate(number(&value).unwrap_or(1.0))),
        "repeat" => Prop::Native(NativeProp::Repeat(value.as_bool().unwrap_or(false))),
        "controls" => Prop::Native(NativeProp::Controls(value.as_bool().unwrap_or(false))),
        "progressUpdateInterval" => Prop::Native(NativeProp::ProgressInterval(
            number(&value).unwrap_or(250.0),
        )),
        "defaultValue" => Prop::DefaultValue(string(&value)?),
        "placeholder" => Prop::Placeholder(string(&value)?),
        "placeholderTextColor" => Prop::PlaceholderColor(prop_color(&value)?),
        "selectionColor" => Prop::SelectionColor(prop_color(&value)?),
        "autoComplete" | "textContentType" => match value.as_string() {
            Some(s) => {
                let text = s.to_string()?;
                match AutoFill::parse(&text) {
                    Some(fill) => Prop::AutoFill(fill),
                    None => {
                        log::warn!("{text:?} is not a supported {name} value");
                        Prop::AutoFill(AutoFill::Default)
                    }
                }
            }
            None => Prop::AutoFill(AutoFill::Default),
        },
        "selection" => {
            let field = |key: &str| {
                value
                    .as_object()
                    .and_then(|o| o.get::<_, Option<f64>>(key).ok().flatten())
            };
            match field("start") {
                Some(start) => {
                    let start = start.max(0.0) as u32;
                    Prop::Selection(start, field("end").map_or(start, |e| e.max(0.0) as u32))
                }
                None => return Ok(None),
            }
        }
        "secureTextEntry" => Prop::Secure(value.as_bool().unwrap_or(false)),
        "autoCorrect" => Prop::AutoCorrect(value.as_bool().unwrap_or(true)),
        "multiline" => Prop::Multiline(value.as_bool().unwrap_or(false)),
        "editable" => Prop::Editable(value.as_bool().unwrap_or(true)),
        "blurOnSubmit" => Prop::BlurOnSubmit(value.as_bool().unwrap_or(true)),
        "autoFocus" => Prop::AutoFocus(value.as_bool().unwrap_or(false)),
        "inputAccessoryViewID" => {
            Prop::InputAccessoryViewId(opt_string(&value)?.map(|s| (*s).into()))
        }
        "nativeID" => Prop::NativeId(opt_string(&value)?.map(|s| (*s).into())),
        "maxLength" => Prop::MaxLength(number(&value).map(|n| n.max(0.0) as u32)),
        "keyboardType" => Prop::KeyboardType(match string(&value)?.as_str() {
            "number-pad" | "numeric" => KeyboardType::Number,
            "decimal-pad" => KeyboardType::Decimal,
            "email-address" => KeyboardType::Email,
            "phone-pad" => KeyboardType::Phone,
            "url" => KeyboardType::Url,
            _ => KeyboardType::Default,
        }),
        "returnKeyType" => Prop::ReturnKeyType(match string(&value)?.as_str() {
            "done" => ReturnKeyType::Done,
            "go" => ReturnKeyType::Go,
            "next" => ReturnKeyType::Next,
            "search" => ReturnKeyType::Search,
            "send" => ReturnKeyType::Send,
            _ => ReturnKeyType::Default,
        }),
        "autoCapitalize" => Prop::AutoCapitalize(match string(&value)?.as_str() {
            "none" => AutoCapitalize::None,
            "words" => AutoCapitalize::Words,
            "characters" => AutoCapitalize::Characters,
            _ => AutoCapitalize::Sentences,
        }),
        "keyboardDismissMode" => Prop::KeyboardDismissMode(match string(&value)?.as_str() {
            "on-drag" => KeyboardDismissMode::OnDrag,
            "interactive" => KeyboardDismissMode::Interactive,
            _ => KeyboardDismissMode::None,
        }),
        "numberOfLines" => Prop::NumberOfLines(number(&value).unwrap_or(0.0).max(0.0) as u32),
        "ellipsizeMode" => Prop::EllipsizeMode(
            match value
                .as_string()
                .map(|s| s.to_string())
                .transpose()?
                .as_deref()
            {
                Some("head") => EllipsizeMode::Head,
                Some("middle") => EllipsizeMode::Middle,
                Some("clip") => EllipsizeMode::Clip,
                _ => EllipsizeMode::Tail,
            },
        ),
        "cellIndex" => Prop::CellIndex(number(&value).unwrap_or(0.0) as u32),
        "title" => Prop::Screen(ScreenProp::Title(opt_string(&value)?)),
        "headerShown" => Prop::Screen(ScreenProp::HeaderShown(value.as_bool().unwrap_or(true))),
        "headerLargeTitle" => Prop::Screen(ScreenProp::HeaderLargeTitle(
            value.as_bool().unwrap_or(false),
        )),
        "headerTransparent" => Prop::Screen(ScreenProp::HeaderTransparent(
            value.as_bool().unwrap_or(false),
        )),
        "headerTintColor" => Prop::Screen(ScreenProp::HeaderTintColor(prop_color(&value)?)),
        "headerBackgroundColor" => {
            Prop::Screen(ScreenProp::HeaderBackgroundColor(prop_color(&value)?))
        }
        "headerTitleColor" => Prop::Screen(ScreenProp::HeaderTitleColor(prop_color(&value)?)),
        "headerBackTitle" => Prop::Screen(ScreenProp::HeaderBackTitle(opt_string(&value)?)),
        "headerBackVisible" => Prop::Screen(ScreenProp::HeaderBackVisible(
            value.as_bool().unwrap_or(true),
        )),
        "gestureEnabled" => {
            Prop::Screen(ScreenProp::GestureEnabled(value.as_bool().unwrap_or(true)))
        }
        "animation" => Prop::Screen(ScreenProp::Animation(match string(&value)?.as_str() {
            "fade" => ScreenAnimation::Fade,
            "none" => ScreenAnimation::None,
            _ => ScreenAnimation::Default,
        })),
        "animationType" => Prop::Modal(ModalProp::Animation(match string(&value)?.as_str() {
            "fade" => ModalAnimation::Fade,
            "none" => ModalAnimation::None,
            _ => ModalAnimation::Slide,
        })),
        "presentationStyle" => {
            Prop::Modal(ModalProp::Presentation(match string(&value)?.as_str() {
                "pageSheet" => ModalPresentation::PageSheet,
                "formSheet" => ModalPresentation::FormSheet,
                _ => ModalPresentation::FullScreen,
            }))
        }
        "transparent" => Prop::Modal(ModalProp::Transparent(value.as_bool().unwrap_or(false))),
        "statusBarTranslucent" => Prop::Modal(ModalProp::StatusBarTranslucent(
            value.as_bool().unwrap_or(false),
        )),
        "supportedOrientations" => Prop::Modal(ModalProp::Orientations({
            let mut bits = 0;
            if let Some(list) = value.as_array() {
                for name in list.iter::<String>() {
                    bits |= match name?.as_str() {
                        "portrait" => ModalConfig::PORTRAIT,
                        "portrait-upside-down" => ModalConfig::PORTRAIT_UPSIDE_DOWN,
                        "landscape" => ModalConfig::LANDSCAPE_LEFT | ModalConfig::LANDSCAPE_RIGHT,
                        "landscape-left" => ModalConfig::LANDSCAPE_LEFT,
                        "landscape-right" => ModalConfig::LANDSCAPE_RIGHT,
                        _ => 0,
                    };
                }
            }
            bits
        })),
        "headerSlot" => Prop::HeaderSlot(match string(&value)?.as_str() {
            "right" => HeaderSlot::Right,
            "title" => HeaderSlot::Title,
            _ => HeaderSlot::Left,
        }),
        "pointerEvents" => Prop::PointerEvents(match string(&value)?.as_str() {
            "none" => PointerEvents::None,
            "box-none" => PointerEvents::BoxNone,
            "box-only" => PointerEvents::BoxOnly,
            _ => PointerEvents::Auto,
        }),
        "hitSlop" => Prop::HitSlop(match number(&value) {
            Some(n) => EdgeInsets::all(n),
            None => insets(&value)?,
        }),
        "activeOpacity" => Prop::ActiveOpacity(number(&value).map(|n| n.clamp(0.0, 1.0))),
        "underlayColor" => Prop::UnderlayColor(prop_color(&value)?),
        "underlayOpacity" => Prop::UnderlayOpacity(
            number(&value).map_or(yf_core::UNDERLAY_OPACITY, |n| n.clamp(0.0, 1.0)),
        ),
        "insetPadding" => Prop::InsetPadding(insets(&value)?),
        "insetMargin" => Prop::InsetMargin(insets(&value)?),
        "layoutDirectionRtl" => Prop::LayoutDirection(value.as_bool().unwrap_or(false)),
        "class" => Prop::Class(match value.as_string() {
            Some(s) => s.to_string()?.into(),
            None => "".into(),
        }),
        // `{ "bg-red-500": hasError() }`: the names whose value is truthy.
        "classList" => Prop::ClassList(match value.as_object() {
            Some(o) => {
                let mut names = Vec::new();
                for entry in o.props::<String, Value>() {
                    let (name, on) = entry?;
                    if truthy(&on) {
                        names.push(name.into());
                    }
                }
                names
            }
            None => Vec::new(),
        }),
        "tailwindTheme" => match value.as_string() {
            Some(s) => Prop::TailwindTheme(s.to_string()?.into()),
            None => return Ok(None),
        },
        "delayLongPress" => Prop::LongPressDelay(number(&value).map(|n| n.max(0.0))),
        "android_ripple" => Prop::Ripple(match value.as_object() {
            Some(o) => {
                let flag = |key: &str| {
                    o.get::<_, Option<bool>>(key)
                        .ok()
                        .flatten()
                        .unwrap_or(false)
                };
                Some(yf_core::Ripple {
                    color: match o.get::<_, Option<String>>("color").ok().flatten() {
                        Some(c) => Some(Color::parse(&c).ok_or_else(|| {
                            rquickjs::Error::new_from_js_message(
                                "value",
                                "color",
                                format!("invalid color {c:?}"),
                            )
                        })?),
                        None => None,
                    },
                    borderless: flag("borderless"),
                    radius: o
                        .get::<_, Option<f64>>("radius")
                        .ok()
                        .flatten()
                        .map(|r| r as f32),
                    foreground: flag("foreground"),
                })
            }
            None => None,
        }),
        "testID" => Prop::TestId(opt_string(&value)?),
        "allowFontScaling" => Prop::AllowFontScaling(value.as_bool().unwrap_or(true)),
        "maxFontSizeMultiplier" => {
            Prop::MaxFontSizeMultiplier(number(&value).unwrap_or(0.0).max(0.0))
        }
        "pinch" => Prop::PinchValue(field_value(&value, "scale")),
        "rotation" => Prop::RotationValue(field_value(&value, "angle")),
        "accessible" => Prop::Accessibility(AccessibilityProp::Accessible(value.as_bool())),
        "accessibilityLabel" => Prop::Accessibility(AccessibilityProp::Label(opt_string(&value)?)),
        "accessibilityHint" => Prop::Accessibility(AccessibilityProp::Hint(opt_string(&value)?)),
        "accessibilityRole" | "role" => {
            let name = string(&value)?;
            Prop::Accessibility(AccessibilityProp::Role(
                AccessibilityRole::parse(&name).unwrap_or_else(|| {
                    if !name.is_empty() {
                        warnings.warn(format!("unknown accessibilityRole `{name}`"));
                    }
                    AccessibilityRole::None
                }),
            ))
        }
        "accessibilityState" => Prop::Accessibility(AccessibilityProp::State(a11y_state(&value)?)),
        "accessibilityValue" => Prop::Accessibility(AccessibilityProp::Value(a11y_value(&value)?)),
        "accessibilityElementsHidden" => {
            Prop::Accessibility(AccessibilityProp::Hidden(value.as_bool().unwrap_or(false)))
        }
        "importantForAccessibility" => Prop::Accessibility(match string(&value)?.as_str() {
            "yes" => AccessibilityProp::Important(Some(true), false),
            "no" => AccessibilityProp::Important(Some(false), false),
            "no-hide-descendants" => AccessibilityProp::Important(Some(false), true),
            _ => AccessibilityProp::Important(None, false),
        }),
        "accessibilityLiveRegion" => Prop::Accessibility(AccessibilityProp::LiveRegion(
            match string(&value)?.as_str() {
                "polite" => LiveRegion::Polite,
                "assertive" => LiveRegion::Assertive,
                _ => LiveRegion::None,
            },
        )),
        "accessibilityActions" => {
            Prop::Accessibility(AccessibilityProp::Actions(a11y_actions(&value)?))
        }
        "scrollValue" => Prop::ScrollValue(native_value(&value)),
        "pagingEnabled" => Prop::Scroll(ScrollProp::Paging(value.as_bool().unwrap_or(false))),
        "snapToInterval" => Prop::Scroll(ScrollProp::SnapInterval(number(&value).unwrap_or(0.0))),
        "snapToOffsets" => Prop::Scroll(ScrollProp::SnapOffsets(match value.as_array() {
            Some(a) => a
                .iter::<f64>()
                .filter_map(|v| v.ok())
                .map(|v| v as f32)
                .collect(),
            None => Vec::new(),
        })),
        "snapToAlignment" => {
            Prop::Scroll(ScrollProp::SnapAlignment(match string(&value)?.as_str() {
                "center" => SnapAlignment::Center,
                "end" => SnapAlignment::End,
                _ => SnapAlignment::Start,
            }))
        }
        // React Native's iOS rates: velocity kept per millisecond.
        "decelerationRate" => Prop::Scroll(ScrollProp::Deceleration(match value.as_string() {
            Some(s) => match s.to_string()?.as_str() {
                "fast" => 0.99,
                "normal" => 0.998,
                _ => 0.0,
            },
            None => number(&value).unwrap_or(0.0),
        })),
        "bounces" => Prop::Scroll(ScrollProp::Bounces(value.as_bool().unwrap_or(true))),
        "disableIntervalMomentum" => Prop::Scroll(ScrollProp::DisableIntervalMomentum(
            value.as_bool().unwrap_or(false),
        )),
        "contentInset" => Prop::Scroll(ScrollProp::ContentInset(insets(&value)?)),
        "contentOffset" => {
            let field = |key: &str| {
                value
                    .as_object()
                    .and_then(|o| o.get::<_, Option<f64>>(key).ok().flatten())
                    .unwrap_or(0.0) as f32
            };
            Prop::Scroll(ScrollProp::ContentOffset(field("x"), field("y")))
        }
        "keyboardShouldPersistTaps" => {
            Prop::Scroll(ScrollProp::KeyboardTaps(match value.as_bool() {
                Some(true) => KeyboardTaps::Always,
                Some(false) => KeyboardTaps::Never,
                None => match string(&value)?.as_str() {
                    "always" => KeyboardTaps::Always,
                    "handled" => KeyboardTaps::Handled,
                    _ => KeyboardTaps::Never,
                },
            }))
        }
        "scrollEventThrottle" => {
            Prop::Scroll(ScrollProp::EventThrottle(number(&value).unwrap_or(0.0)))
        }
        "pan" => {
            let axis = |key: &str| {
                value
                    .as_object()
                    .and_then(|o| o.get::<_, Value>(key).ok())
                    .and_then(|v| native_value(&v))
            };
            Prop::PanValues(axis("x"), axis("y"))
        }
        _ => {
            if let Some(event) = EventKind::from_prop(name) {
                return Ok(Some(Op::Listen {
                    id,
                    event,
                    enabled: value.as_bool().unwrap_or(false),
                }));
            }
            warnings.warn(format!("unsupported prop `{name}`"));
            return Ok(None);
        }
    };
    Ok(Some(Op::SetProp { id, prop }))
}

/// A web view / video `source`: `"https://…"`, `{ uri }` or
/// `{ html, baseUrl }`.
fn web_source(value: &Value<'_>) -> rquickjs::Result<WebSource> {
    if let Some(s) = value.as_string() {
        return Ok(WebSource::Uri(s.to_string()?.into()));
    }
    let Some(o) = value.as_object() else {
        return Ok(WebSource::None);
    };
    if let Some(html) = o.get::<_, Option<String>>("html")? {
        let base: Value = o.get("baseUrl")?;
        return Ok(WebSource::Html {
            html: html.into(),
            base_url: opt_string(&base)?,
        });
    }
    Ok(match o.get::<_, Option<String>>("uri")? {
        Some(uri) => WebSource::Uri(uri.into()),
        None => WebSource::None,
    })
}

/// `"https://…"` or a bundled asset `{ uri, width, height }`.
fn decode_source(value: &Value<'_>) -> rquickjs::Result<Option<ImageSource>> {
    if let Some(s) = value.as_string() {
        let uri = s.to_string()?;
        return Ok((!uri.is_empty()).then(|| ImageSource {
            uri: uri.into(),
            size: None,
        }));
    }
    let Some(obj) = value.as_object() else {
        return Ok(None);
    };
    let Some(uri) = obj.get::<_, Option<String>>("uri")? else {
        return Ok(None);
    };
    let size = match (
        obj.get::<_, Option<f64>>("width")?,
        obj.get::<_, Option<f64>>("height")?,
    ) {
        (Some(w), Some(h)) => Some((w as f32, h as f32)),
        _ => None,
    };
    Ok(Some(ImageSource {
        uri: uri.into(),
        size,
    }))
}

/// `{ id, title, message, buttons: [{ text, style }], cancelable, sheet }`.
fn decode_dialog(arg: &Value<'_>) -> rquickjs::Result<Dialog> {
    let Some(o) = arg.as_object() else {
        return Ok(Dialog {
            id: 0,
            title: None,
            message: None,
            buttons: Vec::new(),
            cancelable: true,
            sheet: false,
        });
    };
    let text = |key: &str| o.get::<_, Option<String>>(key).ok().flatten();
    let mut buttons = Vec::new();
    if let Some(items) = o.get::<_, Option<rquickjs::Array>>("buttons")? {
        for item in items.iter::<Object>() {
            let item = item?;
            buttons.push(DialogButton {
                text: item.get::<_, Option<String>>("text")?.unwrap_or_default(),
                style: match item.get::<_, Option<String>>("style")?.as_deref() {
                    Some("cancel") => DialogButtonStyle::Cancel,
                    Some("destructive") => DialogButtonStyle::Destructive,
                    _ => DialogButtonStyle::Default,
                },
            });
        }
    }
    Ok(Dialog {
        id: o.get::<_, Option<f64>>("id")?.unwrap_or(0.0) as u32,
        title: text("title"),
        message: text("message"),
        buttons,
        cancelable: o.get::<_, Option<bool>>("cancelable")?.unwrap_or(true),
        sheet: o.get::<_, Option<bool>>("sheet")?.unwrap_or(false),
    })
}

/// The properties of a style: an object, or (React Native style) an array
/// of styles, possibly nested, merged left to right; falsy entries are
/// skipped. A property set by a later style replaces it whole.
fn style_entries<'js>(
    value: &Value<'js>,
    out: &mut Vec<(String, Value<'js>)>,
) -> rquickjs::Result<()> {
    if let Some(items) = value.as_array() {
        for item in items.iter::<Value>() {
            style_entries(&item?, out)?;
        }
    } else if let Some(obj) = value.as_object() {
        for prop in obj.props::<String, Value>() {
            let (key, value) = prop?;
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some(entry) => entry.1 = value,
                None => out.push((key, value)),
            }
        }
    }
    Ok(())
}

fn decode_style<'js>(
    entries: Vec<(String, Value<'js>)>,
    warnings: &mut Warnings,
) -> rquickjs::Result<(Box<Style>, StylePatch)> {
    let mut style = Box::<Style>::default();
    let mut patch = StylePatch::default();
    for (key, value) in entries {
        if key == "transform" {
            decode_transform(&value, &mut style, &mut patch, warnings)?;
            continue;
        }
        if (key == "fontVariant" || key == "transformOrigin")
            && let Some(items) = value.as_array()
        {
            // Arrays of names / coordinates: the string form.
            let mut words = Vec::new();
            for item in items.iter::<Value>() {
                let item = item?;
                if let Some(n) = item.as_number() {
                    words.push(format!("{n}px"));
                } else if let Some(s) = item.as_string() {
                    words.push(s.to_string()?);
                }
            }
            let joined = words.join(" ");
            if let Err(e) = patch.set(&mut style, &key, StyleValue::Str(&joined)) {
                warnings.warn(format!("style `{key}`: {e:?}"));
            }
            continue;
        }
        if key == "textShadowOffset" {
            let offset = match value.as_object() {
                Some(o) => Some((
                    o.get::<_, Option<f64>>("width")?.unwrap_or(0.0) as f32,
                    o.get::<_, Option<f64>>("height")?.unwrap_or(0.0) as f32,
                )),
                None => None,
            };
            let _ = patch.push(&mut style, StyleDecl::TextShadowOffset(offset));
            continue;
        }
        if key == "shadowOffset" {
            // `{ width, height }`; null / undefined restores the default.
            let offset = match value.as_object() {
                Some(o) => Some((
                    o.get::<_, Option<f64>>("width")?.unwrap_or(0.0) as f32,
                    o.get::<_, Option<f64>>("height")?.unwrap_or(0.0) as f32,
                )),
                None if value.is_null() || value.is_undefined() => None,
                None => {
                    warnings.warn("style `shadowOffset` must be `{ width, height }`".to_owned());
                    continue;
                }
            };
            let _ = patch.push(&mut style, StyleDecl::ShadowOffset(offset));
            continue;
        }
        if let Some(node) = native_value(&value) {
            match AnimProp::from_name(&key) {
                Some(prop) => {
                    let _ = patch.push(&mut style, StyleDecl::Animated(prop, node));
                }
                None => warnings.warn(format!("style `{key}` cannot be animated natively")),
            }
            continue;
        }
        let string;
        let v = match value.type_of() {
            Type::Int | Type::Float => {
                StyleValue::Number(value.as_number().unwrap_or_default() as f32)
            }
            Type::String => {
                string = value.as_string().expect("string").to_string()?;
                StyleValue::Str(&string)
            }
            Type::Null | Type::Undefined => StyleValue::Null,
            _ => {
                warnings.warn(format!("style `{key}`: unsupported value type"));
                continue;
            }
        };
        match patch.set(&mut style, &key, v) {
            Ok(()) => {}
            Err(StyleError::UnknownProperty) => {
                warnings.warn(format!("unsupported style property `{key}`"))
            }
            Err(StyleError::InvalidValue) => {
                warnings.warn(format!("invalid value for style `{key}`: {v:?}"))
            }
        }
    }
    Ok((style, patch))
}

/// `transform: [{ translateX: 10 }, { rotate: "45deg" }, { scale: nativeValue }]`.
fn decode_transform(
    value: &Value<'_>,
    style: &mut Style,
    patch: &mut StylePatch,
    warnings: &mut Warnings,
) -> rquickjs::Result<()> {
    // A `transform` replaces the class transforms (and an earlier one).
    let _ = patch.push(style, StyleDecl::ClearTransform);
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    for item in items.iter::<Object>() {
        for entry in item?.props::<String, Value>() {
            let (key, value) = entry?;
            let Some(kind) = TransformKind::parse(&key) else {
                warnings.warn(format!("unsupported transform `{key}`"));
                continue;
            };
            // Animated entries keep their place in the list; the native value
            // replaces the placeholder amount.
            let (amount, node) = match native_value(&value) {
                Some(node) => (Some(kind.identity()), Some(node)),
                None => (
                    match value.as_string() {
                        Some(s) if kind.is_angle() => parse_angle(&s.to_string()?),
                        Some(s) => s.to_string()?.trim().parse().ok(),
                        None => number(&value),
                    },
                    None,
                ),
            };
            let Some(amount) = amount else {
                warnings.warn(format!("invalid transform `{key}`"));
                continue;
            };
            match patch.push(style, StyleDecl::Transform(kind, amount, node)) {
                Ok(()) => {}
                Err(_) => warnings.warn(format!(
                    "too many transforms (at most {})",
                    yf_core::MAX_TRANSFORMS
                )),
            }
        }
    }
    Ok(())
}

/// JS truthiness.
fn truthy(v: &Value<'_>) -> bool {
    if let Some(b) = v.as_bool() {
        return b;
    }
    if let Some(n) = v.as_number() {
        return n != 0.0 && !n.is_nan();
    }
    if let Some(s) = v.as_string() {
        return s.to_string().is_ok_and(|s| !s.is_empty());
    }
    !(v.is_null() || v.is_undefined())
}

/// `"45deg"` / `"0.5rad"` -> degrees.
fn parse_angle(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(deg) = s.strip_suffix("deg") {
        deg.trim().parse().ok()
    } else if let Some(rad) = s.strip_suffix("rad") {
        rad.trim().parse::<f32>().ok().map(f32::to_degrees)
    } else {
        s.parse().ok()
    }
}

fn decode_anim(r: &mut Reader<'_, '_>) -> rquickjs::Result<AnimOp> {
    let sub = r.int()?;
    let node = r.int()? as yf_anim::NodeId;
    Ok(match sub {
        ANIM_VALUE => AnimOp::CreateValue {
            node,
            value: r.number()?,
        },
        ANIM_INTERPOLATE => {
            let input = r.int()? as yf_anim::NodeId;
            let input_range = r.numbers()?;
            let outputs = r.value()?;
            let output = decode_output(r, &outputs)?;
            let extrapolate = |name: String| match name.as_str() {
                "extend" => Extrapolate::Extend,
                "identity" => Extrapolate::Identity,
                _ => Extrapolate::Clamp,
            };
            let extrapolate_left = extrapolate(r.string()?);
            let extrapolate_right = extrapolate(r.string()?);
            let easing = r.value()?;
            AnimOp::CreateInterpolate {
                node,
                input,
                input_range,
                output,
                extrapolate_left,
                extrapolate_right,
                easing: decode_easing(&easing, Easing::Linear)?,
            }
        }
        ANIM_DIFF_CLAMP => AnimOp::CreateDiffClamp {
            node,
            input: r.int()? as yf_anim::NodeId,
            min: r.number()?,
            max: r.number()?,
        },
        ANIM_START_COMPOSITE => {
            // `node` carries the animation id here.
            let key = r.int()? as u32;
            let spec = r.value()?;
            AnimOp::StartComposite {
                animation: node,
                key,
                spec: decode_composite(r, &spec, 0)?,
            }
        }
        ANIM_STOP_ANIMATION => AnimOp::StopAnimation { animation: node },
        ANIM_RESET_COMPOSITE => AnimOp::ResetComposite { key: node },
        ANIM_RELEASE_COMPOSITE => AnimOp::ReleaseComposite { key: node },
        ANIM_MATH => {
            let op = match r.string()?.as_str() {
                "add" => MathOp::Add,
                "sub" => MathOp::Sub,
                "mul" => MathOp::Mul,
                "mod" => MathOp::Mod,
                _ => MathOp::Div,
            };
            let a = r.operand()?;
            let b = r.operand()?;
            AnimOp::CreateMath { node, op, a, b }
        }
        ANIM_SET => AnimOp::Set {
            node,
            value: r.number()?,
        },
        ANIM_START => {
            let animation = r.int()? as yf_anim::AnimId;
            let config = r.value()?;
            let driver = decode_driver(r, &config)?;
            AnimOp::Start {
                node,
                animation,
                driver,
            }
        }
        ANIM_STOP => AnimOp::Stop { node },
        ANIM_RELEASE => AnimOp::Release { node },
        other => {
            return Err(Exception::throw_type(
                r.ctx,
                &format!("yf op buffer: unknown anim op {other}"),
            ));
        }
    })
}

fn decode_output(r: &Reader<'_, '_>, outputs: &Value<'_>) -> rquickjs::Result<Output> {
    let Some(items) = outputs.as_array() else {
        return Err(Exception::throw_type(
            r.ctx,
            "interpolate: outputRange must be an array",
        ));
    };
    let first_is_string = items
        .get::<Value>(0)
        .map(|v| v.is_string())
        .unwrap_or(false);
    let angles = first_is_string
        && items
            .get::<String>(0)
            .is_ok_and(|s| s.ends_with("deg") || s.ends_with("rad"));
    if angles {
        // `"45deg"` / `"0.5rad"`: rotations, in degrees.
        let mut degrees = Vec::with_capacity(items.len());
        for item in items.iter::<String>() {
            let item = item?;
            let value = match (item.strip_suffix("deg"), item.strip_suffix("rad")) {
                (Some(d), _) => d.trim().parse::<f32>().ok(),
                (_, Some(r)) => r.trim().parse::<f32>().ok().map(f32::to_degrees),
                _ => None,
            };
            degrees.push(value.ok_or_else(|| {
                Exception::throw_type(r.ctx, &format!("interpolate: invalid angle {item:?}"))
            })?);
        }
        return Ok(Output::Numbers(degrees));
    }
    if first_is_string {
        let mut colors = Vec::with_capacity(items.len());
        for item in items.iter::<String>() {
            let color = Color::parse(&item?).ok_or_else(|| {
                Exception::throw_type(r.ctx, "interpolate: invalid color in outputRange")
            })?;
            colors.push(color.0);
        }
        Ok(Output::Colors(colors))
    } else {
        Ok(Output::Numbers(
            items
                .iter::<f64>()
                .map(|v| v.map(|v| v as f32))
                .collect::<rquickjs::Result<_>>()?,
        ))
    }
}

fn decode_driver(r: &Reader<'_, '_>, config: &Value<'_>) -> rquickjs::Result<Driver> {
    let Some(c) = config.as_object() else {
        return Err(Exception::throw_type(
            r.ctx,
            "animation config must be an object",
        ));
    };
    let num = |key: &str| c.get::<_, Option<f64>>(key).map(|v| v.map(|v| v as f32));
    let kind = c.get::<_, String>("type")?;
    Ok(match kind.as_str() {
        "timing" => Driver::Timing {
            to: num("to")?.unwrap_or(0.0),
            duration_ms: num("duration")?.unwrap_or(300.0),
            easing: decode_easing(&c.get::<_, Value>("easing")?, Easing::EaseInOut)?,
            delay_ms: num("delay")?.unwrap_or(0.0),
        },
        "spring" => Driver::Spring {
            to: num("to")?.unwrap_or(0.0),
            stiffness: num("stiffness")?.unwrap_or(170.0),
            damping: num("damping")?.unwrap_or(26.0),
            mass: num("mass")?.unwrap_or(1.0),
            velocity: num("velocity")?,
        },
        "decay" => Driver::Decay {
            velocity: num("velocity")?,
            deceleration: num("deceleration")?.unwrap_or(0.998),
        },
        other => {
            return Err(Exception::throw_type(
                r.ctx,
                &format!("unknown animation type `{other}`"),
            ));
        }
    })
}

/// An easing: a name (`linear`, `easeIn`, ...), a Bézier `[x1, y1, x2, y2]`
/// or a curve `{ curve, param?, mode }` (the runtime's `Easing`).
fn decode_easing(value: &Value<'_>, default: Easing) -> rquickjs::Result<Easing> {
    if let Some(a) = value.as_array() {
        let p = |i| a.get::<f64>(i).map(|v| v as f32).unwrap_or(0.0);
        return Ok(Easing::Bezier(p(0), p(1), p(2), p(3)));
    }
    if let Some(o) = value.as_object() {
        let curve = o.get::<_, Option<String>>("curve")?.unwrap_or_default();
        let param = o.get::<_, Option<f64>>("param")?.map(|v| v as f32);
        let curve = match curve.as_str() {
            "quad" => Curve::Quad,
            "cubic" => Curve::Cubic,
            "poly" => Curve::Poly(param.unwrap_or(2.0)),
            "sin" => Curve::Sin,
            "circle" => Curve::Circle,
            "exp" => Curve::Exp,
            "elastic" => Curve::Elastic(param.unwrap_or(1.0)),
            "back" => Curve::Back(param.unwrap_or(1.70158)),
            "bounce" => Curve::Bounce,
            _ => return Ok(default),
        };
        let mode = match o.get::<_, Option<String>>("mode")?.as_deref() {
            Some("out") => EaseMode::Out,
            Some("inOut") => EaseMode::InOut,
            _ => EaseMode::In,
        };
        return Ok(Easing::Curve(curve, mode));
    }
    Ok(
        match value
            .as_string()
            .map(|s| s.to_string())
            .transpose()?
            .as_deref()
        {
            Some("linear") => Easing::Linear,
            Some("easeIn") => Easing::EaseIn,
            Some("easeOut") => Easing::EaseOut,
            Some("easeInOut") => Easing::EaseInOut,
            // CSS / React Native `ease`.
            Some("ease") => Easing::Bezier(0.25, 0.1, 0.25, 1.0),
            _ => default,
        },
    )
}

/// A composite animation: `{ type, ... }` with `node` on the runs.
fn decode_composite(
    r: &Reader<'_, '_>,
    spec: &Value<'_>,
    depth: u32,
) -> rquickjs::Result<yf_anim::Composite> {
    use yf_anim::Composite;
    let Some(o) = spec.as_object().filter(|_| depth < 64) else {
        return Err(Exception::throw_type(r.ctx, "animation: invalid composite"));
    };
    let children = |key: &str| -> rquickjs::Result<Vec<Composite>> {
        let list: rquickjs::Array = o.get(key)?;
        list.iter::<Value>()
            .map(|c| decode_composite(r, &c?, depth + 1))
            .collect()
    };
    let kind: String = o.get("type")?;
    Ok(match kind.as_str() {
        "sequence" => Composite::Sequence(children("children")?),
        "parallel" => Composite::Parallel {
            children: children("children")?,
            stop_together: o.get::<_, Option<bool>>("stopTogether")?.unwrap_or(true),
        },
        "delay" => Composite::Delay(o.get::<_, Option<f64>>("ms")?.unwrap_or(0.0) as f32),
        "loop" => Composite::Loop {
            child: Box::new(decode_composite(
                r,
                &o.get::<_, Value>("child")?,
                depth + 1,
            )?),
            iterations: o
                .get::<_, Option<f64>>("iterations")?
                .filter(|n| *n >= 0.0)
                .map(|n| n as u32),
            reset: o.get::<_, Option<bool>>("reset")?.unwrap_or(true),
        },
        _ => Composite::Run {
            node: o.get::<_, f64>("node")? as yf_anim::NodeId,
            driver: decode_driver(r, spec)?,
        },
    })
}
