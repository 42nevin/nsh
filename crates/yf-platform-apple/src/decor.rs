//! Backgrounds, borders, rounded corners, clipping and shadows, all drawn by
//! Core Animation (no `drawRect`, no offscreen bitmaps where avoidable).
//!
//! The cheap path is the view's own layer: `cornerRadius` + `maskedCorners`
//! covers every shape whose rounded corners share one radius. Only corners
//! with different radii need shape layers, and only a view that both clips
//! its children and casts a shadow needs an extra content view (clipping a
//! layer also clips its shadow).

use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_core_foundation::{CFRetained, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{CGColor, CGImage, CGMutablePath};
use objc2_foundation::{NSArray, NSNumber};
use objc2_quartz_core::{
    CACornerMask, CALayer, CAShapeLayer, CATransaction, kCAFillRuleEvenOdd, kCAGravityCenter,
    kCAGravityResize, kCAGravityResizeAspect, kCAGravityResizeAspectFill, kCALineCapRound,
};
use objc2_ui_kit::{UIColor, UIImage, UIView};
use yf_core::{BorderStyle, Borders, Color, CornerRadii, ResizeMode, ViewStyle};

/// Fills sit behind the children. Borders too (CSS), except where the
/// children are clipped: there the layer's own border, which Core Animation
/// always draws in front, looks the same and needs no shape.
const BEHIND: CGFloat = -1024.0;
const BORDER_BEHIND: CGFloat = BEHIND + 1.0;
const IN_FRONT: CGFloat = 1024.0;

/// What a native view is, as far as decoration goes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Has children; can host a content view.
    View,
    /// Draws its content in the layer itself (below any sublayer).
    Text,
    /// Shows an image in an [`Decor::image`] sublayer.
    Image,
    /// Always clips; sublayers would scroll with the content.
    Scroll,
}

/// Extra layers of one view, created on demand.
#[derive(Default)]
pub(crate) struct Decor {
    /// Background for corners of different radii.
    fill: Option<Retained<CAShapeLayer>>,
    /// Border for corners of different radii.
    stroke: Option<Retained<CAShapeLayer>>,
    /// Clip shape for corners of different radii.
    mask: Option<Retained<CAShapeLayer>>,
    /// Holds the children while the view both clips them and casts a shadow.
    content: Option<Retained<UIView>>,
    /// Image content (image views only).
    image: Option<Retained<CALayer>>,
    /// The image, kept to switch between plain and tinted display.
    contents: Option<(Retained<CGImage>, CGFloat)>,
    /// `tintColor`: the image layer shows this color through a mask of the
    /// image.
    tint: Option<(Color, Retained<CALayer>)>,
    /// Borders whose sides differ: one even-odd ring per color.
    ring: Vec<Retained<CAShapeLayer>>,
    /// The shapes depend on the view's size: update them on resize.
    sized: bool,
}

impl Decor {
    /// Decoration of an image view: the image lives in its own clipped
    /// sublayer, so the view's layer can cast an unclipped shadow.
    pub(crate) fn for_image() -> Self {
        let image = CALayer::new();
        image.setMasksToBounds(true);
        Self {
            image: Some(image),
            sized: true,
            ..Self::default()
        }
    }

    /// The view children are added to.
    pub(crate) fn container<'a>(&'a self, view: &'a UIView) -> &'a UIView {
        self.content.as_deref().unwrap_or(view)
    }

    /// True if [`Decor::apply`] must run again when the view is resized.
    #[inline]
    pub(crate) fn sized(&self) -> bool {
        self.sized
    }

    /// Shows `image` (or nothing) in an image view.
    pub(crate) fn set_image(&mut self, image: Option<&UIImage>) {
        // SAFETY: plain getters on a valid image.
        self.contents = image.and_then(|i| unsafe { i.CGImage().map(|cg| (cg, i.scale())) });
        quiet(|| self.show_contents());
    }

    /// Puts the image into the image layer, or into the tint mask.
    fn show_contents(&self) {
        let Some(layer) = &self.image else { return };
        let target = match &self.tint {
            Some((_, tint)) => {
                // SAFETY: clearing the contents.
                unsafe { layer.setContents(None) };
                match tint.mask() {
                    Some(mask) => mask,
                    None => return,
                }
            }
            None => layer.clone(),
        };
        if let Some((_, scale)) = &self.contents {
            target.setContentsScale(*scale);
        }
        // SAFETY: a CGImage is a valid `contents` object.
        unsafe {
            target.setContents(
                self.contents
                    .as_ref()
                    .map(|(c, _)| &*(&**c as *const CGImage as *const AnyObject)),
            )
        };
    }

    /// How an image view fits its image.
    pub(crate) fn set_resize_mode(&self, mode: ResizeMode) {
        let Some(layer) = &self.image else { return };
        // SAFETY: immutable Core Animation constants.
        let gravity = unsafe {
            match mode {
                ResizeMode::Cover => kCAGravityResizeAspectFill,
                ResizeMode::Contain => kCAGravityResizeAspect,
                ResizeMode::Stretch => kCAGravityResize,
                ResizeMode::Center => kCAGravityCenter,
            }
        };
        quiet(|| {
            layer.setContentsGravity(gravity);
            if let Some((_, tint)) = &self.tint
                && let Some(mask) = tint.mask()
            {
                mask.setContentsGravity(gravity);
            }
        });
    }

    /// Shows the image in a single color (`tintColor`), or as is.
    fn set_tint(&mut self, tint: Option<Color>, local: CGRect) {
        let Some(layer) = self.image.clone() else {
            return;
        };
        match (tint, self.tint.clone()) {
            (Some(color), Some((current, t))) => {
                if color != current {
                    t.setBackgroundColor(cg_color(color).as_deref());
                    self.tint = Some((color, t));
                }
            }
            (Some(color), None) => {
                let t = CALayer::new();
                t.setBackgroundColor(cg_color(color).as_deref());
                let mask = CALayer::new();
                mask.setContentsGravity(&layer.contentsGravity());
                // SAFETY: a fresh mask layer owned by the tint layer.
                unsafe { t.setMask(Some(&mask)) };
                layer.addSublayer(&t);
                self.tint = Some((color, t));
                self.show_contents();
            }
            (None, Some((_, t))) => {
                t.removeFromSuperlayer();
                self.tint = None;
                self.show_contents();
            }
            (None, None) => {}
        }
        if let Some((_, t)) = &self.tint {
            t.setFrame(local);
            if let Some(mask) = t.mask() {
                mask.setFrame(local);
            }
        }
    }

    /// Sets the background color: the fill layer's if there is one.
    ///
    /// Always on layers: UIKit paints `UIView.backgroundColor` into the
    /// contents of views with `drawRect:` (text), where corners cannot round it.
    pub(crate) fn set_background(&self, view: &UIView, color: Color) {
        match &self.fill {
            Some(fill) => quiet(|| fill.setFillColor(cg_color(color).as_deref())),
            None => view.layer().setBackgroundColor(cg_color(color).as_deref()),
        }
    }

    /// Shapes `view` after `style` at its current size. `background` is the
    /// current (possibly animated) background; `animated_background` tells
    /// whether it may change without a new style.
    pub(crate) fn apply(
        &mut self,
        view: &UIView,
        kind: Kind,
        style: &ViewStyle,
        background: Color,
        animated_background: bool,
        mtm: MainThreadMarker,
    ) {
        quiet(|| self.apply_quietly(view, kind, style, background, animated_background, mtm));
    }

    fn apply_quietly(
        &mut self,
        view: &UIView,
        kind: Kind,
        style: &ViewStyle,
        background: Color,
        animated_background: bool,
        mtm: MainThreadMarker,
    ) {
        let layer = view.layer();
        let bounds = view.bounds();
        let (w, h) = (bounds.size.width as f32, bounds.size.height as f32);
        let local = CGRect::new(CGPoint::ZERO, bounds.size);
        // Scaling all radii by one factor keeps shared radii shared, so the
        // layer's own (self-clamping) radius needs no size: views without
        // paths are not reshaped on resize.
        let raw = style.radii();
        let radii = raw.fit(w, h);
        let shadow = style.shadow.is_visible();
        let borders = &style.borders;
        // Sides that differ are drawn as colored rings; equal sides by the
        // layer's border or a stroke.
        let uniform = borders.uniform().filter(|_| borders.is_visible());
        let ring = uniform.is_none() && borders.is_visible();
        let border = uniform.is_some();
        let dashed = borders.style != BorderStyle::Solid;

        // Corners sharing one radius are drawn by the layer itself. Scroll
        // views always do so (their sublayers scroll): the largest radius
        // stands in for the others.
        let shared = raw.shared().or_else(|| {
            (kind == Kind::Scroll).then(|| raw.to_array().into_iter().fold(0.0, f32::max))
        });
        let corners = corner_mask(&raw);
        let content = kind == Kind::View && style.overflow_hidden && shadow;

        // Text draws into the layer's own contents, below every sublayer: it
        // cannot have a fill layer, so its odd corners clip it instead.
        let fill = shared.is_none() && kind != Kind::Text;
        let border_behind = kind == Kind::View && !style.overflow_hidden;
        let stroke = border && (shared.is_none() || border_behind || dashed);
        let mask = shared.is_none() && (style.overflow_hidden || kind == Kind::Text) && !content;

        self.sized = kind == Kind::Image || shadow || shared.is_none() || content || stroke || ring;

        // Corners of the layer itself.
        let r = shared.unwrap_or(0.0) as CGFloat;
        layer.setCornerRadius(r);
        layer.setMaskedCorners(corners);

        // Background.
        match (fill, &self.fill) {
            (true, None) => {
                let f = CAShapeLayer::new();
                f.setZPosition(BEHIND);
                layer.insertSublayer_atIndex(&f, 0);
                self.fill = Some(f);
                layer.setBackgroundColor(None);
            }
            (false, Some(f)) => {
                f.removeFromSuperlayer();
                self.fill = None;
            }
            _ => {}
        }
        if let Some(f) = &self.fill {
            f.setFrame(local);
            f.setPath(Some(&rounded(local, radii)));
        }
        self.set_background(view, background);

        // Border: a stroke behind the children or for odd corners, else the
        // layer's own (on the content view when there is one, so the
        // children do not cover it).
        match (stroke, &self.stroke) {
            (true, None) => {
                let s = CAShapeLayer::new();
                s.setFillColor(None);
                layer.addSublayer(&s);
                self.stroke = Some(s);
            }
            (false, Some(s)) => {
                s.removeFromSuperlayer();
                self.stroke = None;
            }
            _ => {}
        }
        let (bw, bc) = uniform.unwrap_or((0.0, Color::TRANSPARENT));
        if let Some(s) = &self.stroke {
            s.setZPosition(if border_behind {
                BORDER_BEHIND
            } else {
                IN_FRONT
            });
            let half = (bw / 2.0) as CGFloat;
            s.setFrame(local);
            s.setPath(Some(&rounded(inset(local, half), radii.inset(bw / 2.0))));
            s.setLineWidth(bw as CGFloat);
            s.setStrokeColor(cg_color(bc).as_deref());
            let w = bw as f64;
            let pattern: Option<Retained<NSArray<NSNumber>>> = match borders.style {
                BorderStyle::Solid => None,
                BorderStyle::Dashed => Some(NSArray::from_retained_slice(&[
                    NSNumber::new_f64(w * 3.0),
                    NSNumber::new_f64(w * 3.0),
                ])),
                // Zero-length dashes with round caps draw dots.
                BorderStyle::Dotted => Some(NSArray::from_retained_slice(&[
                    NSNumber::new_f64(0.0),
                    NSNumber::new_f64(w * 2.0),
                ])),
            };
            s.setLineDashPattern(pattern.as_deref());
            // SAFETY: immutable Core Animation constants.
            s.setLineCap(unsafe {
                if borders.style == BorderStyle::Dotted {
                    kCALineCapRound
                } else {
                    objc2_quartz_core::kCALineCapButt
                }
            });
        }
        self.set_ring(
            layer.as_ref(),
            ring.then_some(borders),
            local,
            radii,
            border_behind,
        );
        let ca_border = border && !stroke;
        let (width, color) = if ca_border {
            (bw as CGFloat, cg_color(bc))
        } else {
            (0.0, None)
        };

        // Children in a content view: it clips, the view casts the shadow.
        self.set_content(view, content, mtm);
        let border_layer = match &self.content {
            Some(c) => {
                layer.setBorderWidth(0.0);
                c.setFrame(local);
                let cl = c.layer();
                cl.setCornerRadius(r);
                cl.setMaskedCorners(corners);
                match shared {
                    Some(_) => {
                        cl.setMasksToBounds(true);
                        // SAFETY: clearing the mask.
                        unsafe { cl.setMask(None) };
                    }
                    None => {
                        cl.setMasksToBounds(false);
                        let m = shape_mask(&mut self.mask, local, radii);
                        // SAFETY: a mask layer owned by this decoration.
                        unsafe { cl.setMask(Some(&m)) };
                    }
                }
                cl
            }
            None => layer.clone(),
        };
        border_layer.setBorderWidth(width);
        border_layer.setBorderColor(color.as_deref());

        // Clipping of the view's own layer.
        if self.content.is_none() {
            let clips = kind == Kind::Scroll || (style.overflow_hidden && shared.is_some());
            layer.setMasksToBounds(clips);
            if mask {
                let m = shape_mask(&mut self.mask, local, radii);
                // SAFETY: a mask layer owned by this decoration.
                unsafe { layer.setMask(Some(&m)) };
            } else {
                // SAFETY: clearing the mask.
                unsafe { layer.setMask(None) };
                self.mask = None;
            }
        } else {
            layer.setMasksToBounds(false);
            // SAFETY: clearing the mask (the content view clips).
            unsafe { layer.setMask(None) };
        }

        // Image content follows the view's shape.
        self.set_tint(style.tint_color.filter(|_| kind == Kind::Image), local);
        if let Some(image) = &self.image {
            image.setFrame(local);
            image.setCornerRadius(r);
            image.setMaskedCorners(corners);
            if shared.is_some() {
                // SAFETY: clearing the mask.
                unsafe { image.setMask(None) };
            } else {
                let m = CAShapeLayer::new();
                m.setPath(Some(&rounded(local, radii)));
                // SAFETY: a fresh mask layer, owned by the image layer.
                unsafe { image.setMask(Some(&m)) };
            }
            if image.superlayer().is_none() {
                layer.addSublayer(image);
            }
        }

        // Shadow. With a background the shape is known and passed as a path,
        // which spares Core Animation an offscreen pass over the content.
        if shadow {
            let s = style.shadow;
            layer.setShadowColor(cg_color(s.color).as_deref());
            layer.setShadowOpacity(s.opacity);
            layer.setShadowOffset(CGSize::new(s.offset_x as CGFloat, s.offset_y as CGFloat));
            layer.setShadowRadius(s.radius as CGFloat);
            let solid = !style.background_color.is_transparent() || animated_background;
            let path = solid.then(|| rounded(local, radii));
            layer.setShadowPath(path.as_deref().map(|p| &**p));
        } else {
            layer.setShadowOpacity(0.0);
            layer.setShadowPath(None);
        }
    }

    /// Draws borders with differing sides: the ring between the outer and
    /// the inner edge, one layer per side color, each masked to its side.
    fn set_ring(
        &mut self,
        layer: &CALayer,
        borders: Option<&Borders>,
        local: CGRect,
        radii: CornerRadii,
        behind: bool,
    ) {
        let Some(b) = borders else {
            for r in self.ring.drain(..) {
                r.removeFromSuperlayer();
            }
            return;
        };
        let [top, right, bottom, left] = b.widths.map(|w| w as CGFloat);
        let ring = ring_path(local, radii, [top, right, bottom, left]);
        let same_color = b.colors.iter().all(|&c| c == b.colors[0]);
        let count = if same_color { 1 } else { 4 };
        while self.ring.len() > count {
            if let Some(r) = self.ring.pop() {
                r.removeFromSuperlayer();
            }
        }
        while self.ring.len() < count {
            let r = CAShapeLayer::new();
            // SAFETY: an immutable Core Animation constant.
            r.setFillRule(unsafe { kCAFillRuleEvenOdd });
            layer.addSublayer(&r);
            self.ring.push(r);
        }
        let (w, h) = (local.size.width, local.size.height);
        for (i, r) in self.ring.iter().enumerate() {
            r.setZPosition(if behind { BORDER_BEHIND } else { IN_FRONT });
            r.setFrame(local);
            r.setPath(Some(&ring));
            r.setFillColor(cg_color(b.colors[i]).as_deref());
            if count == 1 {
                // SAFETY: clearing the mask.
                unsafe { r.setMask(None) };
                continue;
            }
            // The side's trapezoid, from the outer to the inner corners.
            let points = match i {
                0 => [(0.0, 0.0), (w, 0.0), (w - right, top), (left, top)],
                1 => [(w, 0.0), (w, h), (w - right, h - bottom), (w - right, top)],
                2 => [
                    (w, h),
                    (0.0, h),
                    (left, h - bottom),
                    (w - right, h - bottom),
                ],
                _ => [(0.0, h), (0.0, 0.0), (left, top), (left, h - bottom)],
            };
            let path = CGMutablePath::new();
            let p = Some(&*path);
            let m = std::ptr::null();
            // SAFETY: a valid mutable path and a null transform.
            unsafe {
                CGMutablePath::move_to_point(p, m, points[0].0, points[0].1);
                for &(x, y) in &points[1..] {
                    CGMutablePath::add_line_to_point(p, m, x, y);
                }
                CGMutablePath::close_subpath(p);
            }
            let mask = CAShapeLayer::new();
            mask.setFrame(local);
            mask.setPath(Some(&path));
            // SAFETY: a fresh mask layer owned by the ring layer.
            unsafe { r.setMask(Some(&mask)) };
        }
    }

    /// Moves the children into (or back out of) the content view.
    fn set_content(&mut self, view: &UIView, wanted: bool, mtm: MainThreadMarker) {
        match (wanted, &self.content) {
            (true, None) => {
                let content = UIView::new(mtm);
                let children = view.subviews();
                view.addSubview(&content);
                for child in children.iter() {
                    content.addSubview(&child);
                }
                self.content = Some(content);
            }
            (false, Some(content)) => {
                for child in content.subviews().iter() {
                    view.insertSubview_belowSubview(&child, content);
                }
                content.removeFromSuperview();
                self.content = None;
            }
            _ => {}
        }
    }
}

/// Sets a standalone layer's frame without an implicit animation.
pub(crate) fn quiet_frame(layer: &CALayer, frame: CGRect) {
    quiet(|| layer.setFrame(frame));
}

/// Runs `f` without Core Animation's implicit animations (standalone
/// layers animate every property change for 0.25s by default).
fn quiet<R>(f: impl FnOnce() -> R) -> R {
    CATransaction::begin();
    CATransaction::setDisableActions(true);
    let result = f();
    CATransaction::commit();
    result
}

fn shape_mask(
    slot: &mut Option<Retained<CAShapeLayer>>,
    rect: CGRect,
    radii: CornerRadii,
) -> Retained<CAShapeLayer> {
    let mask = slot.get_or_insert_with(CAShapeLayer::new).clone();
    mask.setFrame(rect);
    mask.setPath(Some(&rounded(rect, radii)));
    mask
}

/// The corners a shared radius applies to (UIKit layers are y-down: min Y is
/// the top).
fn corner_mask(radii: &CornerRadii) -> CACornerMask {
    let all = [
        (radii.top_left, CACornerMask::LayerMinXMinYCorner),
        (radii.top_right, CACornerMask::LayerMaxXMinYCorner),
        (radii.bottom_right, CACornerMask::LayerMaxXMaxYCorner),
        (radii.bottom_left, CACornerMask::LayerMinXMaxYCorner),
    ];
    let rounded = all
        .iter()
        .filter(|(r, _)| *r > 0.0)
        .fold(CACornerMask::empty(), |m, &(_, c)| m | c);
    if rounded.is_empty() {
        CACornerMask::all()
    } else {
        rounded
    }
}

fn inset(rect: CGRect, d: CGFloat) -> CGRect {
    CGRect::new(
        CGPoint::new(rect.origin.x + d, rect.origin.y + d),
        CGSize::new(
            (rect.size.width - 2.0 * d).max(0.0),
            (rect.size.height - 2.0 * d).max(0.0),
        ),
    )
}

/// The area between the outer edge (corners `radii`) and the inner edge of
/// borders `[top, right, bottom, left]` wide, for an even-odd fill. Inner
/// corners are elliptical where the adjacent sides differ.
fn ring_path(rect: CGRect, radii: CornerRadii, widths: [CGFloat; 4]) -> CFRetained<CGMutablePath> {
    let path = rounded(rect, radii);
    let [top, right, bottom, left] = widths;
    let (x0, y0) = (rect.origin.x + left, rect.origin.y + top);
    let (x1, y1) = (
        rect.origin.x + rect.size.width - right,
        rect.origin.y + rect.size.height - bottom,
    );
    if x1 <= x0 || y1 <= y0 {
        return path;
    }
    let [tl, tr, br, bl] = radii.to_array().map(|r| r as CGFloat);
    // (horizontal, vertical) radii of each inner corner.
    let corner = |r: CGFloat, h: CGFloat, v: CGFloat| ((r - h).max(0.0), (r - v).max(0.0));
    let (tlx, tly) = corner(tl, left, top);
    let (trx, try_) = corner(tr, right, top);
    let (brx, bry) = corner(br, right, bottom);
    let (blx, bly) = corner(bl, left, bottom);
    // Cubic approximation of a quarter ellipse.
    const K: CGFloat = 0.552_284_75;
    let p = Some(&*path);
    let m = std::ptr::null();
    // SAFETY: a valid mutable path and a null transform.
    unsafe {
        CGMutablePath::move_to_point(p, m, x0 + tlx, y0);
        CGMutablePath::add_line_to_point(p, m, x1 - trx, y0);
        CGMutablePath::add_curve_to_point(
            p,
            m,
            x1 - trx + trx * K,
            y0,
            x1,
            y0 + try_ - try_ * K,
            x1,
            y0 + try_,
        );
        CGMutablePath::add_line_to_point(p, m, x1, y1 - bry);
        CGMutablePath::add_curve_to_point(
            p,
            m,
            x1,
            y1 - bry + bry * K,
            x1 - brx + brx * K,
            y1,
            x1 - brx,
            y1,
        );
        CGMutablePath::add_line_to_point(p, m, x0 + blx, y1);
        CGMutablePath::add_curve_to_point(
            p,
            m,
            x0 + blx - blx * K,
            y1,
            x0,
            y1 - bly + bly * K,
            x0,
            y1 - bly,
        );
        CGMutablePath::add_line_to_point(p, m, x0, y0 + tly);
        CGMutablePath::add_curve_to_point(
            p,
            m,
            x0,
            y0 + tly - tly * K,
            x0 + tlx - tlx * K,
            y0,
            x0 + tlx,
            y0,
        );
        CGMutablePath::close_subpath(p);
    }
    path
}

/// A rectangle with circular corners of the given radii.
fn rounded(rect: CGRect, radii: CornerRadii) -> CFRetained<CGMutablePath> {
    let (x0, y0) = (rect.origin.x, rect.origin.y);
    let (x1, y1) = (x0 + rect.size.width, y0 + rect.size.height);
    let [tl, tr, br, bl] = radii.to_array().map(|r| r as CGFloat);
    let path = CGMutablePath::new();
    let p = Some(&*path);
    let m = std::ptr::null();
    // SAFETY: a valid mutable path and a null transform.
    unsafe {
        CGMutablePath::move_to_point(p, m, x0 + tl, y0);
        CGMutablePath::add_arc_to_point(p, m, x1, y0, x1, y0 + tr, tr);
        CGMutablePath::add_arc_to_point(p, m, x1, y1, x1 - br, y1, br);
        CGMutablePath::add_arc_to_point(p, m, x0, y1, x0, y1 - bl, bl);
        CGMutablePath::add_arc_to_point(p, m, x0, y0, x0 + tl, y0, tl);
        CGMutablePath::close_subpath(p);
    }
    path
}

fn ui_color(color: Color) -> Option<Retained<UIColor>> {
    if color.is_transparent() {
        return None;
    }
    let (r, g, b, a) = color.to_f64();
    Some(UIColor::colorWithRed_green_blue_alpha(r, g, b, a))
}

fn cg_color(color: Color) -> Option<Retained<CGColor>> {
    // SAFETY: the CGColor is retained for as long as the caller keeps it.
    ui_color(color).map(|c| unsafe { c.CGColor() })
}
