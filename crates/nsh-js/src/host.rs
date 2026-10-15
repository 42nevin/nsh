use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use nsh_core::{EventKind, EventPayload, NodeId, Op, PayloadValue, TouchPhase};
use rquickjs::{
    Array, CatchResultExt, Context, Ctx, Function, Module, Persistent, Runtime, Value,
    function::Opt,
};

use crate::{
    console,
    env::{ColorScheme, Environment, Idiom},
    error::JsError,
    net::{HttpResponse, NetworkQueue, RequestId},
    protocol::{self, Warnings},
    sandbox::{self, Budget, Limits, Phase},
    timers::Timers,
    web,
};

/// Receives uncaught JS exceptions.
pub type ErrorHandler = Box<dyn Fn(&JsError)>;

/// State shared between the host and the native functions installed in JS.
/// Only touched on the JS thread.
#[derive(Default)]
pub(crate) struct Shared {
    ops: RefCell<Vec<Op>>,
    timers: RefCell<Timers>,
    warnings: RefCell<Warnings>,
    pub(crate) net: RefCell<NetworkQueue>,
    /// Font assets imported by the app, not yet registered.
    fonts: RefCell<Vec<String>>,
    /// `requestAnimationFrame` callbacks waiting for the next frame.
    frames: RefCell<Vec<(u32, Persistent<Function<'static>>)>>,
    next_frame_id: std::cell::Cell<u32>,
    /// Measures one line of text for `measureText` (the engine's text
    /// engine).
    measurer: RefCell<Option<TextMeasurer>>,
    /// The VM's limits.
    pub(crate) limits: Limits,
    /// Why the VM was stopped (out of memory, over a quota): no JS runs
    /// after that.
    pub(crate) fatal: RefCell<Option<String>>,
}

impl Shared {
    /// Stops the VM for `reason` (the first reason wins).
    pub(crate) fn stop(&self, reason: String) {
        self.fatal.borrow_mut().get_or_insert(reason);
    }
}

/// Measures the width of one line of `text` in `font`, in points.
pub type TextMeasurer = Box<dyn Fn(&nsh_core::canvas::CanvasFont, &str) -> f32>;

/// One QuickJS runtime + context with the nsh host APIs installed.
pub struct Host {
    // Field order matters: the context must drop before the runtime.
    ctx: Context,
    rt: Runtime,
    shared: Rc<Shared>,
    on_error: Option<ErrorHandler>,
    budget: Budget,
}

/// How a VM is set up.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostOptions {
    /// Resource limits.
    pub limits: Limits,
    /// Removes `eval` and the function constructors and freezes the shared
    /// prototypes (see `sandbox`). Off for development bundles.
    pub harden: bool,
}

impl Host {
    /// Creates a runtime with the default limits, not hardened, and installs
    /// `console`, timers and the op bridge.
    pub fn new() -> Result<Host, JsError> {
        Self::with_options(HostOptions::default())
    }

    /// Creates a runtime set up by `options` and installs `console`, timers
    /// and the op bridge.
    pub fn with_options(options: HostOptions) -> Result<Host, JsError> {
        let limits = options.limits;
        let rt = Runtime::new()?;
        // The engine thread is spawned with a larger stack; keep a margin.
        rt.set_max_stack_size(limits.stack_bytes);
        if limits.memory_bytes > 0 {
            rt.set_memory_limit(limits.memory_bytes);
        }
        let budget = Budget::new(limits);
        rt.set_interrupt_handler(Some(budget.interrupt_handler(None)));
        let ctx = Context::full(&rt)?;
        let shared = Rc::new(Shared {
            limits,
            ..Shared::default()
        });
        ctx.with(|ctx| {
            install(&ctx, &shared)?;
            if options.harden {
                sandbox::harden(&ctx)?;
            }
            Ok(())
        })
        .map_err(|e| ctx.with(|ctx| JsError::from_engine(&ctx, e)))?;
        Ok(Host {
            ctx,
            rt,
            shared,
            on_error: None,
            budget,
        })
    }

    /// Aborts running JS (with an uncatchable exception) once `stop` is
    /// set: an engine shutting down never waits for a long-running script.
    pub fn set_stop_flag(&self, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.rt
            .set_interrupt_handler(Some(self.budget.interrupt_handler(Some(stop))));
    }

    /// Why the VM was stopped, if it was: it ran out of memory, or the app
    /// went over a quota. No JS runs after that; the views stay as they are.
    pub fn fatal(&self) -> Option<String> {
        self.shared.fatal.borrow().clone()
    }

    /// Stops the VM (e.g. the core refused an op over a quota).
    pub fn stop(&self, reason: String) {
        self.shared.stop(reason);
    }

    /// The VM's limits.
    pub fn limits(&self) -> &Limits {
        self.budget.limits()
    }

    /// Runs `f` (a call into JS) within the budget of `phase`; `None` once
    /// the VM was stopped.
    fn guarded<R>(&self, phase: Phase, f: impl FnOnce() -> R) -> Option<R> {
        if self.shared.fatal.borrow().is_some() {
            return None;
        }
        let started = self.budget.begin(phase);
        let result = f();
        self.budget.end(started);
        Some(result)
    }

    /// What an error from a call means: an overrun budget or exhausted
    /// memory get a message saying so (exhausted memory also stops the VM).
    fn classify(&self, mut err: JsError) -> JsError {
        if let Some(trip) = self.budget.take_trip() {
            err.message = format!("{trip}\n{}", err.message);
        } else if err.message.contains("out of memory") {
            let reason = format!(
                "the script ran out of memory (limit {} MB) and was stopped",
                self.budget.limits().memory_bytes >> 20
            );
            self.shared.stop(reason.clone());
            err.message = format!("{reason}\n{}", err.message);
        }
        err
    }

    /// The error for a call refused because the VM was stopped.
    fn stopped(&self) -> JsError {
        JsError {
            message: self.fatal().unwrap_or_default(),
        }
    }

    /// Sets what canvas `measureText` measures with.
    pub fn set_text_measurer(&mut self, measurer: TextMeasurer) {
        *self.shared.measurer.borrow_mut() = Some(measurer);
    }

    /// Sets the handler for uncaught exceptions (in event handlers, timers and
    /// promise jobs). Without a handler they are logged.
    pub fn set_error_handler(&mut self, handler: ErrorHandler) {
        self.on_error = Some(handler);
    }

    /// Evaluates a bundle given as source.
    pub fn eval_source(&self, source: &str) -> Result<(), JsError> {
        self.guarded(Phase::Load, || {
            self.ctx
                .with(|ctx| -> Result<(), JsError> {
                    let module = Module::declare(ctx.clone(), crate::BUNDLE_NAME, source)
                        .catch(&ctx)
                        .map_err(JsError::caught)?;
                    let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
                    promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
                })
                .map_err(|e| self.classify(e))?;
            self.run_jobs();
            Ok(())
        })
        .unwrap_or_else(|| Err(self.stopped()))
    }

    /// Evaluates a classic script named `name` (for stack traces) and
    /// returns whether its completion value is `true`: development updates
    /// report whether they could be applied.
    pub fn eval_script(&self, name: &str, source: &str) -> Result<bool, JsError> {
        self.guarded(Phase::Load, || self.eval_script_unguarded(name, source))
            .unwrap_or_else(|| Err(self.stopped()))
    }

    fn eval_script_unguarded(&self, name: &str, source: &str) -> Result<bool, JsError> {
        let applied = self.ctx.with(|ctx| -> Result<bool, JsError> {
            let mut options = rquickjs::context::EvalOptions::default();
            options.global = true;
            options.strict = false;
            options.filename = Some(name.to_owned());
            let value: rquickjs::Value<'_> = ctx
                .eval_with_options(source, options)
                .catch(&ctx)
                .map_err(JsError::caught)?;
            Ok(value.as_bool() == Some(true))
        });
        let applied = applied.map_err(|e| self.classify(e))?;
        self.run_jobs();
        Ok(applied)
    }

    /// Evaluates a bundle compiled with [`crate::compile_bytecode`].
    pub fn eval_bytecode(&self, bytecode: &[u8]) -> Result<(), JsError> {
        self.guarded(Phase::Load, || {
            self.ctx
                .with(|ctx| -> Result<(), JsError> {
                    // SAFETY: bytecode comes from `compile_bytecode` with the same
                    // QuickJS build; QuickJS does not validate arbitrary input.
                    let module = unsafe { Module::load(ctx.clone(), bytecode) }
                        .catch(&ctx)
                        .map_err(JsError::caught)?;
                    let (_, promise) = module.eval().catch(&ctx).map_err(JsError::caught)?;
                    promise.finish::<()>().catch(&ctx).map_err(JsError::caught)
                })
                .map_err(|e| self.classify(e))?;
            self.run_jobs();
            Ok(())
        })
        .unwrap_or_else(|| Err(self.stopped()))
    }

    /// Delivers a native event (and its data) to the node's JS handler.
    pub fn dispatch_event(&self, id: NodeId, event: EventKind, payload: &EventPayload) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let payload = match (event, payload) {
                    // `onChangeText` receives the text itself, as in React Native.
                    (EventKind::ChangeText, EventPayload::Text { text, .. }) => {
                        rquickjs::String::from_str(ctx.clone(), text).map(|s| s.into_value())
                    }
                    (_, EventPayload::Value(value)) => payload_value(&ctx, value),
                    _ => payload_object(&ctx, payload),
                };
                let result = payload.and_then(|payload| {
                    ctx.globals()
                        .get::<_, Function>("__nsh_dispatch")?
                        .call::<_, ()>((id, event.prop_name(), payload))
                });
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// Asks the runtime's `List` to render items into cells:
    /// `(slot, item index)` pairs.
    pub fn bind_list(&self, list: NodeId, binds: &[(u32, u32)]) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let result = (|| {
                    let flat = rquickjs::Array::new(ctx.clone())?;
                    for (i, &(slot, index)) in binds.iter().enumerate() {
                        flat.set(i * 2, slot)?;
                        flat.set(i * 2 + 1, index)?;
                    }
                    ctx.globals()
                        .get::<_, Function>("__nsh_list")?
                        .call::<_, ()>((list, flat))
                })();
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// Resolves the promises of finished (or interrupted) native animations.
    pub fn animations_finished(&self, finished: &[nsh_anim::Finished]) {
        self.guarded(Phase::Call, || {
            if finished.is_empty() {
                return;
            }
            self.ctx.with(|ctx| {
                let result = (|| {
                    let callback = ctx.globals().get::<_, Function>("__nsh_anim_end")?;
                    for f in finished {
                        callback.call::<_, ()>((f.animation, f.finished))?;
                    }
                    Ok(())
                })();
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
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
            // Each callback, with the microtasks it queues, has a budget of its own.
            let ran = self.guarded(Phase::Call, || {
                self.ctx.with(|ctx| {
                    let result = callback.restore(&ctx).and_then(|f| f.call::<_, ()>(()));
                    self.report(&ctx, result);
                });
                self.run_jobs();
            });
            if ran.is_none() {
                break;
            }
        }
    }

    /// Runs pending promise jobs and microtasks until the queue is empty.
    pub fn run_jobs(&self) {
        self.guarded(Phase::Call, || {
            while self.rt.is_job_pending() {
                if let Err(err) = self.rt.execute_pending_job() {
                    let err = err.0.with(|ctx| JsError {
                        message: describe(ctx.catch()),
                    });
                    self.uncaught(&err);
                }
            }
        });
    }

    /// Moves the network requests and cancellations made since the last
    /// call into `out`.
    pub fn take_network(&self, out: &mut NetworkQueue) {
        let mut net = self.shared.net.borrow_mut();
        out.requests.append(&mut net.requests);
        out.cancels.append(&mut net.cancels);
    }

    /// Publishes the surface's environment to JS. Call once before
    /// evaluating the bundle (the runtime reads the initial value), then on
    /// every change.
    pub fn set_environment(&self, env: &Environment) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let result = (|| {
                    let value = environment_object(&ctx, env)?;
                    let globals = ctx.globals();
                    globals.set("__nsh_env_value", value.clone())?;
                    if let Ok(update) = globals.get::<_, Function>("__nsh_env") {
                        update.call::<_, ()>((value,))?;
                    }
                    Ok(())
                })();
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// True if JS asked for a display frame (`requestAnimationFrame`).
    pub fn wants_frame(&self) -> bool {
        !self.shared.frames.borrow().is_empty()
    }

    /// Runs the `requestAnimationFrame` callbacks queued so far, with the
    /// current `performance.now()`. Callbacks requested meanwhile wait for
    /// the next frame.
    pub fn run_frame(&self) {
        self.guarded(Phase::Call, || {
            let callbacks = std::mem::take(&mut *self.shared.frames.borrow_mut());
            if callbacks.is_empty() {
                return;
            }
            self.ctx.with(|ctx| {
                let now = (|| -> rquickjs::Result<f64> {
                    let performance: rquickjs::Object = ctx.globals().get("performance")?;
                    performance.get::<_, Function>("now")?.call(())
                })()
                .unwrap_or(0.0);
                for (_, callback) in callbacks {
                    let result = callback.restore(&ctx).and_then(|f| f.call::<_, ()>((now,)));
                    self.report(&ctx, result);
                }
            });
            self.run_jobs();
        });
    }

    /// Moves the font assets (`asset:/…` URIs) imported since the last call
    /// into `out`. They must be registered before the next layout.
    pub fn take_fonts(&self, out: &mut Vec<String>) {
        out.append(&mut self.shared.fonts.borrow_mut());
    }

    /// Settles the `fetch` promise of request `id`.
    pub fn finish_fetch(&self, id: RequestId, result: Result<HttpResponse, String>) {
        self.guarded(Phase::Call, || {
            self.ctx.with(|ctx| {
                let result = web::finish_fetch(&ctx, id, result);
                self.report(&ctx, result);
            });
            self.run_jobs();
        });
    }

    /// Runs the cycle collector. Call when idle so released nodes are
    /// reported promptly.
    pub fn collect_garbage(&self) {
        self.guarded(Phase::Call, || {
            self.rt.run_gc();
            self.run_jobs();
        });
    }
}

impl Host {
    fn report<T>(&self, ctx: &Ctx<'_>, result: rquickjs::Result<T>) {
        if let Err(err) = result.catch(ctx) {
            self.uncaught(&JsError::caught(err));
        }
    }

    fn uncaught(&self, err: &JsError) {
        let err = self.classify(err.clone());
        match &self.on_error {
            Some(handler) => handler(&err),
            None => log::error!(target: "js", "uncaught exception: {err}"),
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // Persistent JS values must be freed before the runtime.
        self.shared.timers.borrow_mut().clear();
        self.shared.frames.borrow_mut().clear();
    }
}

fn environment_object<'js>(ctx: &Ctx<'js>, env: &Environment) -> rquickjs::Result<Value<'js>> {
    let object = |pairs: &[(&str, f32)]| -> rquickjs::Result<rquickjs::Object<'js>> {
        let o = rquickjs::Object::new(ctx.clone())?;
        for (k, v) in pairs {
            o.set(*k, *v)?;
        }
        Ok(o)
    };
    let obj = rquickjs::Object::new(ctx.clone())?;
    obj.set(
        "window",
        object(&[("width", env.window.0), ("height", env.window.1)])?,
    )?;
    let a = env.safe_area;
    obj.set(
        "safeArea",
        object(&[
            ("top", a.top),
            ("right", a.right),
            ("bottom", a.bottom),
            ("left", a.left),
        ])?,
    )?;
    obj.set(
        "keyboard",
        object(&[
            ("height", env.keyboard_height),
            ("duration", env.keyboard_duration),
        ])?,
    )?;
    obj.set(
        "colorScheme",
        match env.color_scheme {
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
        },
    )?;
    obj.set("fontScale", env.font_scale)?;
    obj.set("rtl", env.rtl)?;
    obj.set("reduceMotion", env.reduce_motion)?;
    obj.set("pixelRatio", env.pixel_ratio)?;
    obj.set("screenReader", env.screen_reader)?;
    obj.set("os", env.os.name())?;
    // React Native: a string on iOS ("17.5"), the API level on Android.
    let (major, minor) = env.os_version;
    if env.os == crate::Os::Android {
        obj.set("osVersion", major)?;
    } else {
        obj.set("osVersion", format!("{major}.{minor}"))?;
    }
    obj.set(
        "idiom",
        match env.idiom {
            Idiom::Phone => "phone",
            Idiom::Pad => "pad",
            Idiom::Mac => "mac",
            Idiom::Tv => "tv",
            Idiom::Desktop => "desktop",
        },
    )?;
    Ok(obj.into_value())
}

/// The JS object handed to event handlers.
fn payload_object<'js>(ctx: &Ctx<'js>, payload: &EventPayload) -> rquickjs::Result<Value<'js>> {
    let obj = rquickjs::Object::new(ctx.clone())?;
    match payload {
        EventPayload::None => return Ok(Value::new_undefined(ctx.clone())),
        EventPayload::Value(value) => return payload_value(ctx, value),
        EventPayload::Map(fields) => {
            for (key, value) in fields {
                obj.set(*key, payload_value(ctx, value)?)?;
            }
        }
        EventPayload::Native { name, data } => {
            obj.set("name", &**name)?;
            obj.set("data", json_value(ctx, data)?)?;
        }
        EventPayload::TextLayout(lines) => {
            let list = rquickjs::Array::new(ctx.clone())?;
            for (i, l) in lines.iter().enumerate() {
                let line = rquickjs::Object::new(ctx.clone())?;
                line.set("text", l.text.as_str())?;
                line.set("x", l.x)?;
                line.set("y", l.y)?;
                line.set("width", l.width)?;
                line.set("height", l.height)?;
                line.set("ascender", l.ascender)?;
                line.set("descender", l.descender)?;
                line.set("capHeight", l.cap_height)?;
                line.set("xHeight", l.x_height)?;
                list.set(i, line)?;
            }
            obj.set("lines", list)?;
        }
        EventPayload::Layout {
            x,
            y,
            width,
            height,
        } => {
            obj.set("x", *x)?;
            obj.set("y", *y)?;
            obj.set("width", *width)?;
            obj.set("height", *height)?;
        }
        EventPayload::Pan {
            translation_x,
            translation_y,
            velocity_x,
            velocity_y,
            value_x,
            value_y,
        } => {
            obj.set("translationX", *translation_x)?;
            obj.set("translationY", *translation_y)?;
            obj.set("velocityX", *velocity_x)?;
            obj.set("velocityY", *velocity_y)?;
            if let Some(v) = value_x {
                obj.set("valueX", *v)?;
            }
            if let Some(v) = value_y {
                obj.set("valueY", *v)?;
            }
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
        EventPayload::Text { text, .. } => obj.set("text", text.as_str())?,
        EventPayload::Touch(touch) => {
            obj.set(
                "phase",
                match touch.phase {
                    TouchPhase::Start => "start",
                    TouchPhase::Move => "move",
                    TouchPhase::End => "end",
                    TouchPhase::Cancel => "cancel",
                },
            )?;
            obj.set("timestamp", touch.timestamp)?;
            let points = rquickjs::Array::new(ctx.clone())?;
            for (i, p) in touch.points.iter().enumerate() {
                let o = rquickjs::Object::new(ctx.clone())?;
                o.set("identifier", p.id)?;
                o.set("target", p.target)?;
                let path = rquickjs::Array::new(ctx.clone())?;
                for (j, id) in p.path.iter().enumerate() {
                    path.set(j, *id)?;
                }
                o.set("path", path)?;
                o.set("pageX", p.page_x)?;
                o.set("pageY", p.page_y)?;
                o.set("locationX", p.location_x)?;
                o.set("locationY", p.location_y)?;
                o.set("changed", p.changed)?;
                o.set("timestamp", touch.timestamp)?;
                points.set(i, o)?;
            }
            obj.set("points", points)?;
        }
    }
    Ok(obj.into_value())
}

/// JSON (a native component's event data) as a JS value.
fn json_value<'js>(ctx: &Ctx<'js>, value: &nsh_core::JsonValue) -> rquickjs::Result<Value<'js>> {
    use nsh_core::JsonValue as J;
    Ok(match value {
        J::Null => Value::new_null(ctx.clone()),
        J::Bool(b) => Value::new_bool(ctx.clone(), *b),
        J::Number(n) => Value::new_float(ctx.clone(), n.as_f64().unwrap_or(0.0)),
        J::String(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
        J::Array(items) => {
            let array = rquickjs::Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() {
                array.set(i, json_value(ctx, item)?)?;
            }
            array.into_value()
        }
        J::Object(fields) => {
            let object = rquickjs::Object::new(ctx.clone())?;
            for (key, item) in fields {
                object.set(key.as_str(), json_value(ctx, item)?)?;
            }
            object.into_value()
        }
    })
}

fn payload_value<'js>(ctx: &Ctx<'js>, value: &PayloadValue) -> rquickjs::Result<Value<'js>> {
    Ok(match value {
        PayloadValue::Number(n) => Value::new_float(ctx.clone(), *n),
        PayloadValue::Bool(b) => Value::new_bool(ctx.clone(), *b),
        PayloadValue::Str(s) => rquickjs::String::from_str(ctx.clone(), s)?.into_value(),
        PayloadValue::Null => Value::new_null(ctx.clone()),
    })
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
    web::install(ctx, shared)?;

    let s = shared.clone();
    globals.set(
        "__nsh_commit",
        Function::new(ctx.clone(), move |buf: Array<'_>| -> rquickjs::Result<()> {
            // Decoding allocates in Rust, outside the heap limit.
            if buf.len() > s.limits.max_op_values {
                let reason = format!(
                    "the app committed {} values at once (limit {}) and was stopped",
                    buf.len(),
                    s.limits.max_op_values
                );
                s.stop(reason.clone());
                return Err(rquickjs::Exception::throw_range(buf.ctx(), &reason));
            }
            let mut ops = s.ops.borrow_mut();
            protocol::decode(buf.ctx(), &buf, &mut ops, &mut s.warnings.borrow_mut(), &s)
        })?,
    )?;

    // Canvas `measureText`: the width of one line, measured by the text
    // engine (0 without one, e.g. in tests).
    let s = shared.clone();
    globals.set(
        "__nsh_measure_text",
        Function::new(ctx.clone(), move |font: String, text: String| -> f64 {
            let font = nsh_core::canvas::CanvasFont::parse(&font).unwrap_or_default();
            s.measurer
                .borrow()
                .as_ref()
                .map_or(0.0, |m| m(&font, &text) as f64)
        })?,
    )?;

    // `import Inter from "./Inter.ttf"` compiles to `__nsh_font(uri, family)`.
    let s = shared.clone();
    globals.set(
        "__nsh_font",
        Function::new(ctx.clone(), move |uri: String, family: String| {
            s.fonts.borrow_mut().push(uri);
            family
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
    let s = shared.clone();
    globals.set(
        "requestAnimationFrame",
        Function::new(ctx.clone(), move |callback: Function<'_>| -> u32 {
            let id = s.next_frame_id.get().wrapping_add(1).max(1);
            s.next_frame_id.set(id);
            let callback = Persistent::save(&callback.ctx().clone(), callback);
            s.frames.borrow_mut().push((id, callback));
            id
        })?,
    )?;
    let s = shared.clone();
    globals.set(
        "cancelAnimationFrame",
        Function::new(ctx.clone(), move |id: Opt<u32>| {
            if let Some(id) = id.0 {
                s.frames.borrow_mut().retain(|(i, _)| *i != id);
            }
        })?,
    )?;
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

    use nsh_core::{EventKind, Op, Tag};

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
            r##"__nsh_commit([
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
        let Op::SetStyle { id: 2, style, .. } = &ops[1] else {
            panic!("{:?}", ops[1])
        };
        assert_eq!(format!("{:?}", style.layout.flex_direction), "Row");
        assert_eq!(style.view.background_color, nsh_core::Color(0xff00_00ff));
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
    fn decodes_shadow_and_corner_styles() {
        let host = Host::new().unwrap();
        host.eval_source(
            r##"__nsh_commit([
                3, 2, "style", {
                    shadowColor: "#000", shadowOffset: { width: 2, height: 6 },
                    shadowOpacity: 0.25, shadowRadius: 10,
                    borderRadius: 12, borderBottomLeftRadius: 0,
                },
                3, 3, "style", { shadowOffset: null },
            ]);"##,
        )
        .unwrap();
        let ops = ops(&host);
        let Op::SetStyle { style, .. } = &ops[0] else {
            panic!("{:?}", ops[0])
        };
        let shadow = style.view.shadow;
        assert_eq!((shadow.offset_x, shadow.offset_y), (2.0, 6.0));
        assert_eq!((shadow.opacity, shadow.radius), (0.25, 10.0));
        assert_eq!(style.view.radii().to_array(), [12.0, 12.0, 12.0, 0.0]);
        let Op::SetStyle { style, .. } = &ops[1] else {
            panic!("{:?}", ops[1])
        };
        assert_eq!(style.view.shadow, nsh_core::Shadow::DEFAULT);
    }

    #[test]
    fn bad_buffer_throws_in_js() {
        let host = Host::new().unwrap();
        let err = host.eval_source(r#"__nsh_commit([42]);"#).unwrap_err();
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
        host.eval_source(r#"__nsh_commit([1, 9, log.join(",")]);"#)
            .unwrap();
        let ops = ops(&host);
        assert!(
            matches!(&ops[0], Op::CreateText { text, .. } if text == "micro,a,b"),
            "{ops:?}"
        );
    }

    fn sandboxed(limits: Limits) -> Host {
        Host::with_options(HostOptions {
            limits,
            harden: true,
        })
        .unwrap()
    }

    #[test]
    fn calls_over_budget_are_interrupted_and_the_vm_goes_on() {
        let mut host = sandboxed(Limits {
            call_budget: Duration::from_millis(50),
            ..Limits::default()
        });
        let errors = Rc::new(RefCell::new(Vec::new()));
        let seen = errors.clone();
        host.set_error_handler(Box::new(move |e| seen.borrow_mut().push(e.message.clone())));
        host.eval_source(
            r#"
            globalThis.__nsh_dispatch = (id) => {
                // try/catch cannot swallow the interrupt.
                try { if (id === 1) for (;;) {} } catch (e) {}
                __nsh_commit([0, id + 10, "view"]);
            };
            "#,
        )
        .unwrap();
        let start = Instant::now();
        host.dispatch_event(1, EventKind::Press, &EventPayload::None);
        assert!(start.elapsed() < Duration::from_secs(2));
        let errors = errors.borrow().clone();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].contains("budget of 50 ms"), "{}", errors[0]);
        assert!(ops(&host).is_empty());
        // The next call runs normally.
        host.dispatch_event(2, EventKind::Press, &EventPayload::None);
        assert_eq!(ops(&host).len(), 1);
        assert!(host.fatal().is_none());
    }

    #[test]
    fn running_out_of_memory_stops_the_vm() {
        let host = sandboxed(Limits {
            memory_bytes: 16 << 20,
            ..Limits::default()
        });
        let err = host
            .eval_source("const keep = []; for (;;) keep.push(new Array(100000).fill(1));")
            .unwrap_err();
        assert!(
            err.message.contains("ran out of memory (limit 16 MB)"),
            "{}",
            err.message
        );
        assert!(host.fatal().is_some());
        // Nothing runs any more.
        assert!(host.eval_source("__nsh_commit([0, 2, 'view']);").is_err());
        assert!(ops(&host).is_empty());
    }

    #[test]
    fn huge_op_buffers_stop_the_vm() {
        let host = sandboxed(Limits {
            max_op_values: 1000,
            ..Limits::default()
        });
        assert!(
            host.eval_source("__nsh_commit(new Array(2000).fill(0));")
                .is_err()
        );
        assert!(host.fatal().unwrap().contains("limit 1000"));
    }

    #[test]
    fn hardened_vms_run_no_code_from_strings() {
        let host = sandboxed(Limits::default());
        host.eval_source(
            r#"
            const results = [];
            const attempt = (f) => { try { f(); results.push("ran"); } catch (e) { results.push(e.constructor.name); } };
            attempt(() => eval("1"));
            attempt(() => new Function("return 1")());
            attempt(() => (function () {}).constructor("return 1")());
            attempt(() => (async function () {}).constructor("return 1"));
            attempt(() => (function* () {}).constructor("yield 1"));
            attempt(() => { "use strict"; Array.prototype.map = null; });
            attempt(() => { "use strict"; Object.prototype.polluted = 1; });
            // Ordinary code is unaffected.
            class MyError extends Error { constructor() { super("x"); this.name = "MyError"; } }
            attempt(() => { const e = new MyError(); if (e.name !== "MyError") throw new Error(); });
            attempt(() => [1, 2].map((x) => x * 2));
            __nsh_commit([1, 1, results.join(",")]);
            "#,
        )
        .unwrap();
        let ops = ops(&host);
        let Op::CreateText { text, .. } = &ops[0] else {
            panic!("{ops:?}")
        };
        assert_eq!(
            text,
            "ReferenceError,TypeError,TypeError,TypeError,TypeError,TypeError,TypeError,ran,ran"
        );
    }

    #[test]
    fn development_vms_keep_eval() {
        let host = Host::new().unwrap();
        host.eval_source(r#"__nsh_commit([1, 1, String(eval("1 + 1"))]);"#)
            .unwrap();
        assert!(matches!(&ops(&host)[0], Op::CreateText { text, .. } if text == "2"));
    }
}
