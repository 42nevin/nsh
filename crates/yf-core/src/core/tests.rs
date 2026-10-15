use std::cell::Cell;

use yf_anim::{AnimOp, AnimProp};

use super::*;
use crate::{EventKind, StyleValue};

/// 8pt per char, 16pt per line, wraps at `max_width`. Counts calls.
#[derive(Default)]
struct FakeEngine {
    measures: Cell<u32>,
    layouts: Cell<u32>,
}

impl TextEngine for FakeEngine {
    type Prepared = usize;

    fn prepare(&self, text: &AttributedText) -> usize {
        text.text.chars().count()
    }

    fn measure(&self, chars: &usize, max_width: Option<f32>) -> Size {
        self.measures.set(self.measures.get() + 1);
        let w = *chars as f32 * 8.0;
        match max_width {
            Some(max) if w > max && max > 0.0 => Size::new(max, (w / max).ceil() * 16.0),
            _ => Size::new(w, 16.0),
        }
    }

    fn layout(&self, _: &usize, size: Size) -> TextLayout {
        self.layouts.set(self.layouts.get() + 1);
        TextLayout(Arc::new(size))
    }
}

fn core() -> Core<FakeEngine> {
    Core::new(FakeEngine::default(), Size::new(320.0, 480.0))
}

fn style(props: &[(&str, StyleValue<'_>)]) -> Box<Style> {
    let mut s = Style::default();
    for &(k, v) in props {
        s.set(k, v).unwrap();
    }
    Box::new(s)
}

fn apply(core: &mut Core<FakeEngine>, ops: Vec<Op>) {
    for op in ops {
        core.apply(op).unwrap();
    }
}

fn frame_of(batch: &MountBatch, id: NodeId) -> Option<Rect> {
    batch.items.iter().rev().find_map(|i| match i {
        MountItem::Frame { id: i, frame } if *i == id => Some(*frame),
        _ => None,
    })
}

#[test]
fn mounts_subtree_built_detached() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 4,
                text: "hello".into(),
            },
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("padding", StyleValue::Number(10.0))]),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let creates: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Create { id, kind } => Some((*id, *kind)),
            _ => None,
        })
        .collect();
    // Raw text never becomes a native view.
    assert_eq!(creates, vec![(2, ViewKind::View), (3, ViewKind::Text)]);
    let text = batch.items.iter().find_map(|i| match i {
        MountItem::Text { id: 3, text, .. } => Some(text.clone()),
        _ => None,
    });
    assert_eq!(text.unwrap().text, "hello");
    assert_eq!(
        frame_of(&batch, 2),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 36.0
        })
    );
    assert_eq!(
        frame_of(&batch, 3),
        Some(Rect {
            x: 10.0,
            y: 10.0,
            width: 300.0,
            height: 16.0
        })
    );
    assert!(c.commit().is_none(), "second commit is a no-op");
}

#[test]
fn text_update_emits_only_text_and_frames() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "a".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    c.apply(Op::SetText {
        id: 3,
        text: "abc".into(),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(&batch.items[0], MountItem::Text { id: 2, text, .. } if text.text == "abc"));
    // Text blocks stretch to the column width, so only the content changes.
    assert_eq!(batch.items.len(), 1);
}

#[test]
fn flex_text_in_a_row_shrinks_and_wraps() {
    // Yoga has no automatic minimum size: `flex: 1` text takes the space
    // left in the row and wraps, however long its natural width.
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("flexDirection", StyleValue::Str("row"))]),
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 3,
                style: style(&[
                    ("width", StyleValue::Number(120.0)),
                    ("height", StyleValue::Number(10.0)),
                ]),
            },
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            Op::SetStyle {
                id: 4,
                style: style(&[("flex", StyleValue::Number(1.0))]),
            },
            Op::CreateText {
                id: 5,
                text: "x".repeat(50),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // 400pt of text in 200pt: two lines.
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 120.0,
            y: 0.0,
            width: 200.0,
            height: 32.0
        })
    );
}

#[test]
fn nested_spans_merge_with_styles() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "Count: ".into(),
            },
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            Op::SetStyle {
                id: 4,
                style: style(&[("fontWeight", StyleValue::Str("bold"))]),
            },
            Op::CreateText {
                id: 5,
                text: "1".into(),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let text = batch
        .items
        .iter()
        .find_map(|i| match i {
            MountItem::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.text, "Count: 1");
    assert_eq!(text.spans.len(), 2);
    assert_eq!(text.spans[1].style.font_weight, 700);
    // The span has no native view.
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Create { id: 4, .. }))
    );
}

#[test]
fn placeholder_text_outside_text_is_ignored() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::CreateText {
                id: 3,
                text: String::new(),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: Some(2),
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 4,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let inserts: Vec<_> = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Insert { child, before, .. } => Some((*child, *before)),
            _ => None,
        })
        .collect();
    assert_eq!(inserts, vec![(2, None), (4, None)]);
}

#[test]
fn remove_reinsert_and_release() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: None,
            },
        ],
    );
    c.commit().unwrap();

    // Move 3 before 2: a single insert, no remove.
    c.apply(Op::Insert {
        parent: ROOT_ID,
        child: 3,
        before: Some(2),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(
        batch.items[0],
        MountItem::Insert {
            child: 3,
            before: Some(2),
            ..
        }
    ));
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Remove { .. }))
    );

    c.apply(Op::Remove {
        parent: ROOT_ID,
        child: 2,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(batch.items[0], MountItem::Remove { id: 2 }));

    // Reinserting a detached, already-created view does not recreate it.
    c.apply(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Create { .. }))
    );

    c.apply(Op::Release { id: 2 }).unwrap();
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Delete { id: 2 }))
    );
    assert_eq!(c.node_count(), 2);
    assert_eq!(
        c.apply(Op::SetText {
            id: 2,
            text: String::new()
        }),
        Err(ApplyError::UnknownNode(2))
    );
}

#[test]
fn events_and_errors() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Listen {
                id: 2,
                event: EventKind::Press,
                enabled: true,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::Events { id: 2, events } if events.contains(EventKind::Press))
    ));
    assert_eq!(
        c.apply(Op::Create {
            id: 2,
            tag: Tag::View
        }),
        Err(ApplyError::DuplicateNode(2))
    );
    assert_eq!(
        c.apply(Op::Insert {
            parent: 2,
            child: ROOT_ID,
            before: None
        }),
        Err(ApplyError::Cycle(1))
    );
}

#[test]
fn color_change_skips_layout_and_identical_texts_share_measurements() {
    let mut c = core();
    let mut ops = Vec::new();
    for i in 0..3u32 {
        let (block, raw) = (10 + i * 2, 11 + i * 2);
        ops.extend([
            Op::Create {
                id: block,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: raw,
                text: "same".into(),
            },
            Op::Insert {
                parent: block,
                child: raw,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: block,
                before: None,
            },
        ]);
    }
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    // Three identical texts: measured once per distinct constraint.
    let first_measures = c.text_engine.measures.get();
    assert!(first_measures <= 2, "measured {first_measures} times");
    assert_eq!(c.text_engine.layouts.get(), 3);
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::Text {
            id: 10, content, ..
        } => Some(*content),
        _ => None,
    });
    assert_eq!(
        content,
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 16.0
        })
    );

    c.apply(Op::SetStyle {
        id: 10,
        style: style(&[("color", StyleValue::Str("red"))]),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert_eq!(
        batch.items.len(),
        1,
        "only the new text layout: {:?}",
        batch.items
    );
    assert!(matches!(&batch.items[0], MountItem::Text { id: 10, .. }));
    assert_eq!(
        c.text_engine.measures.get(),
        first_measures,
        "no re-measure"
    );
    assert_eq!(c.text_engine.layouts.get(), 4);
}

#[test]
fn image_uses_natural_size_and_aspect_ratio() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Image,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Source(Some(ImageSource {
                    uri: "asset:/logo.png".into(),
                    size: Some((200.0, 100.0)),
                })),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            Op::Create {
                id: 3,
                tag: Tag::Image,
            },
            Op::SetStyle {
                id: 3,
                style: style(&[
                    ("width", StyleValue::Number(50.0)),
                    ("alignSelf", StyleValue::Str("flex-start")),
                ]),
            },
            Op::SetProp {
                id: 3,
                prop: Prop::Source(Some(ImageSource {
                    uri: "asset:/logo.png".into(),
                    size: Some((200.0, 100.0)),
                })),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Create {
            id: 2,
            kind: ViewKind::Image
        }
    )));
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Image {
            id: 2,
            source: Some(_),
            ..
        }
    )));
    // Stretched to the column width, height from the aspect ratio.
    assert_eq!(
        frame_of(&batch, 2).map(|f| (f.width, f.height)),
        Some((320.0, 160.0))
    );
    assert_eq!(
        frame_of(&batch, 3).map(|f| (f.width, f.height)),
        Some((50.0, 25.0))
    );
}

#[test]
fn scroll_reports_content_size() {
    let mut c = core();
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::Scroll,
        },
        Op::SetStyle {
            id: 2,
            style: style(&[("height", StyleValue::Number(200.0))]),
        },
        Op::Insert {
            parent: ROOT_ID,
            child: 2,
            before: None,
        },
    ];
    for i in 0..5u32 {
        ops.push(Op::Create {
            id: 10 + i,
            tag: Tag::View,
        });
        ops.push(Op::SetStyle {
            id: 10 + i,
            style: style(&[("height", StyleValue::Number(100.0))]),
        });
        ops.push(Op::Insert {
            parent: 2,
            child: 10 + i,
            before: None,
        });
    }
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::ScrollContent { id: 2, size } => Some(*size),
        _ => None,
    });
    assert_eq!(content, Some(Size::new(320.0, 500.0)));
    // Children are not shrunk to the scroll view's height.
    assert_eq!(
        frame_of(&batch, 14).map(|f| (f.y, f.height)),
        Some((400.0, 100.0))
    );
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::ScrollConfig {
            id: 2,
            horizontal: false,
            ..
        }
    )));
}

/// Plays the JS runtime: creates a cell per new slot, renders a fixed-height
/// row for each bind and confirms the index.
fn render_binds(
    c: &mut Core<FakeEngine>,
    next_id: &mut u32,
    cells: &mut Vec<u32>,
    row_height: f32,
) -> usize {
    let mut binds = Vec::new();
    c.take_list_binds(&mut binds);
    let mut count = 0;
    for bind in binds {
        for (slot, index) in bind.binds {
            count += 1;
            while cells.len() <= slot as usize {
                let id = *next_id;
                *next_id += 1;
                apply(
                    c,
                    vec![
                        Op::Create { id, tag: Tag::Cell },
                        Op::SetProp {
                            id,
                            prop: Prop::Slot(CellSlot::Item(cells.len() as u32)),
                        },
                        Op::Insert {
                            parent: bind.list,
                            child: id,
                            before: None,
                        },
                    ],
                );
                cells.push(id);
            }
            // New content for the item, then the confirmation, as the
            // runtime does.
            let cell = cells[slot as usize];
            c.apply(Op::SetStyle {
                id: cell,
                style: style(&[("height", StyleValue::Number(row_height))]),
            })
            .unwrap();
            c.apply(Op::SetProp {
                id: cell,
                prop: Prop::CellIndex(index),
            })
            .unwrap();
        }
    }
    count
}

#[test]
fn list_renders_window_and_recycles_cells() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::List,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("height", StyleValue::Number(400.0))]),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::EstimatedItemSize(50.0),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Count(1000),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    // Estimated 50 pt rows: viewport + one screen of overscan = 17 rows.
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 100.0), 17);
    let batch = c.commit().unwrap();
    // Rows are really 100 pt: positioned by measured sizes.
    assert_eq!(
        frame_of(&batch, cells[3]),
        Some(Rect {
            x: 0.0,
            y: 300.0,
            width: 320.0,
            height: 100.0
        })
    );
    let content = batch.items.iter().rev().find_map(|i| match i {
        MountItem::ScrollContent { id: 2, size } => Some(size.height),
        _ => None,
    });
    assert_eq!(content, Some(17.0 * 100.0 + 983.0 * 50.0));

    // Scroll far down into unmeasured (50 pt estimated) rows: the window
    // needs 25 rows; all 17 existing cells are reused and 8 are added.
    c.set_scroll_offset(2, 0.0, 10_000.0, u32::MAX);
    c.commit();
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 50.0), 25);
    assert_eq!(cells.len(), 25);
    c.commit();
    // Scrolling on reuses cells only.
    c.set_scroll_offset(2, 0.0, 20_000.0, u32::MAX);
    c.commit();
    assert_eq!(render_binds(&mut c, &mut next_id, &mut cells, 50.0), 25);
    assert_eq!(cells.len(), 25);
    c.commit();
    assert_eq!(c.node_count(), 2 + cells.len());
}

#[test]
fn list_keeps_visible_content_anchored() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::List,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("height", StyleValue::Number(400.0))]),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::EstimatedItemSize(50.0),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Count(100),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    // Jump into the middle before anything above was measured.
    c.set_scroll_offset(2, 0.0, 2000.0, u32::MAX);
    c.commit();
    let (mut next_id, mut cells) = (100, Vec::new());
    render_binds(&mut c, &mut next_id, &mut cells, 50.0);
    c.commit();
    // Scroll back up so rows above the old position get rendered at 80 pt.
    c.set_scroll_offset(2, 0.0, 1200.0, u32::MAX);
    c.commit();
    render_binds(&mut c, &mut next_id, &mut cells, 80.0);
    let batch = c.commit().unwrap();
    let shift: f32 = batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::ScrollBy { id: 2, dy, .. } => Some(*dy),
            _ => None,
        })
        .sum();
    assert!(
        shift > 0.0,
        "rows above the viewport grew; content must be shifted down"
    );
}

#[test]
fn anim_ops_and_bindings_are_forwarded_in_order() {
    let mut c = core();
    let mut s = Style::default();
    s.animated.push((AnimProp::Opacity, 7));
    apply(
        &mut c,
        vec![
            Op::Anim(AnimOp::CreateValue {
                node: 7,
                value: 1.0,
            }),
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: Box::new(s),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    let anim = batch
        .items
        .iter()
        .position(|i| matches!(i, MountItem::Anim(AnimOp::CreateValue { node: 7, .. })));
    let bind = batch
        .items
        .iter()
        .position(|i| matches!(i, MountItem::AnimBindings { id: 2, .. }));
    assert!(
        anim.unwrap() < bind.unwrap(),
        "values exist before views bind them"
    );
}

#[test]
fn leaf_styles_are_resolved() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            // Text with padding: the frame wraps the measured text.
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("padding", StyleValue::Number(10.0))]),
            },
            Op::CreateText {
                id: 3,
                text: "abc".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
            // Empty leaf view: min size and aspect ratio still apply.
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 4,
                style: style(&[
                    ("width", StyleValue::Number(100.0)),
                    ("aspectRatio", StyleValue::Number(2.0)),
                    ("alignSelf", StyleValue::Str("flex-start")),
                ]),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 4,
                before: None,
            },
            Op::Create {
                id: 5,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 5,
                style: style(&[("minHeight", StyleValue::Number(80.0))]),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 5,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(16.0 + 20.0));
    let content = batch.items.iter().find_map(|i| match i {
        MountItem::Text { id: 2, content, .. } => Some(*content),
        _ => None,
    });
    assert_eq!(
        content,
        Some(Rect {
            x: 10.0,
            y: 10.0,
            width: 300.0,
            height: 16.0
        })
    );
    assert_eq!(
        frame_of(&batch, 4).map(|f| (f.width, f.height)),
        Some((100.0, 50.0))
    );
    assert_eq!(frame_of(&batch, 5).map(|f| f.height), Some(80.0));
}

/// `cargo test --release -p yf-core bench_row_relayout -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_row_relayout() {
    let mut c = core();
    // A gallery-like row: [image] [title / author] in a list cell.
    let row = |s: &[(&str, StyleValue<'_>)]| style(s);
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: row(&[
                    ("flexDirection", StyleValue::Str("row")),
                    ("alignItems", StyleValue::Str("center")),
                    ("gap", StyleValue::Number(12.0)),
                    ("padding", StyleValue::Number(8.0)),
                ]),
            },
            Op::Create {
                id: 3,
                tag: Tag::Image,
            },
            Op::SetStyle {
                id: 3,
                style: row(&[
                    ("width", StyleValue::Number(48.0)),
                    ("height", StyleValue::Number(48.0)),
                ]),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 4,
                style: row(&[("flex", StyleValue::Number(1.0))]),
            },
            Op::Insert {
                parent: 2,
                child: 4,
                before: None,
            },
            Op::Create {
                id: 5,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 6,
                text: "Photo #1".into(),
            },
            Op::Insert {
                parent: 5,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Create {
                id: 7,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 8,
                text: "by Ada".into(),
            },
            Op::Insert {
                parent: 7,
                child: 8,
                before: None,
            },
            Op::Insert {
                parent: 4,
                child: 7,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit();
    let n = 2000;
    let start = std::time::Instant::now();
    for i in 0..n {
        c.apply(Op::SetText {
            id: 6,
            text: format!("Photo #{i}"),
        })
        .unwrap();
        c.apply(Op::SetText {
            id: 8,
            text: format!("by {}", ["Ada", "Ken"][i % 2]),
        })
        .unwrap();
        c.commit();
    }
    eprintln!("row relayout: {:?} per commit", start.elapsed() / n as u32);
}

/// `cargo test --release -p yf-core bench_cell_relayout -- --ignored --nocapture`
#[test]
#[ignore]
fn bench_cell_relayout() {
    let mut c = Core::new(crate::ApproxTextEngine, Size::new(390.0, 800.0));
    let s = |p: &[(&str, StyleValue<'_>)]| style(p);
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::List,
        },
        Op::SetStyle {
            id: 2,
            style: s(&[("flex", StyleValue::Number(1.0))]),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::EstimatedItemSize(64.0),
        },
        Op::SetProp {
            id: 2,
            prop: Prop::Count(1000),
        },
        Op::Insert {
            parent: ROOT_ID,
            child: 2,
            before: None,
        },
    ];
    for op in std::mem::take(&mut ops) {
        c.apply(op).unwrap();
    }
    c.commit();
    let mut binds = Vec::new();
    c.take_list_binds(&mut binds);
    // One cell with a gallery row.
    for op in [
        Op::Create {
            id: 10,
            tag: Tag::Cell,
        },
        Op::SetProp {
            id: 10,
            prop: Prop::Slot(CellSlot::Item(0)),
        },
        Op::Insert {
            parent: 2,
            child: 10,
            before: None,
        },
        Op::Create {
            id: 11,
            tag: Tag::View,
        },
        Op::SetStyle {
            id: 11,
            style: s(&[
                ("flexDirection", StyleValue::Str("row")),
                ("alignItems", StyleValue::Str("center")),
                ("gap", StyleValue::Number(12.0)),
                ("paddingHorizontal", StyleValue::Number(16.0)),
                ("paddingVertical", StyleValue::Number(8.0)),
            ]),
        },
        Op::Insert {
            parent: 10,
            child: 11,
            before: None,
        },
        Op::Create {
            id: 12,
            tag: Tag::Image,
        },
        Op::SetStyle {
            id: 12,
            style: s(&[
                ("width", StyleValue::Number(48.0)),
                ("height", StyleValue::Number(48.0)),
            ]),
        },
        Op::Insert {
            parent: 11,
            child: 12,
            before: None,
        },
        Op::Create {
            id: 13,
            tag: Tag::View,
        },
        Op::SetStyle {
            id: 13,
            style: s(&[
                ("flex", StyleValue::Number(1.0)),
                ("gap", StyleValue::Number(2.0)),
            ]),
        },
        Op::Insert {
            parent: 11,
            child: 13,
            before: None,
        },
        Op::Create {
            id: 14,
            tag: Tag::Text,
        },
        Op::CreateText {
            id: 15,
            text: "Photo #1".into(),
        },
        Op::Insert {
            parent: 14,
            child: 15,
            before: None,
        },
        Op::Insert {
            parent: 13,
            child: 14,
            before: None,
        },
        Op::Create {
            id: 16,
            tag: Tag::Text,
        },
        Op::CreateText {
            id: 17,
            text: "by Ada".into(),
        },
        Op::Insert {
            parent: 16,
            child: 17,
            before: None,
        },
        Op::Insert {
            parent: 13,
            child: 16,
            before: None,
        },
        Op::SetProp {
            id: 10,
            prop: Prop::CellIndex(0),
        },
    ] {
        c.apply(op).unwrap();
    }
    c.commit();
    for idle in [0u64, 10] {
        let n = if idle == 0 { 2000 } else { 100 };
        let mut total = std::time::Duration::ZERO;
        for i in 0..n {
            std::thread::sleep(std::time::Duration::from_millis(idle));
            let start = std::time::Instant::now();
            c.apply(Op::SetText {
                id: 15,
                text: format!("Photo #{i}"),
            })
            .unwrap();
            c.apply(Op::SetText {
                id: 17,
                text: format!("by {}", ["Ada", "Ken"][i % 2]),
            })
            .unwrap();
            c.commit();
            total += start.elapsed();
        }
        eprintln!(
            "cell relayout ({idle} ms idle between frames): {:?} per commit",
            total / n as u32
        );
    }
}

fn input_core() -> Core<FakeEngine> {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Input,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::Placeholder("Name".into()),
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("padding", StyleValue::Number(6.0))]),
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 2,
                before: None,
            },
        ],
    );
    c
}

#[test]
fn input_measures_its_placeholder_and_reports_its_content_box() {
    let mut c = input_core();
    let batch = c.commit().unwrap();
    // One line of text (16pt) plus padding, stretched across the root.
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(28.0));
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::Input { id: 2, config } if config.placeholder == "Name" && !config.multiline)));
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::InputContent { id: 2, content } if *content == Rect { x: 6.0, y: 6.0, width: 308.0, height: 16.0 })));
    // No text yet: nothing to send.
    assert!(
        !batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::InputValue { .. }))
    );
}

#[test]
fn input_values_from_js_and_native_edits() {
    let mut c = input_core();
    c.commit();
    // The user types; JS echoes the same text: nothing goes back.
    c.input_changed(2, "ab", 2);
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Value("ab".into()),
    })
    .unwrap();
    assert!(c.commit().is_none());
    // JS transforms the text: it is sent with the edit count it is based on.
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Value("AB".into()),
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(batch.items.iter().any(|i| matches!(i,
        MountItem::InputValue { id: 2, text, count: 2 } if text == "AB")));
    // `defaultValue` no longer applies once a value was set.
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::DefaultValue("x".into()),
    })
    .unwrap();
    assert!(c.commit().is_none());
}

#[test]
fn multiline_inputs_grow_with_their_text() {
    let mut c = input_core();
    c.apply(Op::SetProp {
        id: 2,
        prop: Prop::Multiline(true),
    })
    .unwrap();
    c.commit();
    // 50 characters at 8pt in 308pt: two lines.
    c.input_changed(2, &"x".repeat(50), 1);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(44.0));
    // A trailing line break starts an empty line that still takes space.
    c.input_changed(2, "x\n", 2);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 2).map(|f| f.height), Some(28.0));
}

#[test]
fn commands_reach_created_views_only() {
    let mut c = input_core();
    c.apply(Op::Command {
        id: 2,
        command: crate::Command::Focus,
    })
    .unwrap();
    let batch = c.commit().unwrap();
    assert!(matches!(
        batch.items.last(),
        Some(MountItem::Command {
            id: 2,
            command: crate::Command::Focus
        })
    ));
    c.apply(Op::Create {
        id: 3,
        tag: Tag::Input,
    })
    .unwrap();
    c.apply(Op::Command {
        id: 3,
        command: crate::Command::Focus,
    })
    .unwrap();
    assert!(c.commit().is_none());
}

fn insert_items(batch: &MountBatch) -> Vec<(NodeId, NodeId, Option<NodeId>)> {
    batch
        .items
        .iter()
        .filter_map(|i| match i {
            MountItem::Insert {
                parent,
                child,
                before,
            } => Some((*parent, *child, *before)),
            _ => None,
        })
        .collect()
}

#[test]
fn z_index_orders_native_views() {
    let mut c = core();
    let z = |v: f32| style(&[("zIndex", StyleValue::Number(v))]);
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: z(5.0),
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 3,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // 3 (z 0) goes below 2 (z 5) although it comes later in the tree.
    assert_eq!(insert_items(&batch), vec![(1, 2, None), (1, 3, Some(2))]);
    // Raising 3 above 2 moves it to the top.
    apply(
        &mut c,
        vec![Op::SetStyle {
            id: 3,
            style: z(9.0),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(insert_items(&batch), vec![(1, 3, None)]);
    // A new child with the default z index goes below both.
    apply(
        &mut c,
        vec![Op::Insert {
            parent: 1,
            child: 4,
            before: None,
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(insert_items(&batch), vec![(1, 4, Some(2))]);
}

#[test]
fn layout_events_carry_frames_and_late_listeners_get_one() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("height", StyleValue::Number(40.0))]),
            },
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: true,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    let mut events = Vec::new();
    c.take_events(&mut events);
    assert_eq!(
        events,
        vec![(
            2,
            EventKind::Layout,
            EventPayload::Layout {
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: 40.0
            }
        )]
    );
    // Unchanged frames report nothing; a listener added later learns the
    // current frame.
    apply(
        &mut c,
        vec![
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: false,
            },
            Op::Listen {
                id: 2,
                event: EventKind::Layout,
                enabled: true,
            },
        ],
    );
    c.commit();
    events.clear();
    c.take_events(&mut events);
    assert_eq!(events.len(), 1);
}

#[test]
fn font_scale_scales_text_that_allows_it() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 3,
                text: "abc".into(),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Create {
                id: 4,
                tag: Tag::Text,
            },
            Op::CreateText {
                id: 5,
                text: "abc".into(),
            },
            Op::SetProp {
                id: 4,
                prop: Prop::AllowFontScaling(false),
            },
            Op::Insert {
                parent: 4,
                child: 5,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 4,
                before: None,
            },
        ],
    );
    c.commit().unwrap();
    c.set_font_scale(2.0);
    let batch = c.commit().unwrap();
    let size = |id| {
        batch.items.iter().find_map(|i| match i {
            MountItem::Text { id: i, text, .. } if *i == id => Some(text.spans[0].style.font_size),
            _ => None,
        })
    };
    assert_eq!(size(2), Some(28.0));
    assert_eq!(size(4), None, "text without font scaling is unchanged");
}

#[test]
fn layout_animation_leads_the_batch() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
            Op::Command {
                id: 1,
                command: Command::LayoutAnimation(crate::LayoutAnimation {
                    create: None,
                    update: None,
                    delete: None,
                }),
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(
        matches!(batch.items[0], MountItem::LayoutAnimation(_)),
        "{:?}",
        batch.items
    );
}

#[test]
fn view_props_and_gestures_reach_the_platform() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::View,
            },
            Op::SetProp {
                id: 2,
                prop: Prop::HitSlop(crate::EdgeInsets::all(10.0)),
            },
            Op::SetProp {
                id: 2,
                prop: Prop::PinchValue(Some(7)),
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert!(
        batch.items.iter().any(
            |i| matches!(i, MountItem::ViewProps { id: 2, props } if props.hit_slop.top == 10.0)
        )
    );
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::Gestures { id: 2, values } if values.pinch == Some(7)))
    );
}

#[test]
fn screens_fill_their_stack_below_the_header() {
    let mut c = core();
    c.set_safe_area(crate::EdgeInsets {
        top: 20.0,
        ..Default::default()
    });
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Stack,
            },
            Op::SetStyle {
                id: 2,
                style: style(&[("flex", StyleValue::Number(1.0))]),
            },
            Op::Create {
                id: 3,
                tag: Tag::Screen,
            },
            Op::Create {
                id: 4,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 4,
                style: style(&[("flex", StyleValue::Number(1.0))]),
            },
            Op::Insert {
                parent: 3,
                child: 4,
                before: None,
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // The screen fills the stack; its content starts below the status bar
    // and the 44pt bar.
    assert_eq!(
        frame_of(&batch, 3),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 480.0
        })
    );
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 0.0,
            y: 64.0,
            width: 320.0,
            height: 416.0
        })
    );
    assert!(batch.items.iter().any(
        |i| matches!(i, MountItem::ScreenConfig { id: 3, config } if config.header_height == 64.0)
    ));
    // Without a header, the content starts at the top.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: 3,
            prop: Prop::Screen(crate::ScreenProp::HeaderShown(false)),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(
        frame_of(&batch, 4),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 480.0
        })
    );
}

#[test]
fn modals_lay_out_at_the_reported_size() {
    let mut c = core();
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 2,
                tag: Tag::Modal,
            },
            Op::Create {
                id: 3,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 3,
                style: style(&[("flex", StyleValue::Number(1.0))]),
            },
            Op::Insert {
                parent: 2,
                child: 3,
                before: None,
            },
            Op::Insert {
                parent: 1,
                child: 2,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    // The viewport until the platform reports the presented size.
    assert_eq!(frame_of(&batch, 3).map(|f| f.height), Some(480.0));
    c.set_root_size(2, Size::new(320.0, 400.0), crate::EdgeInsets::default());
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.height), Some(400.0));
}

#[test]
fn right_to_left_layouts() {
    let mut c = core();
    let n = |v: f32| StyleValue::Number(v);
    let s = |v: &'static str| StyleValue::Str(v);
    let mut ops = vec![
        Op::Create {
            id: 2,
            tag: Tag::View,
        },
        Op::SetStyle {
            id: 2,
            style: style(&[
                ("flexDirection", s("row")),
                ("width", n(300.0)),
                ("height", n(50.0)),
            ]),
        },
    ];
    // Two boxes in a row: the first with `marginStart`, the second with
    // `marginLeft` (swapped right to left, as in React Native).
    for (id, key) in [(3, "marginStart"), (4, "marginLeft")] {
        ops.push(Op::Create { id, tag: Tag::View });
        ops.push(Op::SetStyle {
            id,
            style: style(&[("width", n(50.0)), ("height", n(50.0)), (key, n(10.0))]),
        });
        ops.push(Op::Insert {
            parent: 2,
            child: id,
            before: None,
        });
    }
    ops.push(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    });
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(10.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(70.0));

    // The surface turns right to left: the row runs from the right.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(true),
        }],
    );
    let batch = c.commit().unwrap();
    assert!(
        batch
            .items
            .iter()
            .any(|i| matches!(i, MountItem::LayoutDirection { rtl: true }))
    );
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(300.0 - 10.0 - 50.0));
    assert_eq!(
        frame_of(&batch, 4).map(|f| f.x),
        Some(300.0 - 60.0 - 10.0 - 50.0)
    );

    // A subtree built detached and inserted takes the direction too; an
    // explicit `direction` overrides it.
    apply(
        &mut c,
        vec![
            Op::Create {
                id: 5,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 5,
                style: style(&[
                    ("flexDirection", s("row")),
                    ("direction", s("ltr")),
                    ("height", n(20.0)),
                ]),
            },
            Op::Create {
                id: 6,
                tag: Tag::View,
            },
            Op::SetStyle {
                id: 6,
                style: style(&[
                    ("width", n(20.0)),
                    ("start", n(5.0)),
                    ("position", s("absolute")),
                ]),
            },
            Op::Insert {
                parent: 5,
                child: 6,
                before: None,
            },
            Op::Insert {
                parent: ROOT_ID,
                child: 5,
                before: None,
            },
        ],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 6).map(|f| f.x), Some(5.0));

    // Back to left to right.
    apply(
        &mut c,
        vec![Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(false),
        }],
    );
    let batch = c.commit().unwrap();
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(10.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(70.0));
}

#[test]
fn right_to_left_horizontal_scrolls() {
    let mut c = core();
    let n = |v: f32| StyleValue::Number(v);
    let mut ops = vec![
        Op::SetProp {
            id: ROOT_ID,
            prop: Prop::LayoutDirection(true),
        },
        Op::Create {
            id: 2,
            tag: Tag::Scroll,
        },
        Op::SetProp {
            id: 2,
            prop: Prop::Horizontal(true),
        },
        Op::SetStyle {
            id: 2,
            style: style(&[
                ("width", n(150.0)),
                ("height", n(50.0)),
                ("paddingStart", n(10.0)),
            ]),
        },
    ];
    for id in [3, 4] {
        ops.push(Op::Create { id, tag: Tag::View });
        ops.push(Op::SetStyle {
            id,
            style: style(&[("width", n(100.0)), ("height", n(50.0))]),
        });
        ops.push(Op::Insert {
            parent: 2,
            child: id,
            before: None,
        });
    }
    ops.push(Op::Insert {
        parent: ROOT_ID,
        child: 2,
        before: None,
    });
    apply(&mut c, ops);
    let batch = c.commit().unwrap();
    // Content: 10 (start padding) + 200; the first child at the right end.
    assert_eq!(frame_of(&batch, 3).map(|f| f.x), Some(210.0 - 10.0 - 100.0));
    assert_eq!(frame_of(&batch, 4).map(|f| f.x), Some(0.0));
    // It starts scrolled to its right end.
    assert!(batch.items.iter().any(|i| matches!(
        i,
        MountItem::Command {
            id: 2,
            command: Command::ScrollTo { x, .. }
        } if *x == 60.0
    )));
}
