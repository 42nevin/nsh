use crate::{
    driver::Running,
    ops::{AnimId, AnimOp, Extrapolate, MathOp, NodeId, Operand, Output},
};

/// The evaluated value of a node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimValue {
    /// A number.
    Number(f32),
    /// A color, `0xRRGGBBAA`.
    Color(u32),
}

impl AnimValue {
    /// The number, or 0 for colors.
    pub fn number(self) -> f32 {
        match self {
            AnimValue::Number(n) => n,
            AnimValue::Color(_) => 0.0,
        }
    }
}

/// A finished (or interrupted) animation, to report back to JS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Finished {
    /// The animation.
    pub animation: AnimId,
    /// False if it was stopped or replaced before reaching its end.
    pub finished: bool,
}

#[derive(Debug)]
enum Node {
    Value {
        value: f32,
        velocity: f32,
        running: Option<Running>,
    },
    Interpolate {
        input: NodeId,
        input_range: Vec<f32>,
        output: Output,
        extrapolate: Extrapolate,
    },
    Math {
        op: MathOp,
        a: Operand,
        b: Operand,
    },
}

/// Nodes deeper than this are treated as cycles (JS bug) and evaluate to 0.
const MAX_DEPTH: u32 = 64;

/// All animation nodes of one surface.
#[derive(Debug, Default)]
pub struct Graph {
    /// Indexed by node id (dense, assigned by JS).
    nodes: Vec<Option<Node>>,
    /// Value nodes with a running animation.
    animating: Vec<NodeId>,
}

impl Graph {
    /// Applies an op. Interrupted animations are pushed to `finished`.
    pub fn apply(&mut self, op: AnimOp, finished: &mut Vec<Finished>) {
        match op {
            AnimOp::CreateValue { node, value } => self.insert(
                node,
                Node::Value {
                    value,
                    velocity: 0.0,
                    running: None,
                },
            ),
            AnimOp::CreateInterpolate {
                node,
                input,
                input_range,
                output,
                extrapolate,
            } => self.insert(
                node,
                Node::Interpolate {
                    input,
                    input_range,
                    output,
                    extrapolate,
                },
            ),
            AnimOp::CreateMath { node, op, a, b } => self.insert(node, Node::Math { op, a, b }),
            AnimOp::Set { node, value } => self.set(node, value, finished),
            AnimOp::Start {
                node,
                animation,
                driver,
            } => {
                if let Some(Some(Node::Value {
                    value,
                    velocity,
                    running,
                })) = self.nodes.get_mut(node as usize)
                {
                    if let Some(old) =
                        running.replace(Running::new(animation, driver, *value, *velocity))
                    {
                        finished.push(Finished {
                            animation: old.id,
                            finished: false,
                        });
                    }
                    if !self.animating.contains(&node) {
                        self.animating.push(node);
                    }
                }
            }
            AnimOp::Stop { node } => self.stop(node, finished),
            AnimOp::Release { node } => {
                self.stop(node, finished);
                if let Some(slot) = self.nodes.get_mut(node as usize) {
                    *slot = None;
                }
            }
        }
    }

    fn insert(&mut self, id: NodeId, node: Node) {
        let idx = id as usize;
        if idx >= self.nodes.len() {
            self.nodes
                .resize_with((idx + 1).next_power_of_two(), || None);
        }
        self.nodes[idx] = Some(node);
    }

    /// Sets a value directly (gestures, scroll offsets, JS `set`), stopping
    /// its animation.
    pub fn set(&mut self, node: NodeId, new: f32, finished: &mut Vec<Finished>) {
        self.stop(node, finished);
        if let Some(Some(Node::Value {
            value, velocity, ..
        })) = self.nodes.get_mut(node as usize)
        {
            *value = new;
            *velocity = 0.0;
        }
    }

    fn stop(&mut self, node: NodeId, finished: &mut Vec<Finished>) {
        if let Some(Some(Node::Value {
            running, velocity, ..
        })) = self.nodes.get_mut(node as usize)
            && let Some(old) = running.take()
        {
            *velocity = 0.0;
            finished.push(Finished {
                animation: old.id,
                finished: false,
            });
        }
        self.animating.retain(|&n| n != node);
    }

    /// The current value of a value node.
    pub fn value(&self, node: NodeId) -> Option<f32> {
        match self.nodes.get(node as usize)? {
            Some(Node::Value { value, .. }) => Some(*value),
            _ => None,
        }
    }

    /// True while any animation runs (the platform keeps its frame clock on).
    pub fn is_animating(&self) -> bool {
        !self.animating.is_empty()
    }

    /// Advances all animations to `now` (seconds, monotonic). Completed
    /// animations are pushed to `finished`. Returns true if a value changed.
    pub fn tick(&mut self, now: f64, finished: &mut Vec<Finished>) -> bool {
        let mut changed = false;
        let mut i = 0;
        while i < self.animating.len() {
            let node = self.animating[i];
            let done = match self.nodes.get_mut(node as usize) {
                Some(Some(Node::Value {
                    value,
                    velocity,
                    running: running @ Some(_),
                })) => {
                    let r = running.as_mut().expect("checked");
                    let (v, vel, done) = r.step(now);
                    changed |= v != *value;
                    *value = v;
                    *velocity = vel;
                    if done {
                        finished.push(Finished {
                            animation: r.id,
                            finished: true,
                        });
                        *running = None;
                    }
                    done
                }
                _ => true,
            };
            if done {
                self.animating.swap_remove(i);
            } else {
                i += 1;
            }
        }
        changed
    }

    /// Evaluates any node.
    pub fn eval(&self, node: NodeId) -> AnimValue {
        self.eval_depth(node, 0)
    }

    fn eval_depth(&self, node: NodeId, depth: u32) -> AnimValue {
        if depth > MAX_DEPTH {
            return AnimValue::Number(0.0);
        }
        match self.nodes.get(node as usize) {
            Some(Some(Node::Value { value, .. })) => AnimValue::Number(*value),
            Some(Some(Node::Interpolate {
                input,
                input_range,
                output,
                extrapolate,
            })) => interpolate(
                self.eval_depth(*input, depth + 1).number(),
                input_range,
                output,
                *extrapolate,
            ),
            Some(Some(Node::Math { op, a, b })) => {
                let operand = |o: &Operand| match *o {
                    Operand::Node(n) => self.eval_depth(n, depth + 1).number(),
                    Operand::Const(c) => c,
                };
                let (a, b) = (operand(a), operand(b));
                AnimValue::Number(match op {
                    MathOp::Add => a + b,
                    MathOp::Sub => a - b,
                    MathOp::Mul => a * b,
                    MathOp::Div if b != 0.0 => a / b,
                    MathOp::Div => 0.0,
                })
            }
            _ => AnimValue::Number(0.0),
        }
    }
}

fn interpolate(x: f32, input: &[f32], output: &Output, extrapolate: Extrapolate) -> AnimValue {
    let n = input.len().min(match output {
        Output::Numbers(v) => v.len(),
        Output::Colors(v) => v.len(),
    });
    if n == 0 {
        return AnimValue::Number(x);
    }
    let (first, last) = (input[0], input[n - 1]);
    if n == 1 || (x < first || x > last) && extrapolate != Extrapolate::Extend {
        if extrapolate == Extrapolate::Identity && n > 1 {
            return AnimValue::Number(x);
        }
        let i = if n == 1 || x < first { 0 } else { n - 1 };
        return match output {
            Output::Numbers(v) => AnimValue::Number(v[i]),
            Output::Colors(v) => AnimValue::Color(v[i]),
        };
    }
    // Segment containing x (or the edge segment when extending).
    let seg = input[..n]
        .windows(2)
        .position(|w| x <= w[1])
        .unwrap_or(n - 2);
    let (a, b) = (input[seg], input[seg + 1]);
    let t = if b == a { 0.0 } else { (x - a) / (b - a) };
    match output {
        Output::Numbers(v) => AnimValue::Number(v[seg] + (v[seg + 1] - v[seg]) * t),
        Output::Colors(v) => AnimValue::Color(lerp_color(v[seg], v[seg + 1], t.clamp(0.0, 1.0))),
    }
}

fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let mut out = 0u32;
    for shift in [24, 16, 8, 0] {
        let ca = ((a >> shift) & 0xff) as f32;
        let cb = ((b >> shift) & 0xff) as f32;
        out |= ((ca + (cb - ca) * t).round() as u32 & 0xff) << shift;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::{Driver, Easing};

    #[test]
    fn interpolation_and_extrapolation() {
        let out = Output::Numbers(vec![0.0, 10.0, 0.0]);
        let r = [0.0, 1.0, 2.0];
        assert_eq!(
            interpolate(0.5, &r, &out, Extrapolate::Clamp),
            AnimValue::Number(5.0)
        );
        assert_eq!(
            interpolate(1.5, &r, &out, Extrapolate::Clamp),
            AnimValue::Number(5.0)
        );
        assert_eq!(
            interpolate(3.0, &r, &out, Extrapolate::Clamp),
            AnimValue::Number(0.0)
        );
        assert_eq!(
            interpolate(3.0, &r, &out, Extrapolate::Extend),
            AnimValue::Number(-10.0)
        );
        assert_eq!(
            interpolate(-1.0, &r, &out, Extrapolate::Identity),
            AnimValue::Number(-1.0)
        );
        let colors = Output::Colors(vec![0x0000_00ff, 0xff00_00ff]);
        assert_eq!(
            interpolate(0.5, &[0.0, 1.0], &colors, Extrapolate::Clamp),
            AnimValue::Color(0x8000_00ff)
        );
    }

    #[test]
    fn derived_nodes_follow_animated_values() {
        let mut g = Graph::default();
        let mut finished = Vec::new();
        g.apply(
            AnimOp::CreateValue {
                node: 1,
                value: 0.0,
            },
            &mut finished,
        );
        g.apply(
            AnimOp::CreateInterpolate {
                node: 2,
                input: 1,
                input_range: vec![0.0, 100.0],
                output: Output::Numbers(vec![1.0, 0.0]),
                extrapolate: Extrapolate::Clamp,
            },
            &mut finished,
        );
        g.apply(
            AnimOp::CreateMath {
                node: 3,
                op: MathOp::Mul,
                a: Operand::Node(1),
                b: Operand::Const(-1.0),
            },
            &mut finished,
        );
        g.apply(
            AnimOp::Start {
                node: 1,
                animation: 7,
                driver: Driver::Timing {
                    to: 100.0,
                    duration_ms: 100.0,
                    easing: Easing::Linear,
                    delay_ms: 0.0,
                },
            },
            &mut finished,
        );
        assert!(g.is_animating());
        g.tick(0.0, &mut finished);
        g.tick(0.05, &mut finished);
        assert!((g.eval(2).number() - 0.5).abs() < 1e-4);
        assert!((g.eval(3).number() + 50.0).abs() < 1e-3);
        g.tick(0.2, &mut finished);
        assert_eq!(
            finished,
            vec![Finished {
                animation: 7,
                finished: true
            }]
        );
        assert!(!g.is_animating());
        assert_eq!(g.value(1), Some(100.0));
    }

    #[test]
    fn interrupting_reports_unfinished_and_keeps_velocity() {
        let mut g = Graph::default();
        let mut finished = Vec::new();
        g.apply(
            AnimOp::CreateValue {
                node: 1,
                value: 0.0,
            },
            &mut finished,
        );
        let spring = |to| Driver::Spring {
            to,
            stiffness: 100.0,
            damping: 10.0,
            mass: 1.0,
            velocity: None,
        };
        g.apply(
            AnimOp::Start {
                node: 1,
                animation: 1,
                driver: spring(100.0),
            },
            &mut finished,
        );
        g.tick(0.0, &mut finished);
        g.tick(0.1, &mut finished);
        g.apply(
            AnimOp::Start {
                node: 1,
                animation: 2,
                driver: spring(0.0),
            },
            &mut finished,
        );
        assert_eq!(
            finished,
            vec![Finished {
                animation: 1,
                finished: false
            }]
        );
        // A gesture takes over: stops the running animation.
        g.set(1, 42.0, &mut finished);
        assert_eq!(
            finished.last(),
            Some(&Finished {
                animation: 2,
                finished: false
            })
        );
        assert_eq!(g.value(1), Some(42.0));
        assert!(!g.is_animating());
    }

    #[test]
    fn cycles_do_not_overflow() {
        let mut g = Graph::default();
        let mut f = Vec::new();
        g.apply(
            AnimOp::CreateMath {
                node: 1,
                op: MathOp::Add,
                a: Operand::Node(1),
                b: Operand::Const(1.0),
            },
            &mut f,
        );
        let _ = g.eval(1);
    }
}
