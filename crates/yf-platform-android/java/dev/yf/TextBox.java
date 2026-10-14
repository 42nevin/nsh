package dev.yf;

import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.Canvas;
import android.text.Layout;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;

/** Draws a StaticLayout made on the engine thread: no text layout here. */
@SuppressLint("ViewConstructor")
final class TextBox extends View implements Node {
    final Decor decor = new Decor();
    final int id;
    boolean clip;
    /** Accessibility from the app's properties, if any. */
    A11y a11y;
    private Layout layout;
    private float contentX, contentY, contentWidth, contentHeight;

    /** Press handling, once the view has press listeners. */
    Pressable press;

    TextBox(Context context, int id) {
        super(context);
        this.id = id;
        setBackground(decor);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
    }

    @Override
    public int nodeId() {
        return id;
    }

    @android.annotation.SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(android.view.MotionEvent e) {
        return press != null && press.onTouchEvent(e);
    }

    @Override
    public boolean performClick() {
        super.performClick();
        if (press != null) press.click();
        return true;
    }

    /** Screen readers read the text (unless a label replaces it). */
    @Override
    public void onInitializeAccessibilityNodeInfo(AccessibilityNodeInfo info) {
        super.onInitializeAccessibilityNodeInfo(info);
        if (layout != null && getContentDescription() == null) info.setText(layout.getText());
        if (info.getClassName() == null || info.getClassName().equals(View.class.getName())) {
            info.setClassName("android.widget.TextView");
        }
    }

    void setLayout(Layout layout, float x, float y, float width, float height) {
        this.layout = layout;
        this.contentX = x;
        this.contentY = y;
        this.contentWidth = width;
        this.contentHeight = height;
        invalidate();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (layout == null) return;
        canvas.save();
        canvas.translate(contentX, contentY);
        // Lines past `numberOfLines` (clip mode) stay outside the box.
        canvas.clipRect(0, 0, Math.max(contentWidth, layout.getWidth()), contentHeight);
        layout.draw(canvas);
        canvas.restore();
    }
}
