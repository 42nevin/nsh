use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use rquickjs::{
    Array, CatchResultExt, Context, Ctx, Function, Module, Persistent, Runtime, Value,
    function::Opt,
};
use yf_core::{EventKind, EventPayload, NodeId, Op};

use crate::{
    console,
    error::JsError,
    protocol::{self, Warnings},
    timers::Timers,
};

/// Receives uncaught JS exceptions.
pub type ErrorHandler = Box<dyn Fn(&JsError)>;

/// State shared between the host and the native functions installed in JS.
/// Only touched on the JS thread.
#[derive(Default)]
struct Shared {
    ops: RefCell<Vec<Op>>,
    timers: RefCell<Timers>,
    warnings: RefCell<Warnings>,
}

/// One QuickJS runtime + context with the yf host APIs installed.
pub struct Host {
    // Field order matters: the context must drop before the runtime.
    ctx: Context,
    rt: Runtime,
    shared: Rc<Shared>,
    on_error: Option<ErrorHandler>,
}

impl Host {
    /// Creates a runtime and installs `console`, timers and the op bridge.
    pub fn new() -> Result<Host, JsError> {
        let rt = Runtime::new()?;
        // The engine thread is spawned with a larger stack; keep a margin.
        rt.set_max_stack_size(4 * 1024 * 1024);
        let ctx = Context::full(&rt)?;
        let shared = Rc::new(Shared::default());
        ctx.with(|ctx| install(&ctx, &shared).map_err(|e| JsError::from_engine(&ctx, e)))?;
        Ok(Host {
            ctx,
            rt,
            shared,
            on_error: None,
        })
    }

    /// Sets the handler for uncaught exceptions (in event handlers, timers and
    /// promise jobs). Without a handler they are logged.
    pub fn set_error_handler(&mut self, handler: ErrorHandler) {
        self.on_error = Some(handler);
    }

    /// Evaluates a bundle given as source.
    pub fn eval_source(&self, source: &str) -> Result<(), JsError> {
        self.ctx.with(|ctx| -> Result<(), JsError> {
            let module = Module::declare(ctx.clone(), crate::BUNDLE_NAME, source)
                .catch(&ctx)
                .map_err(JsError::caught)?;
            let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
            promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
        })?;
        self.run_jobs();
        Ok(())
    }

    /// Evaluates a bundle compiled with [`crate::compile_bytecode`].
    pub fn eval_bytecode(&self, bytecode: &[u8]) -> Result<(), JsError> {
        self.ctx.with(|ctx| -> Result<(), JsError> {
            // SAFETY: bytecode comes from `compile_bytecode` with the same
            // QuickJS build; QuickJS does not validate arbitrary input.
            let module = unsafe { Module::load(ctx.clone(), bytecode) }
                .catch(&ctx)
                .map_err(JsError::caught)?;
            let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
            promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
        })?;
        self.run_jobs();
        Ok(())
    }

    /// Delivers a native event (and its data) to the node's JS handler.
    pub fn dispatch_event(&self, id: NodeId, event: EventKind, payload: &EventPayload) {
        self.ctx.with(|ctx| {
            let result = payload_object(&ctx, payload).and_then(|payload| {
                ctx.globals()
                    .get::<_, Function>("__yf_dispatch")?
                    .call::<_, ()>((id, event.prop_name(), payload))
            });
            self.report(&ctx, result);
        });
        self.run_jobs();
    }

    /// Asks the runtime's `List` to render items into cells:
    /// `(slot, item index)` pairs.
    pub fn bind_list(&self, list: NodeId, binds: &[(u32, u32)]) {
        self.ctx.with(|ctx| {
            let result = (|| {
                let flat = rquickjs::Array::new(ctx.clone())?;
                for (i, &(slot, index)) in binds.iter().enumerate() {
                    flat.set(i * 2, slot)?;
                    flat.set(i * 2 + 1, index)?;
                }
                ctx.globals()
                    .get::<_, Function>("__yf_list")?
                    .call::<_, ()>((list, flat))
            })();
            self.report(&ctx, result);
        });
        self.run_jobs();
    }

    /// Resolves the promises of finished (or interrupted) native animations.
    pub fn animations_finished(&self, finished: &[yf_anim::Finished]) {
        if finished.is_empty() {
            return;
        }
        self.ctx.with(|ctx| {
            let result = (|| {
                let callback = ctx.globals().get::<_, Function>("__yf_anim_end")?;
                for f in finished {
                    callback.call::<_, ()>((f.animation, f.finished))?;
                }
                Ok(())
            })();
            self.report(&ctx, result);
        });
        self.run_jobs();
    }

    /// Moves the ops committed by JS since the last call into `out`.
    pub fn take_ops(&self, out: &mut Vec<Op>) {
        let mut ops = self.shared.ops.borrow_mut();
        if out.is_empty() {
            std::mem::swap(out, &mut ops);
        } else {
            out.append(&mut ops);
        }
    }

    /// Earliest timer deadline, if any timer is pending.
    pub fn next_timer(&self) -> Option<Instant> {
        self.shared.timers.borrow_mut().next_deadline()
    }

    /// Runs all timers due at `now`.
    pub fn run_timers(&self, now: Instant) {
        loop {
            // Release the borrow before calling into JS: callbacks may add timers.
            let Some(callback) = self.shared.timers.borrow_mut().pop_due(now) else {
                break;
            };
            self.ctx.with(|ctx| {
                let result = callback.restore(&ctx).and_then(|f| f.call::<_, ()>(()));
                self.report(&ctx, result);
            });
            self.run_jobs();
        }
    }

    /// Runs pending promise jobs and microtasks until the queue is empty.
    pub fn run_jobs(&self) {
        while self.rt.is_job_pending() {
            if let Err(err) = self.rt.execute_pending_job() {
                let err = err.0.with(|ctx| JsError {
                    message: describe(ctx.catch()),
                });
                self.uncaught(&err);
            }
        }
    }

    /// Runs the cycle collector. Call when idle so released nodes are
    /// reported promptly.
    pub fn collect_garbage(&self) {
        self.rt.run_gc();
        self.run_jobs();
    }
}

impl Host {
    fn report<T>(&self, ctx: &Ctx<'_>, result: rquickjs::Result<T>) {
        if let Err(err) = result.catch(ctx) {
            self.uncaught(&JsError::caught(err));
        }
    }

    fn uncaught(&self, err: &JsError) {
        match &self.on_error {
            Some(handler) => handler(err),
            None => log::error!(target: "js", "uncaught exception: {err}"),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Persistent JS values must be freed before the runtime.
        self.shared.timers.borrow_mut().clear();
    }
}

/// The JS object handed to event handlers.
fn payload_object<'js>(ctx: &Ctx<'js>, payload: &EventPayload) -> rquickjs::Result<Value<'js>> {
    let obj = rquickjs::Object::new(ctx.clone())?;
    match payload {
        EventPayload::None => return Ok(Value::new_undefined(ctx.clone())),
        EventPayload::Pan {
            translation_x,
            translation_y,
            velocity_x,
            velocity_y,
        } => {
            obj.set("translationX", *translation_x)?;
            obj.set("translationY", *translation_y)?;
            obj.set("velocityX", *velocity_x)?;
            obj.set("velocityY", *velocity_y)?;
        }
        EventPayload::Scroll { x, y } => {
            obj.set("x", *x)?;
            obj.set("y", *y)?;
        }
        EventPayload::Image { width, height } => {
            obj.set("width", *width)?;
            obj.set("height", *height)?;
        }
        EventPayload::Error { message } => obj.set("message", message.as_str())?,
    }
    Ok(obj.into_value())
}

fn describe(value: Value<'_>) -> String {
    match value.as_exception() {
        Some(e) => e.to_string(),
        None => format!("{value:?}"),
    }
}

fn install(ctx: &Ctx<'_>, shared: &Rc<Shared>) -> rquickjs::Result<()> {
    let globals = ctx.globals();
    console::install(ctx)?;

    let s = shared.clone();
    globals.set(
        "__yf_commit",
        Function::new(ctx.clone(), move |buf: Array<'_>| -> rquickjs::Result<()> {
            let mut ops = s.ops.borrow_mut();
            protocol::decode(buf.ctx(), &buf, &mut ops, &mut s.warnings.borrow_mut())
        })?,
    )?;

    for (name, repeat) in [("setTimeout", false), ("setInterval", true)] {
        let s = shared.clone();
        globals.set(
            name,
            Function::new(
                ctx.clone(),
                move |callback: Function<'_>, delay: Opt<f64>| -> u32 {
                    let ms = delay.0.unwrap_or(0.0);
                    let delay = Duration::from_secs_f64(if ms.is_finite() && ms > 0.0 {
                        ms / 1000.0
                    } else {
                        0.0
                    });
                    let callback = Persistent::save(&callback.ctx().clone(), callback);
                    s.timers.borrow_mut().add(callback, delay, repeat)
                },
            )?,
        )?;
    }
    for name in ["clearTimeout", "clearInterval"] {
        let s = shared.clone();
        globals.set(
            name,
            Function::new(ctx.clone(), move |id: Opt<u32>| {
                if let Some(id) = id.0 {
                    s.timers.borrow_mut().cancel(id);
                }
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use yf_core::{EventKind, Op, Tag};

    use super::*;

    fn ops(host: &Host) -> Vec<Op> {
        let mut out = Vec::new();
        host.take_ops(&mut out);
        out
    }

    #[test]
    fn decodes_op_buffer() {
        let host = Host::new().unwrap();
        host.eval_source(
            r##"__yf_commit([
                0, 2, "view",
                3, 2, "style", { flexDirection: "row", padding: 8, backgroundColor: "#ff0000", bogus: 1 },
                3, 2, "onPress", true,
                1, 3, "hi",
                4, 2, 3, 0,
                4, 1, 2, 0,
                2, 3, "bye",
                5, 2, 3,
                6, 3,
            ]);"##)
        .unwrap();
        let ops = ops(&host);
        assert_eq!(ops.len(), 9);
        assert!(matches!(
            ops[0],
            Op::Create {
                id: 2,
                tag: Tag::View
            }
        ));
        let Op::SetStyle { id: 2, style } = &ops[1] else {
            panic!("{:?}", ops[1])
        };
        assert_eq!(format!("{:?}", style.layout.flex_direction), "Row");
        assert_eq!(style.view.background_color, yf_core::Color(0xff00_00ff));
        assert!(matches!(
            ops[2],
            Op::Listen {
                id: 2,
                event: EventKind::Press,
                enabled: true
            }
        ));
        assert!(matches!(&ops[3], Op::CreateText { id: 3, text } if text == "hi"));
        assert!(matches!(
            ops[4],
            Op::Insert {
                parent: 2,
                child: 3,
                before: None
            }
        ));
        assert!(matches!(&ops[6], Op::SetText { id: 3, text } if text == "bye"));
        assert!(matches!(
            ops[7],
            Op::Remove {
                parent: 2,
                child: 3
            }
        ));
        assert!(matches!(ops[8], Op::Release { id: 3 }));
    }

    #[test]
    fn bad_buffer_throws_in_js() {
        let host = Host::new().unwrap();
        let err = host.eval_source(r#"__yf_commit([42]);"#).unwrap_err();
        assert!(err.message.contains("unknown opcode 42"), "{err}");
    }

    #[test]
    fn timers_fire_in_order_and_cancel() {
        let host = Host::new().unwrap();
        host.eval_source(
            r#"
            globalThis.log = [];
            setTimeout(() => log.push("b"), 20);
            setTimeout(() => log.push("a"), 0);
            const c = setTimeout(() => log.push("cancelled"), 10);
            clearTimeout(c);
            Promise.resolve().then(() => log.push("micro"));
            "#,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_millis(30);
        while let Some(next) = host.next_timer() {
            std::thread::sleep(next.saturating_duration_since(Instant::now()));
            host.run_timers(Instant::now());
            assert!(Instant::now() < deadline + Duration::from_secs(1));
        }
        host.eval_source(r#"__yf_commit([1, 9, log.join(",")]);"#)
            .unwrap();
        let ops = ops(&host);
        assert!(
            matches!(&ops[0], Op::CreateText { text, .. } if text == "micro,a,b"),
            "{ops:?}"
        );
    }
}
