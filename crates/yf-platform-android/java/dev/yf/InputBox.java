package dev.yf;

import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.drawable.ColorDrawable;
import android.os.Build;
import android.text.Editable;
import android.text.InputFilter;
import android.text.InputType;
import android.text.TextWatcher;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;
import android.widget.FrameLayout;

/** `<input>`: an EditText inside a container that carries the decoration. */
@SuppressLint("ViewConstructor")
final class InputBox extends FrameLayout implements Node {
    final Mounter mounter;
    final int id;
    final Decor decor = new Decor();
    final EditText edit;
    /** Edits reported to the engine. */
    int count;
    private boolean multiline;
    /** Return unfocuses a single-line input. */
    private boolean blurOnSubmit = true;
    private boolean applying;
    private boolean configured;

    InputBox(Context context, Mounter mounter, int id) {
        super(context);
        this.mounter = mounter;
        this.id = id;
        setBackground(decor);
        edit = new EditText(context) {
            @Override
            public boolean onKeyUp(int code, android.view.KeyEvent event) {
                // The up of an Enter whose down submitted another field (focus
                // moved meanwhile) must not advance focus again.
                if (code == android.view.KeyEvent.KEYCODE_ENTER && !multiline) return true;
                return super.onKeyUp(code, event);
            }
        };
        edit.setBackground(new ColorDrawable(0));
        edit.setPadding(0, 0, 0, 0);
        edit.setIncludeFontPadding(false);
        edit.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence s, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence s, int start, int before, int count) {}

            @Override
            public void afterTextChanged(Editable s) {
                if (applying) return;
                InputBox.this.count++;
                mounter.event(id, Native.CHANGE_TEXT, s.toString(), InputBox.this.count, 0, 0);
            }
        });
        edit.setOnEditorActionListener((v, action, event) -> {
            if (multiline && event != null) return false;
            // A hardware Enter reports its down and its up: act once. (The up
            // would otherwise reach the next field, focused meanwhile.)
            if (event != null && event.getAction() != android.view.KeyEvent.ACTION_DOWN) return true;
            mounter.event(id, Native.SUBMIT_EDITING, edit.getText().toString(), count, 0, 0);
            if (!multiline && blurOnSubmit) blur();
            return true;
        });
        edit.setOnFocusChangeListener((v, focused) -> {
            mounter.inputFocused(this, focused);
            if (mounter.wants(id, focused ? Native.FOCUS : Native.BLUR)) {
                mounter.event(id, focused ? Native.FOCUS : Native.BLUR);
            }
        });
        addView(edit, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
    }

    void configure(float fontSizeDp, int weight, boolean italic, int color, String family, float letterSpacingDp,
                   int align, String placeholder, boolean hasPlaceholderColor, int placeholderColor,
                   boolean secure, int keyboard, int returnKey, int capitalize, boolean autoCorrect,
                   boolean multiline, int maxLength, boolean editable, boolean autoFocus,
                   boolean hasSelectionColor, int selectionColor, boolean blurOnSubmit) {
        this.multiline = multiline;
        this.blurOnSubmit = blurOnSubmit;
        edit.setTextSize(TypedValue.COMPLEX_UNIT_PX, fontSizeDp * mounter.density);
        edit.setTextColor(color);
        if (letterSpacingDp != 0) edit.setLetterSpacing(letterSpacingDp / fontSizeDp);
        int horizontal = align == 1 ? Gravity.CENTER_HORIZONTAL : align == 2 ? Gravity.END : Gravity.START;
        edit.setGravity(horizontal | (multiline ? Gravity.TOP : Gravity.CENTER_VERTICAL));
        edit.setHint(placeholder);
        if (hasPlaceholderColor) edit.setHintTextColor(placeholderColor);
        int type;
        switch (keyboard) {
            case 1: type = InputType.TYPE_CLASS_NUMBER; break;
            case 2: type = InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL; break;
            case 3: type = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS; break;
            case 4: type = InputType.TYPE_CLASS_PHONE; break;
            case 5: type = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI; break;
            default: type = InputType.TYPE_CLASS_TEXT;
        }
        if ((type & InputType.TYPE_MASK_CLASS) == InputType.TYPE_CLASS_TEXT) {
            switch (capitalize) {
                case 1: type |= InputType.TYPE_TEXT_FLAG_CAP_SENTENCES; break;
                case 2: type |= InputType.TYPE_TEXT_FLAG_CAP_WORDS; break;
                case 3: type |= InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS; break;
                default: break;
            }
            type |= autoCorrect ? InputType.TYPE_TEXT_FLAG_AUTO_CORRECT : InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
            if (multiline) type |= InputType.TYPE_TEXT_FLAG_MULTI_LINE;
            if (secure) type = (type & ~InputType.TYPE_MASK_VARIATION) | InputType.TYPE_TEXT_VARIATION_PASSWORD;
        } else if (secure && (type & InputType.TYPE_MASK_CLASS) == InputType.TYPE_CLASS_NUMBER) {
            type |= InputType.TYPE_NUMBER_VARIATION_PASSWORD;
        }
        int selection = edit.getSelectionEnd();
        edit.setInputType(type);
        // After the input type: password types reset the typeface to monospace.
        edit.setTypeface(TextLayouts.typeface(family, weight, italic));
        edit.setSingleLine(!multiline);
        if (multiline) edit.setMaxLines(Integer.MAX_VALUE);
        if (selection >= 0 && selection <= edit.length()) edit.setSelection(selection);
        int ime;
        switch (returnKey) {
            case 1: ime = EditorInfo.IME_ACTION_DONE; break;
            case 2: ime = EditorInfo.IME_ACTION_GO; break;
            case 3: ime = EditorInfo.IME_ACTION_NEXT; break;
            case 4: ime = EditorInfo.IME_ACTION_SEARCH; break;
            case 5: ime = EditorInfo.IME_ACTION_SEND; break;
            default: ime = multiline ? EditorInfo.IME_ACTION_NONE : EditorInfo.IME_ACTION_DONE;
        }
        edit.setImeOptions(ime);
        edit.setFilters(maxLength >= 0 ? new InputFilter[] {new InputFilter.LengthFilter(maxLength)} : new InputFilter[0]);
        edit.setEnabled(editable);
        if (hasSelectionColor && Build.VERSION.SDK_INT >= 29) {
            edit.setTextCursorDrawable(new ColorDrawable(selectionColor) {
                @Override
                public int getIntrinsicWidth() {
                    return Math.round(2 * mounter.density);
                }
            });
            edit.setHighlightColor((selectionColor & 0x00ffffff) | 0x55000000);
        } else if (!hasSelectionColor) {
            edit.setHighlightColor(0x5533b5e5);
        }
        boolean first = !configured;
        configured = true;
        if (first && autoFocus) post(this::focus);
    }

    /** Sets the text from JS unless the user typed since JS saw `count`. */
    void setValue(String text, int count) {
        if (count != this.count || text.contentEquals(edit.getText())) return;
        applying = true;
        int selection = edit.getSelectionEnd();
        edit.setText(text);
        edit.setSelection(Math.min(Math.max(selection, 0), text.length()));
        applying = false;
    }

    void setContent(int left, int top, int right, int bottom) {
        // Padding of the container: the EditText fills the content box.
        setPadding(left, top, right, bottom);
    }

    void focus() {
        edit.requestFocus();
        InputMethodManager imm = (InputMethodManager) getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        imm.showSoftInput(edit, InputMethodManager.SHOW_IMPLICIT);
    }

    void blur() {
        if (!edit.hasFocus()) return;
        hideKeyboard();
        // To the (focusable) root: clearFocus() would hand focus to the first
        // focusable view, often another field.
        mounter.host.requestFocus();
    }

    private void hideKeyboard() {
        InputMethodManager imm = (InputMethodManager) getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        imm.hideSoftInputFromWindow(edit.getWindowToken(), 0);
    }

    @Override
    public int nodeId() {
        return id;
    }
}
