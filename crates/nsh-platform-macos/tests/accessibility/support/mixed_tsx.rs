//! Disposable mixed-TSX fixture. No NSH ancestor access or mutation.
use super::*;

pub(super) struct MixedShared {
    run: String,
    case: String,
    phase: String,
    submitted: u64,
    applied: u64,
    context: Option<ComponentContext>,
    next: u64,
    controls: HashMap<u64, Value>,
}
impl MixedShared {
    pub(super) fn new(run: &str, case: &str) -> Self {
        Self {
            run: run.into(),
            case: case.into(),
            phase: "initial".into(),
            submitted: 0,
            applied: 0,
            context: None,
            next: 0,
            controls: HashMap::new(),
        }
    }
    pub(super) fn set_phase(&mut self, phase: &str) {
        self.phase = phase.into();
    }
    pub(super) fn diagnostic(&self) -> Value {
        json!({"phase":self.phase,"submitted":self.submitted,"applied":self.applied,"controls":self.controls})
    }
}
struct MixedTargetState {
    context: ComponentContext,
    shared: Rc<RefCell<MixedShared>>,
    events: Events,
    token: u64,
}
define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = MixedTargetState]
    #[name = "MixedTSXActionTarget"]
    struct MixedTarget;
    unsafe impl NSObjectProtocol for MixedTarget {}
    impl MixedTarget {
        #[unsafe(method(invoke:))]
        fn invoke(&self, sender: &NSButton) {
            let state = self.ivars();
            let mut shared = state.shared.borrow_mut();
            let phase = shared.phase.clone();
            let control = shared.controls.get_mut(&state.token).expect("live native action");
            let sequence = control["sequence"].as_u64().unwrap();
            let payload = json!({"id":control["id"],"appearance":"native","mount":state.token,
                "phase":phase,"sequence":sequence});
            // Actual entry is observable BEFORE native enabled admission.
            state.events.borrow_mut().push(json!({"kind":"target_entry","payload":payload,"system_ticks":system_ticks()}));
            if !sender.isEnabled() { return; }
            control["sequence"] = json!(sequence+1);
            let mut payload = payload; payload["sequence"] = json!(sequence+1);
            state.events.borrow_mut().push(json!({"kind":"native_press","payload":payload,"system_ticks":system_ticks()}));
            drop(shared);
            state.context.emit("press",payload);
        }
    }
);
struct MixedAction {
    button: Retained<NSButton>,
    _target: Retained<MixedTarget>,
    shared: Rc<RefCell<MixedShared>>,
    events: Events,
    token: u64,
}
impl NativeComponent for MixedAction {
    fn view(&self) -> Retained<NSView> {
        self.button.clone().into_super().into_super()
    }
    fn set_prop(&mut self, name: &str, value: Option<&Value>) {
        match name {
            "id" => self.button.setAccessibilityIdentifier(
                value
                    .and_then(Value::as_str)
                    .map(NSString::from_str)
                    .as_deref(),
            ),
            "title" => {
                let title = NSString::from_str(value.and_then(Value::as_str).unwrap_or(""));
                self.button.setTitle(&title);
                self.button.setAccessibilityLabel(Some(&title));
            }
            "enabled" => self
                .button
                .setEnabled(value.and_then(Value::as_bool).unwrap_or(false)),
            _ => return,
        }
        let mut shared = self.shared.borrow_mut();
        let row = shared.controls.get_mut(&self.token).unwrap();
        row[name] = value.cloned().unwrap_or(Value::Null);
        self.events.borrow_mut().push(
            json!({"kind":"native_prop","mount":self.token,"name":name,"value":value,
            "system_ticks":system_ticks()}),
        );
    }
    fn command(&mut self, name: &str, payload: &Value) {
        assert!(
            name == "bind_ref" && payload["id"].is_string() && payload["ref"].as_u64().is_some(),
            "fixture native ref binding command"
        );
        // This receipt binds a public component ref to its own factory instance.
        // It never inspects NSH nodes, ancestors or public AX representations.
        let mut shared = self.shared.borrow_mut();
        let row = shared
            .controls
            .get_mut(&self.token)
            .expect("owned binding instance");
        row["binding"] = payload.clone();
        self.events.borrow_mut().push(
            json!({"kind":"ref_bound","payload":payload,"mount":self.token,
            "system_ticks":system_ticks()}),
        );
    }
}
impl Drop for MixedAction {
    fn drop(&mut self) {
        self.button.setEnabled(false);
        unsafe {
            self.button.setTarget(None);
            self.button.setAction(None);
        }
        let control = self.shared.borrow_mut().controls.remove(&self.token);
        self.events.borrow_mut().push(json!({"kind":"native_drop","mount":self.token,"control":control,"system_ticks":system_ticks()}));
    }
}
struct Bridge {
    view: Retained<NSView>,
    shared: Rc<RefCell<MixedShared>>,
    events: Events,
}
impl NativeComponent for Bridge {
    fn view(&self) -> Retained<NSView> {
        self.view.clone()
    }
    fn set_prop(&mut self, _: &str, _: Option<&Value>) {}
    fn command(&mut self, name: &str, payload: &Value) {
        if name == "mode_receipt" {
            let mut state = self.shared.borrow_mut();
            assert!(
                payload["run"] == state.run
                    && payload["case"] == state.case
                    && payload["transition"].as_u64() == Some(state.applied + 1)
                    && state.submitted == state.applied + 1,
                "bound JS mode receipt"
            );
            state.applied += 1;
            state.phase = payload["phase"].as_str().expect("phase").into();
        }
        self.events
            .borrow_mut()
            .push(json!({"kind":name,"payload":payload,"system_ticks":system_ticks()}));
    }
}
pub(super) fn register(
    components: &Components,
    mtm: MainThreadMarker,
    events: &Events,
    shared: &Rc<RefCell<MixedShared>>,
) {
    let (shared_action, events_action) = (shared.clone(), events.clone());
    components.register(
        "mixed.action",
        Rc::new(move |context| {
            let token = {
                let mut state = shared_action.borrow_mut();
                state.next += 1;
                state.next
            };
            let button = NSButton::new(mtm);
            button.setButtonType(NSButtonType::MomentaryPushIn);
            button.setBezelStyle(NSBezelStyle::Push);
            button.setTitle(&NSString::from_str("Pending native title"));
            button.sizeToFit();
            let target = MixedTarget::alloc(mtm).set_ivars(MixedTargetState {
                context: context.clone(),
                shared: shared_action.clone(),
                events: events_action.clone(),
                token,
            });
            let target: Retained<MixedTarget> = unsafe { msg_send![super(target), init] };
            unsafe {
                button.setTarget(Some(&target));
                button.setAction(Some(sel!(invoke:)));
            }
            shared_action.borrow_mut().controls.insert(
                token,
                json!({"id":null,"sequence":0,
            "mount":token,"object":Retained::as_ptr(&button) as usize}),
            );
            events_action
                .borrow_mut()
                .push(json!({"kind":"native_mount","mount":token,"system_ticks":system_ticks()}));
            Box::new(MixedAction {
                button,
                _target: target,
                shared: shared_action.clone(),
                events: events_action.clone(),
                token,
            })
        }),
    );
    let (shared, events) = (shared.clone(), events.clone());
    components.register(
        "mixed.bridge",
        Rc::new(move |context| {
            shared.borrow_mut().context = Some(context);
            Box::new(Bridge {
                view: NSView::new(mtm),
                shared: shared.clone(),
                events: events.clone(),
            })
        }),
    );
}
pub(super) fn poll_control(
    shared: &Rc<RefCell<MixedShared>>,
    directory: &std::path::Path,
    events: &Events,
) {
    let Some(payload) = std::fs::read(directory.join("transition.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    else {
        return;
    };
    let context = {
        let mut state = shared.borrow_mut();
        if payload["run"] != state.run
            || payload["case"] != state.case
            || payload["transition"].as_u64() != Some(state.submitted + 1)
        {
            return;
        }
        let Some(context) = state.context.clone() else {
            return;
        };
        state.submitted += 1;
        context
    };
    events
        .borrow_mut()
        .push(json!({"kind":"control_emit","payload":payload,"system_ticks":system_ticks()}));
    context.emit("control", payload);
}
