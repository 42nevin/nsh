//! CSS grid syntax for style strings and classes: track lists
//! (`repeat(3, minmax(0, 1fr))`, `1fr 2fr 100px`), placements
//! (`span 2 / span 2`, `1 / -1`) and `grid-auto-flow`.

use taffy::{
    GridAutoFlow, GridPlacement, GridTemplateComponent, GridTemplateRepetition, Line,
    MaxTrackSizingFunction, MinTrackSizingFunction, RepetitionCount, TrackSizingFunction,
    style_helpers::{TaffyGridLine, TaffyGridSpan},
};

type Max = MaxTrackSizingFunction;
type Min = MinTrackSizingFunction;

use crate::css::{self, split_top, words};

/// A template track list (`grid-template-columns`); `none` is empty.
pub(crate) fn template(s: &str) -> Option<Vec<GridTemplateComponent<String>>> {
    let s = s.trim();
    if s == "none" || s.is_empty() {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for word in words(s) {
        if let Some(args) = word.strip_prefix("repeat(") {
            let args = args.strip_suffix(')')?;
            let (count, tracks) = args.split_once(',')?;
            let count = match count.trim() {
                "auto-fill" => RepetitionCount::AutoFill,
                "auto-fit" => RepetitionCount::AutoFit,
                n => RepetitionCount::Count(n.parse().ok().filter(|&n: &u16| n > 0)?),
            };
            out.push(GridTemplateComponent::Repeat(GridTemplateRepetition {
                count,
                tracks: track_list(tracks)?,
                line_names: Vec::new(),
            }));
        } else {
            out.push(GridTemplateComponent::Single(track(word)?));
        }
    }
    Some(out)
}

/// Implicit track sizes (`grid-auto-rows`).
pub(crate) fn auto_tracks(s: &str) -> Option<Vec<TrackSizingFunction>> {
    track_list(s)
}

fn track_list(s: &str) -> Option<Vec<TrackSizingFunction>> {
    words(s).into_iter().map(track).collect()
}

/// One track size: `1fr`, `100px`, `25%`, `auto`, `min-content`,
/// `max-content`, `minmax(a, b)`, `fit-content(100px)`.
fn track(s: &str) -> Option<TrackSizingFunction> {
    let s = s.trim();
    if let Some(args) = s.strip_prefix("minmax(") {
        let parts = split_top(args.strip_suffix(')')?, ',');
        let [min, max] = parts.as_slice() else {
            return None;
        };
        return Some(TrackSizingFunction {
            min: min_track(min)?,
            max: max_track(max)?,
        });
    }
    if let Some(arg) = s.strip_prefix("fit-content(") {
        let max = match css::length_or_percent(arg.strip_suffix(')')?)? {
            Ok(points) => Max::fit_content_px(points),
            Err(fraction) => Max::fit_content_percent(fraction),
        };
        return Some(TrackSizingFunction {
            min: Min::auto(),
            max,
        });
    }
    let max = max_track(s)?;
    // A flexible track's minimum is `auto` (CSS); fixed tracks are their
    // own minimum.
    let min = if s.ends_with("fr") {
        Min::auto()
    } else {
        min_track(s)?
    };
    Some(TrackSizingFunction { min, max })
}

fn min_track(s: &str) -> Option<Min> {
    Some(match s.trim() {
        "auto" => Min::auto(),
        "min-content" => Min::min_content(),
        "max-content" => Min::max_content(),
        s => match css::length_or_percent(s)? {
            Ok(points) => Min::length(points),
            Err(fraction) => Min::percent(fraction),
        },
    })
}

fn max_track(s: &str) -> Option<Max> {
    let s = s.trim();
    if let Some(fr) = s.strip_suffix("fr") {
        return Some(Max::fr(css::number(fr)?.max(0.0)));
    }
    Some(match s {
        "auto" => Max::auto(),
        "min-content" => Max::min_content(),
        "max-content" => Max::max_content(),
        s => match css::length_or_percent(s)? {
            Ok(points) => Max::length(points),
            Err(fraction) => Max::percent(fraction),
        },
    })
}

/// One end of a placement: `auto`, `3`, `-1`, `span 2`.
pub(crate) fn placement(s: &str) -> Option<GridPlacement<String>> {
    let s = s.trim();
    if s == "auto" {
        return Some(GridPlacement::Auto);
    }
    if let Some(n) = s.strip_prefix("span") {
        let n: u16 = n.trim().parse().ok().filter(|&n| n > 0)?;
        return Some(GridPlacement::from_span(n));
    }
    let n: i16 = s.parse().ok().filter(|&n| n != 0)?;
    Some(GridPlacement::from_line_index(n))
}

/// `grid-column` / `grid-row`: `start [/ end]`; a lone span or line keeps
/// `end` automatic.
pub(crate) fn line(s: &str) -> Option<Line<GridPlacement<String>>> {
    match s.split_once('/') {
        Some((a, b)) => Some(Line {
            start: placement(a)?,
            end: placement(b)?,
        }),
        None => Some(Line {
            start: placement(s)?,
            end: GridPlacement::Auto,
        }),
    }
}

/// `grid-auto-flow`.
pub(crate) fn auto_flow(s: &str) -> Option<GridAutoFlow> {
    let w = words(s);
    let dense = w.contains(&"dense");
    let column = w.contains(&"column");
    if w.iter().any(|x| !matches!(*x, "dense" | "column" | "row")) {
        return None;
    }
    Some(match (column, dense) {
        (false, false) => GridAutoFlow::Row,
        (true, false) => GridAutoFlow::Column,
        (false, true) => GridAutoFlow::RowDense,
        (true, true) => GridAutoFlow::ColumnDense,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let t = template("repeat(3, minmax(0, 1fr))").unwrap();
        assert_eq!(t.len(), 1);
        assert!(matches!(&t[0], GridTemplateComponent::Repeat(r) if r.tracks.len() == 1));
        let t = template("1fr 2fr 100px auto").unwrap();
        assert_eq!(t.len(), 4);
        assert!(template("none").unwrap().is_empty());
        assert!(template("1zz").is_none());
    }

    #[test]
    fn placements() {
        let l = line("span 2 / span 2").unwrap();
        assert_eq!(l.start, GridPlacement::Span(2));
        let l = line("1 / -1").unwrap();
        assert_eq!(l.end, GridPlacement::from_line_index(-1));
        assert_eq!(line("3").unwrap().end, GridPlacement::Auto);
        assert!(placement("0").is_none());
        assert_eq!(auto_flow("column dense"), Some(GridAutoFlow::ColumnDense));
    }
}
