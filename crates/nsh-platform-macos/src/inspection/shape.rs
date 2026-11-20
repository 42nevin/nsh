//! Projectable convex outlines. Curves use the renderer's corner segments;
//! a bounds rectangle is only the readout, never the picking predicate.
use nsh_core::{
    Bounds, CornerRadii, Point, Points, Size,
    shape::{self, Segment},
};

pub(super) type P = (f32, f32);

fn cross(a: P, b: P, p: P) -> f32 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

fn edges(p: &[P]) -> impl Iterator<Item = (P, P)> + '_ {
    p.iter().copied().zip(p.iter().copied().cycle().skip(1))
}

fn area(p: &[P]) -> f32 {
    edges(p).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum()
}

pub(super) fn bounds(points: &[P]) -> Option<(P, P)> {
    let first = *points.first()?;
    Some(
        points[1..]
            .iter()
            .fold((first, first), |(lo, hi), &(x, y)| {
                ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y)))
            }),
    )
}

pub(super) fn rect(a: P, b: P) -> Vec<P> {
    vec![a, (b.0, a.1), b, (a.0, b.1)]
}

pub(super) fn contains(shape: &[P], p: P) -> bool {
    let a = area(shape);
    a != 0.0 && edges(shape).all(|(u, v)| cross(u, v, p) * a >= 0.0)
}

/// Clips one convex outline by another, including outlines mirrored by a transform.
pub(super) fn intersection(mut subject: Vec<P>, clip: &[P]) -> Vec<P> {
    let sign = area(clip).signum();
    if clip.len() < 3 || area(clip) == 0.0 {
        return Vec::new();
    }
    for (a, b) in edges(clip) {
        let old = std::mem::take(&mut subject);
        for (u, v) in edges(&old) {
            let x = cross(a, b, u) * sign;
            let y = cross(a, b, v) * sign;
            if (x >= 0.0) != (y >= 0.0) {
                let t = x / (x - y);
                subject.push((u.0 + t * (v.0 - u.0), u.1 + t * (v.1 - u.1)));
            }
            if y >= 0.0 {
                subject.push(v);
            }
        }
        if subject.len() < 3 {
            return Vec::new();
        }
    }
    if area(&subject) == 0.0 {
        subject.clear();
    }
    subject
}

pub(super) fn outline(size: Size<Points>, radii: CornerRadii) -> Vec<P> {
    let mut points = Vec::new();
    let mut last = (0.0, 0.0);
    let radii = radii.fit(size);
    shape::rounded_rect(
        Bounds::new(Point::new(Points::ZERO, Points::ZERO), size),
        radii.corners().map(|r| Size::new(*r, *r)),
        radii.continuous,
        false,
        &mut |seg| match seg {
            Segment::Move(p) | Segment::Line(p) => {
                last = (p.x.0, p.y.0);
                if points.last() != Some(&last) {
                    points.push(last);
                }
            }
            Segment::Cubic(curve) => {
                let [b, c, d] = curve.map(|p| (p.x.0, p.y.0));
                flatten(last, b, c, d, 0, &mut points);
                last = d;
            }
            Segment::Close => {}
        },
    );
    if points.first() == points.last() {
        points.pop();
    }
    points
}

fn flatten(a: P, b: P, c: P, d: P, depth: u8, out: &mut Vec<P>) {
    let mid = |a: P, b: P| ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    let chord = (d.0 - a.0).hypot(d.1 - a.1);
    // The outline follows the renderer to within an eighth of a logical point.
    if depth >= 12 || chord > 0.0 && cross(a, d, b).abs().max(cross(a, d, c).abs()) <= chord * 0.125
    {
        if out.last() != Some(&d) {
            out.push(d);
        }
        return;
    }
    let ab = mid(a, b);
    let bc = mid(b, c);
    let cd = mid(c, d);
    let l = mid(ab, bc);
    let r = mid(bc, cd);
    let m = mid(l, r);
    flatten(a, ab, l, m, depth + 1, out);
    flatten(m, r, cd, d, depth + 1, out);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transformed_shape_does_not_pick_its_bounding_box() {
        let diamond = vec![(0.0, 40.0), (40.0, 0.0), (80.0, 40.0), (40.0, 80.0)];
        assert!(!contains(&diamond, (3.0, 3.0)));
        assert!(contains(&diamond, (40.0, 40.0)));
        assert!(intersection(diamond, &rect((0.0, 0.0), (10.0, 10.0))).is_empty());
    }

    #[test]
    fn intersecting_opposite_winding_masks_keeps_only_visible_content() {
        let mut clip = rect((15.0, 0.0), (25.0, 100.0));
        clip.reverse();
        let result = intersection(rect((0.0, 20.0), (100.0, 35.0)), &clip);
        assert_eq!(bounds(&result), Some(((15.0, 20.0), (25.0, 35.0))));
    }
}
