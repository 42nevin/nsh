//! Decoding of canvas drawings (`OP_DRAW`): the operations a `draw`
//! callback recorded through the runtime's `CanvasRenderingContext2D`.
//!
//! The sub-opcodes must match `CANVAS_*` in `js/runtime/index.ts`.

use std::sync::Arc;

use rquickjs::{Array, Ctx, Exception, Value};
use yf_core::{
    Color,
    canvas::{
        CanvasFont, CanvasGradient, CanvasImage, CanvasOp, CanvasTextAlign, CanvasTextBaseline,
        Composite, Drawing, FillRule, GradientShape, LineCap, LineJoin, Num, Paint, PathOp,
        parse_svg_path,
    },
};

const SAVE: i32 = 0;
const RESTORE: i32 = 1;
const TRANSLATE: i32 = 2;
const SCALE: i32 = 3;
const ROTATE: i32 = 4;
const TRANSFORM: i32 = 5;
const SET_TRANSFORM: i32 = 6;
const BEGIN_PATH: i32 = 7;
const MOVE_TO: i32 = 8;
const LINE_TO: i32 = 9;
const QUAD_TO: i32 = 10;
const CUBIC_TO: i32 = 11;
const ARC: i32 = 12;
const ARC_TO: i32 = 13;
const ELLIPSE: i32 = 14;
const RECT: i32 = 15;
const ROUND_RECT: i32 = 16;
const CLOSE: i32 = 17;
const FILL: i32 = 18;
const STROKE: i32 = 19;
const CLIP: i32 = 20;
const FILL_RECT: i32 = 21;
const STROKE_RECT: i32 = 22;
const CLEAR_RECT: i32 = 23;
const FILL_STYLE: i32 = 24;
const STROKE_STYLE: i32 = 25;
const LINE_WIDTH: i32 = 26;
const LINE_CAP: i32 = 27;
const LINE_JOIN: i32 = 28;
const MITER_LIMIT: i32 = 29;
const LINE_DASH: i32 = 30;
const LINE_DASH_OFFSET: i32 = 31;
const GLOBAL_ALPHA: i32 = 32;
const COMPOSITE: i32 = 33;
const SHADOW_COLOR: i32 = 34;
const SHADOW_BLUR: i32 = 35;
const SHADOW_OFFSET: i32 = 36;
const FONT: i32 = 37;
const TEXT_ALIGN: i32 = 38;
const TEXT_BASELINE: i32 = 39;
const FILL_TEXT: i32 = 40;
const STROKE_TEXT: i32 = 41;
const DRAW_IMAGE: i32 = 42;
/// In `Path2D` lists: SVG path data.
const SVG_PATH: i32 = 50;

struct Items<'a, 'js> {
    ctx: &'a Ctx<'js>,
    buf: &'a Array<'js>,
    pos: usize,
    end: usize,
}

impl<'js> Items<'_, 'js> {
    fn more(&self) -> bool {
        self.pos < self.end
    }

    fn value(&mut self) -> rquickjs::Result<Value<'js>> {
        if self.pos >= self.end {
            return Err(Exception::throw_type(
                self.ctx,
                "yf canvas: truncated drawing",
            ));
        }
        let v = self.buf.get::<Value>(self.pos)?;
        self.pos += 1;
        Ok(v)
    }

    fn int(&mut self) -> rquickjs::Result<i32> {
        let v = self.value()?;
        v.as_int()
            .or_else(|| v.as_float().map(|f| f as i32))
            .ok_or_else(|| Exception::throw_type(self.ctx, "yf canvas: expected an operation"))
    }

    fn float(&mut self) -> rquickjs::Result<f32> {
        Ok(self.value()?.as_number().map_or(0.0, |n| n as f32))
    }

    fn bool(&mut self) -> rquickjs::Result<bool> {
        Ok(self.value()?.as_bool().unwrap_or(false))
    }

    fn string(&mut self) -> rquickjs::Result<String> {
        let v = self.value()?;
        match v.as_string() {
            Some(s) => s.to_string(),
            None => Ok(String::new()),
        }
    }

    /// A number or a native value (a slot of `drawing`).
    fn num(&mut self, drawing: &mut Drawing) -> rquickjs::Result<Num> {
        let v = self.value()?;
        if let Some(n) = v.as_number() {
            return Ok(Num::Const(n as f32));
        }
        match native_value(&v) {
            Some(node) => Ok(Num::Slot(drawing.slot(node))),
            None => Ok(Num::Const(0.0)),
        }
    }

    fn nums<const N: usize>(&mut self, drawing: &mut Drawing) -> rquickjs::Result<[Num; N]> {
        let mut out = [Num::default(); N];
        for n in &mut out {
            *n = self.num(drawing)?;
        }
        Ok(out)
    }
}

fn native_value(value: &Value<'_>) -> Option<yf_anim::NodeId> {
    value
        .as_object()?
        .get::<_, Option<u32>>("__nv")
        .ok()
        .flatten()
}

/// Decodes the `end - start` items of a drawing in `buf`.
pub(crate) fn decode<'js>(
    ctx: &Ctx<'js>,
    buf: &Array<'js>,
    start: usize,
    end: usize,
) -> rquickjs::Result<Drawing> {
    let mut drawing = Drawing::default();
    let mut r = Items {
        ctx,
        buf,
        pos: start,
        end,
    };
    while r.more() {
        let code = r.int()?;
        if let Some(path) = path_op(code, &mut r, &mut drawing)? {
            drawing.ops.push(CanvasOp::Path(path));
            continue;
        }
        let op = match code {
            SAVE => CanvasOp::Save,
            RESTORE => CanvasOp::Restore,
            TRANSLATE => {
                let [x, y] = r.nums(&mut drawing)?;
                CanvasOp::Translate(x, y)
            }
            SCALE => {
                let [x, y] = r.nums(&mut drawing)?;
                CanvasOp::Scale(x, y)
            }
            ROTATE => CanvasOp::Rotate(r.num(&mut drawing)?),
            TRANSFORM => CanvasOp::Transform(r.nums(&mut drawing)?),
            SET_TRANSFORM => CanvasOp::SetTransform(r.nums(&mut drawing)?),
            BEGIN_PATH => CanvasOp::BeginPath,
            FILL => {
                let path = path_value(ctx, r.value()?, &mut drawing)?;
                CanvasOp::Fill(path, fill_rule(&r.string()?))
            }
            STROKE => CanvasOp::Stroke(path_value(ctx, r.value()?, &mut drawing)?),
            CLIP => {
                let path = path_value(ctx, r.value()?, &mut drawing)?;
                CanvasOp::Clip(path, fill_rule(&r.string()?))
            }
            FILL_RECT => CanvasOp::FillRect(r.nums(&mut drawing)?),
            STROKE_RECT => CanvasOp::StrokeRect(r.nums(&mut drawing)?),
            CLEAR_RECT => CanvasOp::ClearRect(r.nums(&mut drawing)?),
            FILL_STYLE | STROKE_STYLE => {
                let Some(paint) = paint(r.value()?, &mut drawing)? else {
                    continue;
                };
                if code == FILL_STYLE {
                    CanvasOp::FillStyle(paint)
                } else {
                    CanvasOp::StrokeStyle(paint)
                }
            }
            LINE_WIDTH => CanvasOp::LineWidth(r.num(&mut drawing)?),
            LINE_CAP => CanvasOp::LineCap(match r.string()?.as_str() {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                _ => LineCap::Butt,
            }),
            LINE_JOIN => CanvasOp::LineJoin(match r.string()?.as_str() {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                _ => LineJoin::Miter,
            }),
            MITER_LIMIT => CanvasOp::MiterLimit(r.float()?.max(1.0)),
            LINE_DASH => {
                let v = r.value()?;
                let mut dash: Vec<f32> = match v.as_array() {
                    Some(a) => a
                        .iter::<f64>()
                        .filter_map(Result::ok)
                        .map(|d| d as f32)
                        .collect(),
                    None => Vec::new(),
                };
                // Canvas ignores lists with negative or non-finite entries.
                if dash.iter().any(|d| !d.is_finite() || *d < 0.0) {
                    continue;
                }
                if dash.len() % 2 == 1 {
                    dash.extend_from_within(..);
                }
                CanvasOp::LineDash(dash.into())
            }
            LINE_DASH_OFFSET => CanvasOp::LineDashOffset(r.num(&mut drawing)?),
            GLOBAL_ALPHA => CanvasOp::GlobalAlpha(r.num(&mut drawing)?),
            COMPOSITE => match Composite::parse(&r.string()?) {
                Some(c) => CanvasOp::Composite(c),
                None => continue,
            },
            SHADOW_COLOR => match Color::parse(&r.string()?) {
                Some(c) => CanvasOp::ShadowColor(c),
                None => continue,
            },
            SHADOW_BLUR => CanvasOp::ShadowBlur(r.float()?.max(0.0)),
            SHADOW_OFFSET => CanvasOp::ShadowOffset(r.float()?, r.float()?),
            FONT => match CanvasFont::parse(&r.string()?) {
                Some(f) => CanvasOp::Font(f),
                None => continue,
            },
            TEXT_ALIGN => CanvasOp::TextAlign(match r.string()?.as_str() {
                "end" => CanvasTextAlign::End,
                "left" => CanvasTextAlign::Left,
                "right" => CanvasTextAlign::Right,
                "center" => CanvasTextAlign::Center,
                _ => CanvasTextAlign::Start,
            }),
            TEXT_BASELINE => CanvasOp::TextBaseline(match r.string()?.as_str() {
                "top" => CanvasTextBaseline::Top,
                "hanging" => CanvasTextBaseline::Hanging,
                "middle" => CanvasTextBaseline::Middle,
                "ideographic" => CanvasTextBaseline::Ideographic,
                "bottom" => CanvasTextBaseline::Bottom,
                _ => CanvasTextBaseline::Alphabetic,
            }),
            FILL_TEXT | STROKE_TEXT => {
                let text: Arc<str> = r.string()?.into();
                let [x, y] = r.nums(&mut drawing)?;
                let max = r.value()?.as_number().map(|n| n as f32);
                if code == FILL_TEXT {
                    CanvasOp::FillText {
                        text,
                        x,
                        y,
                        max_width: max,
                    }
                } else {
                    CanvasOp::StrokeText {
                        text,
                        x,
                        y,
                        max_width: max,
                    }
                }
            }
            DRAW_IMAGE => {
                let uri: Arc<str> = r.string()?.into();
                let source = r.value()?;
                let source = source.as_array().map(|a| {
                    let mut out = [0f32; 4];
                    for (i, v) in a.iter::<f64>().take(4).enumerate() {
                        out[i] = v.map_or(0.0, |v| v as f32);
                    }
                    out
                });
                let dest = r.nums(&mut drawing)?;
                if uri.is_empty() {
                    continue;
                }
                CanvasOp::DrawImage {
                    image: CanvasImage { uri },
                    source,
                    dest,
                }
            }
            other => {
                return Err(Exception::throw_type(
                    ctx,
                    &format!("yf canvas: unknown operation {other}"),
                ));
            }
        };
        drawing.ops.push(op);
    }
    Ok(drawing)
}

/// A path-building operation, if `code` is one.
fn path_op(
    code: i32,
    r: &mut Items<'_, '_>,
    drawing: &mut Drawing,
) -> rquickjs::Result<Option<PathOp>> {
    Ok(Some(match code {
        MOVE_TO => {
            let [x, y] = r.nums(drawing)?;
            PathOp::MoveTo(x, y)
        }
        LINE_TO => {
            let [x, y] = r.nums(drawing)?;
            PathOp::LineTo(x, y)
        }
        QUAD_TO => PathOp::QuadTo(r.nums(drawing)?),
        CUBIC_TO => PathOp::CubicTo(r.nums(drawing)?),
        ARC => {
            let [x, y, radius, start, end] = r.nums(drawing)?;
            PathOp::Arc {
                x,
                y,
                radius,
                start,
                end,
                ccw: r.bool()?,
            }
        }
        ARC_TO => PathOp::ArcTo(r.nums(drawing)?),
        ELLIPSE => {
            let geometry = r.nums(drawing)?;
            let [start, end] = r.nums(drawing)?;
            PathOp::Ellipse {
                geometry,
                start,
                end,
                ccw: r.bool()?,
            }
        }
        RECT => PathOp::Rect(r.nums(drawing)?),
        ROUND_RECT => {
            let rect = r.nums(drawing)?;
            PathOp::RoundRect(rect, r.nums(drawing)?)
        }
        CLOSE => PathOp::Close,
        _ => return Ok(None),
    }))
}

/// A `Path2D` (its recorded list), or `None` for the current path.
fn path_value<'js>(
    ctx: &Ctx<'js>,
    value: Value<'js>,
    drawing: &mut Drawing,
) -> rquickjs::Result<Option<Arc<[PathOp]>>> {
    let Some(list) = value.as_array() else {
        return Ok(None);
    };
    let mut r = Items {
        ctx,
        buf: list,
        pos: 0,
        end: list.len(),
    };
    let mut ops = Vec::new();
    while r.more() {
        let code = r.int()?;
        if code == SVG_PATH {
            ops.extend(parse_svg_path(&r.string()?));
            continue;
        }
        match path_op(code, &mut r, drawing)? {
            Some(op) => ops.push(op),
            None => {
                return Err(Exception::throw_type(
                    ctx,
                    &format!("yf canvas: {code} is not a path operation"),
                ));
            }
        }
    }
    Ok(Some(ops.into()))
}

fn fill_rule(name: &str) -> FillRule {
    if name == "evenodd" {
        FillRule::EvenOdd
    } else {
        FillRule::NonZero
    }
}

/// A color string, an animated color or a gradient
/// (`[kind, ...geometry, [offset, color, ...]]`).
fn paint<'js>(value: Value<'js>, drawing: &mut Drawing) -> rquickjs::Result<Option<Paint>> {
    if let Some(s) = value.as_string() {
        return Ok(Color::parse(&s.to_string()?).map(Paint::Color));
    }
    if let Some(node) = native_value(&value) {
        return Ok(Some(Paint::Slot(drawing.slot(node))));
    }
    let Some(list) = value.as_array() else {
        return Ok(None);
    };
    let n = |i: usize| -> f32 { list.get::<f64>(i).map_or(0.0, |v| v as f32) };
    let (shape, stops_at) = match list.get::<i32>(0).unwrap_or(-1) {
        0 => (GradientShape::Linear([n(1), n(2), n(3), n(4)]), 5),
        1 => (
            GradientShape::Radial([n(1), n(2), n(3), n(4), n(5), n(6)]),
            7,
        ),
        2 => (GradientShape::Conic([n(1), n(2), n(3)]), 4),
        _ => return Ok(None),
    };
    let mut stops = Vec::new();
    if let Ok(raw) = list.get::<Array>(stops_at) {
        let mut i = 0;
        while i + 1 < raw.len() {
            let offset = raw.get::<f64>(i).unwrap_or(0.0) as f32;
            let color = raw.get::<String>(i + 1).ok().and_then(|c| Color::parse(&c));
            if let Some(color) = color {
                stops.push((offset.clamp(0.0, 1.0), color));
            }
            i += 2;
        }
    }
    // Stops apply in offset order (equal offsets keep their order).
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(Some(Paint::Gradient(Arc::new(CanvasGradient {
        shape,
        stops,
    }))))
}
