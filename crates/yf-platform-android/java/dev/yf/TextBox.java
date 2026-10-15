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

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        decor.applyPivot(this);
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

    /** Pressable spans: (start, end, node) triples in UTF-16 offsets, and their events. */
    private int[] links = new int[0];
    private long[] linkEvents = new long[0];

    void setLinks(int[] links, long[] events) {
        this.links = links;
        this.linkEvents = events;
    }

    boolean hasLinks() {
        return linkEvents.length > 0;
    }

    /**
     * The pressable span under ({@code x}, {@code y}) (view pixels): its
     * index, or -1.
     */
    int linkAt(float x, float y) {
        if (layout == null || linkEvents.length == 0) return -1;
        float lx = x - contentX, ly = y - contentY;
        float slack = 4 * getResources().getDisplayMetrics().density;
        if (ly < -slack || ly > layout.getHeight() + slack) return -1;
        int line = layout.getLineForVertical((int) ly);
        if (lx < layout.getLineLeft(line) - slack || lx > layout.getLineRight(line) + slack) return -1;
        int offset = layout.getOffsetForHorizontal(line, lx);
        // The nearest caret: the character is the one before it when the
        // point is left of the caret.
        if (offset > layout.getLineStart(line) && lx < layout.getPrimaryHorizontal(offset)) offset--;
        int best = -1;
        for (int i = 0; i < linkEvents.length; i++) {
            int start = links[i * 3], end = links[i * 3 + 1];
            if (offset < start || offset >= end) continue;
            if (best < 0 || end - start < links[best * 3 + 1] - links[best * 3]) best = i;
        }
        return best;
    }

    int linkNode(int link) {
        return links[link * 3 + 2];
    }

    long linkEvents(int link) {
        return linkEvents[link];
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
        drawDecorations(canvas);
        canvas.restore();
    }

    private final android.graphics.Paint decorationPaint = new android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG);

    private final android.graphics.Path decorationPath = new android.graphics.Path();

    /** Underlines and strikethroughs, across the lines each run covers. */
    private void drawDecorations(Canvas canvas) {
        CharSequence text = layout.getText();
        if (!(text instanceof android.text.Spanned)) return;
        android.text.Spanned spanned = (android.text.Spanned) text;
        TextLayouts.Decoration[] decorations = spanned.getSpans(0, text.length(), TextLayouts.Decoration.class);
        if (decorations.length == 0) return;
        int lines = layout.getLineCount();
        for (TextLayouts.Decoration d : decorations) {
            int start = spanned.getSpanStart(d), end = spanned.getSpanEnd(d);
            decorationPaint.setColor(d.color);
            float thickness = d.thicknessPx > 0 ? d.thicknessPx : Math.max(1, d.sizePx / 16);
            decorationPaint.setStrokeWidth(thickness);
            decorationPaint.setStyle(android.graphics.Paint.Style.STROKE);
            switch (d.style) {
                case 2:
                    decorationPaint.setPathEffect(new android.graphics.DashPathEffect(new float[] {0, thickness * 2}, 0));
                    decorationPaint.setStrokeCap(android.graphics.Paint.Cap.ROUND);
                    break;
                case 3:
                    decorationPaint.setPathEffect(new android.graphics.DashPathEffect(new float[] {thickness * 3, thickness * 2}, 0));
                    decorationPaint.setStrokeCap(android.graphics.Paint.Cap.BUTT);
                    break;
                default:
                    decorationPaint.setPathEffect(null);
                    decorationPaint.setStrokeCap(android.graphics.Paint.Cap.BUTT);
            }
            for (int line = layout.getLineForOffset(start); line < lines; line++) {
                int lineStart = layout.getLineStart(line), lineEnd = layout.getLineVisibleEnd(line);
                if (lineStart >= end) break;
                int a = Math.max(start, lineStart), b = Math.min(end, lineEnd);
                if (a >= b) continue;
                float x1 = layout.getPrimaryHorizontal(a);
                float x2 = b < layout.getLineEnd(line) ? layout.getPrimaryHorizontal(b) : layout.getLineRight(line);
                if (layout.getParagraphDirection(line) < 0 && b >= lineEnd) x2 = layout.getLineLeft(line);
                float baseline = layout.getLineBaseline(line);
                if (d.underline) {
                    float y = baseline + (Float.isNaN(d.offsetPx) ? d.sizePx * 0.11f : d.offsetPx + thickness / 2);
                    line(canvas, d.style, x1, x2, y, thickness);
                }
                if (d.strike) line(canvas, d.style, x1, x2, baseline - d.sizePx * 0.27f, thickness);
            }
        }
    }

    /** One decoration line centered on {@code y}. */
    private void line(Canvas canvas, int style, float x1, float x2, float y, float thickness) {
        if (x2 < x1) {
            float t = x1;
            x1 = x2;
            x2 = t;
        }
        switch (style) {
            case 1:
                canvas.drawLine(x1, y - thickness, x2, y - thickness, decorationPaint);
                canvas.drawLine(x1, y + thickness, x2, y + thickness, decorationPaint);
                break;
            case 4: {
                float amplitude = thickness * 1.5f, period = Math.max(4, thickness * 4);
                decorationPath.reset();
                decorationPath.moveTo(x1, y);
                int steps = Math.max(2, (int) Math.ceil((x2 - x1) / period * 8));
                for (int i = 1; i <= steps; i++) {
                    float t = (x2 - x1) * i / steps;
                    decorationPath.lineTo(x1 + t, y + amplitude * (float) Math.sin(t / period * Math.PI * 2));
                }
                canvas.drawPath(decorationPath, decorationPaint);
                break;
            }
            default:
                canvas.drawLine(x1, y, x2, y, decorationPaint);
        }
    }
}
