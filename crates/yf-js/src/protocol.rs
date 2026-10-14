//! Decoder for the flat op buffer filled by `js/runtime/index.ts`.
//!
//! The buffer is a plain JS array of `[opcode, args...]` records. Keeping
//! the JS side to `array.push` of raw values is deliberate: QuickJS is the
//! slowest stage, so all parsing (tags, style objects, event names) happens
//! here in Rust.

use rquickjs::{Array, Ctx, Exception, Object, Type, Value};
use rustc_hash::FxHashSet;
use yf_anim::{AnimOp, AnimProp, Driver, Easing, Extrapolate, MathOp, Operand, Output};
use yf_core::{
    Color, EventKind, ImageSource, NodeId, Op, Prop, ResizeMode, Style, StyleError, StyleValue, Tag,
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

// Sub-opcodes of `OP_ANIM`.
const ANIM_VALUE: i32 = 0;
const ANIM_INTERPOLATE: i32 = 1;
const ANIM_MATH: i32 = 2;
const ANIM_SET: i32 = 3;
const ANIM_START: i32 = 4;
const ANIM_STOP: i32 = 5;
const ANIM_RELEASE: i32 = 6;

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

fn decode_prop<'js>(
    id: NodeId,
    name: &str,
    value: Value<'js>,
    warnings: &mut Warnings,
) -> rquickjs::Result<Option<Op>> {
    let prop = match name {
        "style" => {
            let style = match value.as_object() {
                Some(obj) => decode_style(obj, warnings)?,
                None => Box::default(),
            };
            return Ok(Some(Op::SetStyle { id, style }));
        }
        "src" => Prop::Source(decode_source(&value)?),
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
        "slot" => Prop::Slot(number(&value).unwrap_or(0.0) as u32),
        "cellIndex" => Prop::CellIndex(number(&value).unwrap_or(0.0) as u32),
        "scrollValue" => Prop::ScrollValue(native_value(&value)),
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

fn decode_style<'js>(obj: &Object<'js>, warnings: &mut Warnings) -> rquickjs::Result<Box<Style>> {
    let mut style = Box::<Style>::default();
    for prop in obj.props::<String, Value>() {
        let (key, value) = prop?;
        if key == "transform" {
            decode_transform(&value, &mut style, warnings)?;
            continue;
        }
        if let Some(node) = native_value(&value) {
            match AnimProp::from_name(&key) {
                Some(prop) => style.animated.push((prop, node)),
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
        match style.set(&key, v) {
            Ok(()) => {}
            Err(StyleError::UnknownProperty) => {
                warnings.warn(format!("unsupported style property `{key}`"))
            }
            Err(StyleError::InvalidValue) => {
                warnings.warn(format!("invalid value for style `{key}`: {v:?}"))
            }
        }
    }
    Ok(style)
}

/// `transform: [{ translateX: 10 }, { rotate: "45deg" }, { scale: nativeValue }]`.
fn decode_transform(
    value: &Value<'_>,
    style: &mut Style,
    warnings: &mut Warnings,
) -> rquickjs::Result<()> {
    let Some(items) = value.as_array() else {
        return Ok(());
    };
    for item in items.iter::<Object>() {
        for entry in item?.props::<String, Value>() {
            let (key, value) = entry?;
            if let Some(node) = native_value(&value) {
                match AnimProp::from_name(&key) {
                    Some(prop) => style.animated.push((prop, node)),
                    None => warnings.warn(format!("transform `{key}` cannot be animated natively")),
                }
                continue;
            }
            let amount = match value.as_string() {
                Some(s) => parse_angle(&s.to_string()?),
                None => number(&value),
            };
            match amount.map(|v| style.set_transform(&key, v)) {
                Some(Ok(())) => {}
                _ => warnings.warn(format!("invalid transform `{key}`")),
            }
        }
    }
    Ok(())
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
            let extrapolate = match r.string()?.as_str() {
                "extend" => Extrapolate::Extend,
                "identity" => Extrapolate::Identity,
                _ => Extrapolate::Clamp,
            };
            AnimOp::CreateInterpolate {
                node,
                input,
                input_range,
                output,
                extrapolate,
            }
        }
        ANIM_MATH => {
            let op = match r.string()?.as_str() {
                "add" => MathOp::Add,
                "sub" => MathOp::Sub,
                "mul" => MathOp::Mul,
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
            easing: match c.get::<_, Value>("easing")? {
                v if v.is_array() => {
                    let a = v.as_array().expect("array");
                    let p = |i| a.get::<f64>(i).map(|v| v as f32).unwrap_or(0.0);
                    Easing::Bezier(p(0), p(1), p(2), p(3))
                }
                v => match v.as_string().map(|s| s.to_string()).transpose()?.as_deref() {
                    Some("linear") => Easing::Linear,
                    Some("easeIn") => Easing::EaseIn,
                    Some("easeOut") => Easing::EaseOut,
                    _ => Easing::EaseInOut,
                },
            },
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
