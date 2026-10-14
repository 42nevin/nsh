//! UIKit controls: `UISwitch`, `UISlider` and `UIActivityIndicatorView`.

use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained, sel};
use objc2_core_foundation::CGSize;
use objc2_ui_kit::{
    UIActivityIndicatorView, UIActivityIndicatorViewStyle, UIBehavioralStyle, UIColor,
    UIControlEvents, UISlider, UISwitch, UISwitchStyle, UIView,
};
use yf_core::{ActivityProps, Color, ControlSizes, Size, SliderProps, SwitchProps};

use crate::target::EventTarget;

fn ui_color(c: Color) -> Retained<UIColor> {
    let (r, g, b, a) = c.to_f64();
    UIColor::colorWithRed_green_blue_alpha(r, g, b, a)
}

/// The natural sizes of this device's controls.
pub(crate) fn sizes(mtm: MainThreadMarker) -> ControlSizes {
    let size = |v: &UIView| {
        let s = v.sizeThatFits(CGSize::new(0.0, 0.0));
        Size::new(s.width as f32, s.height as f32)
    };
    let indicator = |style| {
        let v = UIActivityIndicatorView::initWithActivityIndicatorStyle(
            UIActivityIndicatorView::alloc(mtm),
            style,
        );
        size(&v)
    };
    let defaults = ControlSizes::default();
    let slider = UISlider::new(mtm);
    slider.setPreferredBehavioralStyle(UIBehavioralStyle::Pad);
    let slider = size(&slider).height;
    let switch = new_switch_view(mtm);
    ControlSizes {
        switch: size(&switch),
        activity_small: indicator(UIActivityIndicatorViewStyle::Medium),
        activity_large: indicator(UIActivityIndicatorViewStyle::Large),
        slider_height: if slider > 0.0 {
            slider
        } else {
            defaults.slider_height
        },
    }
}

/// A switch that slides (Catalyst's Mac idiom shows checkboxes otherwise).
fn new_switch_view(mtm: MainThreadMarker) -> Retained<UISwitch> {
    let s = UISwitch::new(mtm);
    s.setPreferredStyle(UISwitchStyle::Sliding);
    s
}

/// Catalyst's Mac idiom throws on colored switches.
fn mac_idiom(mtm: MainThreadMarker) -> bool {
    objc2_ui_kit::UIDevice::currentDevice(mtm).userInterfaceIdiom()
        == objc2_ui_kit::UIUserInterfaceIdiom::Mac
}

pub(crate) fn new_switch(mtm: MainThreadMarker, target: &EventTarget) -> Retained<UISwitch> {
    let s = new_switch_view(mtm);
    // SAFETY: `handleSwitch:` is implemented by the target, which outlives
    // the control.
    unsafe {
        s.addTarget_action_forControlEvents(
            Some(target),
            sel!(handleSwitch:),
            UIControlEvents::ValueChanged,
        )
    };
    s
}

pub(crate) fn configure_switch(s: &UISwitch, p: &SwitchProps) {
    if s.isOn() != p.value {
        s.setOn_animated(p.value, true);
    }
    s.setEnabled(!p.disabled);
    if mac_idiom(s.mtm()) {
        return;
    }
    s.setOnTintColor(p.track_on.map(ui_color).as_deref());
    s.setThumbTintColor(p.thumb.map(ui_color).as_deref());
    // The off track is the switch's background, rounded like the track.
    match p.track_off {
        Some(c) => {
            // SAFETY: a plain getter.
            let color = unsafe { ui_color(c).CGColor() };
            s.layer().setBackgroundColor(Some(&color));
            s.layer().setCornerRadius(s.bounds().size.height / 2.0);
        }
        None => s.layer().setBackgroundColor(None),
    }
}

pub(crate) fn new_slider(mtm: MainThreadMarker, target: &EventTarget) -> Retained<UISlider> {
    let s = UISlider::new(mtm);
    // The iOS look and behavior: Catalyst's Mac idiom cannot color controls.
    s.setPreferredBehavioralStyle(UIBehavioralStyle::Pad);
    // SAFETY: the selectors are implemented by the target, which outlives
    // the control.
    unsafe {
        s.addTarget_action_forControlEvents(
            Some(target),
            sel!(handleSlider:),
            UIControlEvents::ValueChanged,
        );
        s.addTarget_action_forControlEvents(
            Some(target),
            sel!(handleSliderEnd:),
            UIControlEvents::TouchUpInside
                | UIControlEvents::TouchUpOutside
                | UIControlEvents::TouchCancel,
        );
    }
    s
}

pub(crate) fn configure_slider(s: &UISlider, p: &SliderProps) {
    s.setMinimumValue(p.min);
    s.setMaximumValue(p.max);
    if !s.isTracking() {
        s.setValue(p.value);
    }
    s.setEnabled(!p.disabled);
    s.setMinimumTrackTintColor(p.min_track.map(ui_color).as_deref());
    s.setMaximumTrackTintColor(p.max_track.map(ui_color).as_deref());
    s.setThumbTintColor(p.thumb.map(ui_color).as_deref());
}

/// The slider's value snapped to `step` (it snaps under the finger too).
pub(crate) fn snapped(s: &UISlider, step: f32) -> f32 {
    let v = s.value();
    if step <= 0.0 {
        return v;
    }
    let min = s.minimumValue();
    let snapped = (min + ((v - min) / step).round() * step).clamp(min, s.maximumValue());
    if snapped != v {
        s.setValue(snapped);
    }
    snapped
}

pub(crate) fn new_activity(mtm: MainThreadMarker) -> Retained<UIActivityIndicatorView> {
    UIActivityIndicatorView::initWithActivityIndicatorStyle(
        UIActivityIndicatorView::alloc(mtm),
        UIActivityIndicatorViewStyle::Medium,
    )
}

pub(crate) fn configure_activity(a: &UIActivityIndicatorView, p: &ActivityProps) {
    a.setActivityIndicatorViewStyle(if p.large {
        UIActivityIndicatorViewStyle::Large
    } else {
        UIActivityIndicatorViewStyle::Medium
    });
    // SAFETY: a plain setter; `None` restores the default color.
    unsafe { a.setColor(p.color.map(ui_color).as_deref()) };
    a.setHidesWhenStopped(p.hides_when_stopped);
    if p.animating {
        a.startAnimating();
    } else {
        a.stopAnimating();
    }
}
