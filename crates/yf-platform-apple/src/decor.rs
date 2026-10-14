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
use objc2_core_graphics::{CGColor, CGMutablePath};
use objc2_quartz_core::{
    CACornerMask, CALayer, CAShapeLayer, CATransaction, kCAGravityCenter, kCAGravityResize,
    kCAGravityResizeAspect, kCAGravityResizeAspectFill,
};
use objc2_ui_kit::{UIColor, UIImage, UIView};
use yf_core::{Color, CornerRadii, ResizeMode, ViewStyle};

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
    pub(crate) fn set_image(&self, image: Option<&UIImage>) {
        let Some(layer) = &self.image else { return };
        quiet(|| {
            // SAFETY: plain getters on a valid image.
            let cg = image.and_then(|i| unsafe { i.CGImage() });
            if let Some(image) = image {
                // SAFETY: plain getter.
                layer.setContentsScale(unsafe { image.scale() });
            }
            // SAFETY: a CGImage is a valid `contents` object.
            unsafe {
                layer.setContents(cg.as_deref().map(|c| &*(c as *const _ as *const AnyObject)))
            };
        });
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
        quiet(|| layer.setContentsGravity(gravity));
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
        let border = style.border_width > 0.0 && !style.border_color.is_transparent();

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
        let stroke = border && (shared.is_none() || border_behind);
        let mask = shared.is_none() && (style.overflow_hidden || kind == Kind::Text) && !content;

        self.sized = kind == Kind::Image || shadow || shared.is_none() || content || stroke;

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
        if let Some(s) = &self.stroke {
            s.setZPosition(if border_behind {
                BORDER_BEHIND
            } else {
                IN_FRONT
            });
            let bw = style.border_width;
            let half = (bw / 2.0) as CGFloat;
            s.setFrame(local);
            s.setPath(Some(&rounded(inset(local, half), radii.inset(bw / 2.0))));
            s.setLineWidth(bw as CGFloat);
            s.setStrokeColor(cg_color(style.border_color).as_deref());
        }
        let ca_border = border && !stroke;
        let (width, color) = if ca_border {
            (style.border_width as CGFloat, cg_color(style.border_color))
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
