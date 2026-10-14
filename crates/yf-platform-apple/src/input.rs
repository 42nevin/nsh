//! Text inputs: a `UITextField` (one line) or `UITextView` (`multiline`)
//! inside a container view that carries the decoration (background,
//! border, corners, shadow) like any other view.

use std::cell::Cell;

use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, ProtocolObject, Sel},
    sel,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSObject, NSRange, NSString,
};
use objc2_ui_kit::{
    NSFontAttributeName, NSForegroundColorAttributeName, NSTextAlignment, UIColor, UIControl,
    UIControlEvents, UIEdgeInsets, UIKeyboardType, UILabel, UIResponder, UIReturnKeyType,
    UITextAutocapitalizationType, UITextAutocorrectionType, UITextField, UITextView, UIView,
    UIViewAutoresizing,
};
use yf_core::{
    AutoCapitalize, Color, InputConfig, KeyboardType, NodeId, Rect, ReturnKeyType, TextAlign,
};

use crate::{target::EventTarget, text};

/// Limits shared by both input classes, read synchronously by delegate
/// callbacks (which cannot wait for the mounter).
pub(crate) struct InputIvars {
    insets: Cell<UIEdgeInsets>,
    /// Maximum length in UTF-16 units.
    max_length: Cell<Option<u32>>,
}

impl Default for InputIvars {
    fn default() -> Self {
        Self {
            insets: Cell::new(UIEdgeInsets {
                top: 0.0,
                left: 0.0,
                bottom: 0.0,
                right: 0.0,
            }),
            max_length: Cell::new(None),
        }
    }
}

define_class!(
    /// A one-line field whose text sits inside the node's padding.
    #[unsafe(super(UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = InputIvars]
    pub(crate) struct Field;

    impl Field {
        #[unsafe(method(textRectForBounds:))]
        fn text_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }

        #[unsafe(method(editingRectForBounds:))]
        fn editing_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }

        #[unsafe(method(placeholderRectForBounds:))]
        fn placeholder_rect(&self, bounds: CGRect) -> CGRect {
            inset(bounds, self.ivars().insets.get())
        }
    }
);

define_class!(
    /// A multi-line text area.
    #[unsafe(super(UITextView, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = InputIvars]
    pub(crate) struct Area;
);

impl Field {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(InputIvars::default());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

impl Area {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(InputIvars::default());
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

fn inset(r: CGRect, i: UIEdgeInsets) -> CGRect {
    CGRect::new(
        CGPoint::new(r.origin.x + i.left, r.origin.y + i.top),
        CGSize::new(
            (r.size.width - i.left - i.right).max(0.0),
            (r.size.height - i.top - i.bottom).max(0.0),
        ),
    )
}

/// True if replacing `range` of `current` with `replacement` keeps the
/// text within `max` UTF-16 units (pasting may still be cut by the platform).
pub(crate) fn fits(
    current: &NSString,
    range: NSRange,
    replacement: &NSString,
    max: Option<u32>,
) -> bool {
    let Some(max) = max else { return true };
    let len = current.length() - range.length.min(current.length()) + replacement.length();
    len <= max as usize || replacement.length() == 0
}

/// Reads the limits of the input behind a delegate callback.
pub(crate) fn max_length(view: &UIView) -> Option<u32> {
    if let Some(field) = view.downcast_ref::<Field>() {
        return field.ivars().max_length.get();
    }
    view.downcast_ref::<Area>()
        .and_then(|area| area.ivars().max_length.get())
}

enum Editor {
    Line(Retained<Field>),
    Area {
        view: Retained<Area>,
        /// UITextView has no placeholder of its own.
        placeholder: Retained<UILabel>,
    },
}

/// The native side of an `<input>`.
pub(crate) struct Input {
    container: Retained<UIView>,
    editor: Editor,
    id: NodeId,
    /// Edits reported to the engine.
    pub(crate) count: u32,
    config: Option<InputConfig>,
    content: Rect,
}

impl Input {
    pub(crate) fn new(mtm: MainThreadMarker, id: NodeId, target: &EventTarget) -> Self {
        let container = UIView::new(mtm);
        container.setTag(id as isize);
        let editor = Editor::Line(Field::new(mtm));
        let input = Self {
            container,
            editor,
            id,
            count: 0,
            config: None,
            content: Rect::default(),
        };
        input.attach(target);
        input
    }

    pub(crate) fn view(&self) -> &UIView {
        &self.container
    }

    /// The view that edits (first responder).
    pub(crate) fn editor_view(&self) -> &UIView {
        match &self.editor {
            Editor::Line(f) => f,
            Editor::Area { view, .. } => view,
        }
    }

    pub(crate) fn text(&self) -> String {
        match &self.editor {
            Editor::Line(f) => f.text().map(|t| t.to_string()).unwrap_or_default(),
            Editor::Area { view, .. } => view.text().to_string(),
        }
    }

    fn attach(&self, target: &EventTarget) {
        let view = self.editor_view();
        view.setTag(self.id as isize);
        view.setFrame(self.container.bounds());
        view.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        view.setBackgroundColor(None);
        match &self.editor {
            Editor::Line(field) => {
                // SAFETY: the target implements the delegate protocol and
                // `handleEditingChanged:`, and outlives the field (both are
                // owned by the mounter).
                unsafe {
                    field.setDelegate(Some(ProtocolObject::from_ref(target)));
                    field.addTarget_action_forControlEvents(
                        Some(target),
                        sel!(handleEditingChanged:),
                        UIControlEvents::EditingChanged,
                    );
                }
            }
            Editor::Area { view, placeholder } => {
                // SAFETY: as above.
                unsafe { view.setDelegate(Some(ProtocolObject::from_ref(target))) };
                view.textContainer().setLineFragmentPadding(0.0);
                view.addSubview(placeholder);
            }
        }
        self.container.addSubview(view);
    }

    /// Applies a configuration; switching `multiline` swaps the editor.
    pub(crate) fn configure(
        &mut self,
        config: InputConfig,
        target: &EventTarget,
        mtm: MainThreadMarker,
    ) {
        let multiline = matches!(self.editor, Editor::Area { .. });
        if config.multiline != multiline {
            let text = self.text();
            let focused = self.editor_view().isFirstResponder();
            self.editor_view().removeFromSuperview();
            self.editor = if config.multiline {
                Editor::Area {
                    view: Area::new(mtm),
                    placeholder: UILabel::new(mtm),
                }
            } else {
                Editor::Line(Field::new(mtm))
            };
            self.attach(target);
            self.set_text(&text);
            self.set_content(self.content);
            if focused {
                self.editor_view().becomeFirstResponder();
            }
        }
        let font = text::ui_font(&config.style);
        let color = ui_color(config.style.color);
        let align = match config.align {
            TextAlign::Auto | TextAlign::Left | TextAlign::Justify => NSTextAlignment::Left,
            TextAlign::Center => NSTextAlignment::Center,
            TextAlign::Right => NSTextAlignment::Right,
        };
        let tint = config.selection_color.map(ui_color);
        match &self.editor {
            Editor::Line(field) => {
                field.setFont(Some(&font));
                field.setTextColor(Some(&color));
                field.setTextAlignment(align);
                field.setEnabled(config.editable);
                // SAFETY: a plain setter; `None` restores the default.
                unsafe { field.setTintColor(tint.as_deref()) };
                field.setAttributedPlaceholder(Some(&placeholder(&config, &font)));
                traits(field.as_ref(), &config);
                field.ivars().max_length.set(config.max_length);
            }
            Editor::Area { view, placeholder } => {
                view.setFont(Some(&font));
                view.setTextColor(Some(&color));
                view.setTextAlignment(align);
                view.setEditable(config.editable);
                // SAFETY: plain setters (`None` restores the default tint).
                unsafe {
                    view.setTintColor(tint.as_deref());
                    placeholder.setFont(Some(&font));
                    placeholder.setTextColor(Some(
                        &config
                            .placeholder_color
                            .map(ui_color)
                            .unwrap_or_else(UIColor::placeholderTextColor),
                    ));
                }
                placeholder.setTextAlignment(align);
                placeholder.setNumberOfLines(0);
                placeholder.setText(Some(&NSString::from_str(&config.placeholder)));
                traits(view.as_ref(), &config);
                view.ivars().max_length.set(config.max_length);
            }
        }
        let first = self.config.is_none();
        let auto_focus = config.auto_focus;
        self.config = Some(config);
        self.update_placeholder();
        if first && auto_focus {
            self.editor_view().becomeFirstResponder();
        }
    }

    /// Sets the text from JS unless the user typed since JS saw `count`.
    pub(crate) fn set_value(&mut self, text: &str, count: u32) {
        if count != self.count || self.text() == text {
            return;
        }
        self.set_text(text);
    }

    fn set_text(&self, text: &str) {
        let text = NSString::from_str(text);
        match &self.editor {
            Editor::Line(field) => field.setText(Some(&text)),
            Editor::Area { view, .. } => view.setText(Some(&text)),
        }
        self.update_placeholder();
    }

    /// Where the text goes, relative to the container.
    pub(crate) fn set_content(&mut self, content: Rect) {
        self.content = content;
        let bounds = self.container.bounds().size;
        let insets = UIEdgeInsets {
            top: content.y as CGFloat,
            left: content.x as CGFloat,
            bottom: (bounds.height - (content.y + content.height) as CGFloat).max(0.0),
            right: (bounds.width - (content.x + content.width) as CGFloat).max(0.0),
        };
        match &self.editor {
            Editor::Line(field) => {
                field.ivars().insets.set(insets);
                field.setNeedsLayout();
            }
            Editor::Area { view, placeholder } => {
                view.setTextContainerInset(insets);
                placeholder.setFrame(CGRect::new(
                    CGPoint::new(content.x as CGFloat, content.y as CGFloat),
                    CGSize::new(content.width as CGFloat, content.height as CGFloat),
                ));
                placeholder.sizeToFit();
                let size = placeholder.frame().size;
                placeholder.setFrame(CGRect::new(
                    CGPoint::new(content.x as CGFloat, content.y as CGFloat),
                    CGSize::new(content.width as CGFloat, size.height),
                ));
            }
        }
    }

    /// Shows the text area's placeholder while it is empty.
    pub(crate) fn update_placeholder(&self) {
        if let Editor::Area { view, placeholder } = &self.editor {
            placeholder.setHidden(view.text().length() > 0);
        }
    }

    /// Whether the return key ends editing (single line) or adds a line.
    pub(crate) fn multiline(&self) -> bool {
        matches!(self.editor, Editor::Area { .. })
    }

    /// Whether return unfocuses the input (`blurOnSubmit`).
    pub(crate) fn blur_on_submit(&self) -> bool {
        self.config.as_ref().is_none_or(|c| c.blur_on_submit)
    }
}

/// Sends a one-argument setter without objc2's debug-build signature check:
/// UIKit implements the `UITextInputTraits` setters of text fields by
/// message forwarding, so the check cannot find them on the class.
///
/// # Safety
///
/// `view` must respond to `sel`, which takes one argument of type `A`.
unsafe fn send<A: Copy>(view: &AnyObject, sel: Sel, value: A) {
    // SAFETY: `objc_msgSend` called with the selector's real signature
    // (guaranteed by the caller).
    unsafe {
        let send: unsafe extern "C-unwind" fn(&AnyObject, Sel, A) =
            std::mem::transmute(objc2::ffi::objc_msgSend as unsafe extern "C-unwind" fn());
        send(view, sel, value);
    }
}

fn traits(view: &AnyObject, config: &InputConfig) {
    let keyboard = match config.keyboard {
        KeyboardType::Default => UIKeyboardType::Default,
        KeyboardType::Number => UIKeyboardType::NumberPad,
        KeyboardType::Decimal => UIKeyboardType::DecimalPad,
        KeyboardType::Email => UIKeyboardType::EmailAddress,
        KeyboardType::Phone => UIKeyboardType::PhonePad,
        KeyboardType::Url => UIKeyboardType::URL,
    };
    let return_key = match config.return_key {
        ReturnKeyType::Default => UIReturnKeyType::Default,
        ReturnKeyType::Done => UIReturnKeyType::Done,
        ReturnKeyType::Go => UIReturnKeyType::Go,
        ReturnKeyType::Next => UIReturnKeyType::Next,
        ReturnKeyType::Search => UIReturnKeyType::Search,
        ReturnKeyType::Send => UIReturnKeyType::Send,
    };
    let capitalize = match config.auto_capitalize {
        AutoCapitalize::None => UITextAutocapitalizationType::None,
        AutoCapitalize::Sentences => UITextAutocapitalizationType::Sentences,
        AutoCapitalize::Words => UITextAutocapitalizationType::Words,
        AutoCapitalize::Characters => UITextAutocapitalizationType::AllCharacters,
    };
    let correct = if config.auto_correct {
        UITextAutocorrectionType::Default
    } else {
        UITextAutocorrectionType::No
    };
    // SAFETY: UITextField and UITextView adopt UITextInputTraits; each
    // setter takes the NSInteger-backed enum (or BOOL) passed here.
    unsafe {
        send(view, sel!(setSecureTextEntry:), Bool::new(config.secure));
        send(view, sel!(setKeyboardType:), keyboard.0);
        send(view, sel!(setReturnKeyType:), return_key.0);
        send(view, sel!(setAutocapitalizationType:), capitalize.0);
        send(view, sel!(setAutocorrectionType:), correct.0);
    }
}

fn placeholder(config: &InputConfig, font: &objc2_ui_kit::UIFont) -> Retained<NSAttributedString> {
    let color = config
        .placeholder_color
        .map(ui_color)
        .unwrap_or_else(UIColor::placeholderTextColor);
    // SAFETY: immutable UIKit attribute keys with values of their documented
    // types (UIFont, UIColor).
    let keys: [&NSAttributedStringKey; 2] =
        unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&objc2::runtime::AnyObject; 2] = [font.as_ref(), color.as_ref()];
    let attributes = NSDictionary::from_slices(&keys, &values);
    // SAFETY: a string and a dictionary of valid attributes.
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(&config.placeholder),
            Some(&attributes),
        )
    }
}

fn ui_color(color: Color) -> Retained<UIColor> {
    let (r, g, b, a) = color.to_f64();
    UIColor::colorWithRed_green_blue_alpha(r, g, b, a)
}
