package dev.nsh;

import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.Canvas;
import android.text.Layout;
import android.view.View;
import android.view.accessibility.AccessibilityNodeInfo;

/**
 * Draws a StaticLayout made on the engine thread: no text layout here. A
 * group: the views of its attachments (views and images inside the text)
 * are its children, placed on their placeholders by the core.
 */
@SuppressLint("ViewConstructor")
final class TextBox extends android.view.ViewGroup implements Node {
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
        setWillNotDraw(false);
        setClipChildren(false);
        setBackground(decor);
        setImportantForAccessibility(IMPORTANT_FOR_ACCESSIBILITY_YES);
    }

    @Override
    public int nodeId() {
        return id;
    }

    /** Attachments are placed by the core (`Box.place`). */
    @Override
    protected void onLayout(boolean changed, int l, int t, int r, int b) {}

    @Override
    public boolean shouldDelayChildPressedState() {
        return false;
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        decor.applyPivot(this);
    }

    /** `selectable`: a long press offers to copy the text. */
    boolean selectable;
    /** The copy menu shows: the text is highlighted. */
    private boolean selected;
    private float downX, downY;
    private final Runnable select = this::select;

    @android.annotation.SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(android.view.MotionEvent e) {
        boolean pressed = press != null && press.onTouchEvent(e);
        if (!selectable) return pressed;
        switch (e.getActionMasked()) {
            case android.view.MotionEvent.ACTION_DOWN:
                downX = e.getX();
                downY = e.getY();
                postDelayed(select, android.view.ViewConfiguration.getLongPressTimeout());
                break;
            case android.view.MotionEvent.ACTION_MOVE: {
                float slop = android.view.ViewConfiguration.get(getContext()).getScaledTouchSlop();
                if (Math.hypot(e.getX() - downX, e.getY() - downY) > slop) removeCallbacks(select);
                break;
            }
            case android.view.MotionEvent.ACTION_UP:
            case android.view.MotionEvent.ACTION_CANCEL:
                removeCallbacks(select);
                break;
            default:
                break;
        }
        return true;
    }

    /** Highlights the text and shows the floating copy toolbar. */
    private void select() {
        if (layout == null) return;
        selected = true;
        invalidate();
        startActionMode(new android.view.ActionMode.Callback2() {
            @Override
            public boolean onCreateActionMode(android.view.ActionMode mode, android.view.Menu menu) {
                menu.add(android.view.Menu.NONE, android.R.id.copy, 0, android.R.string.copy);
                return true;
            }

            @Override
            public boolean onPrepareActionMode(android.view.ActionMode mode, android.view.Menu menu) {
                return false;
            }

            @Override
            public boolean onActionItemClicked(android.view.ActionMode mode, android.view.MenuItem item) {
                if (item.getItemId() != android.R.id.copy) return false;
                android.content.ClipboardManager clipboard =
                        (android.content.ClipboardManager) getContext().getSystemService(Context.CLIPBOARD_SERVICE);
                // Without the placeholders of attachments.
                String copied = layout.getText().toString().replace("\uFFFC", "");
                clipboard.setPrimaryClip(android.content.ClipData.newPlainText(null, copied));
                mode.finish();
                return true;
            }

            @Override
            public void onDestroyActionMode(android.view.ActionMode mode) {
                selected = false;
                invalidate();
            }

            @Override
            public void onGetContentRect(android.view.ActionMode mode, View view, android.graphics.Rect out) {
                out.set(Math.round(contentX), Math.round(contentY), Math.round(contentX + contentWidth),
                        Math.round(contentY + contentHeight));
            }
        }, android.view.ActionMode.TYPE_FLOATING);
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
        if (selected) {
            // The whole text is what the toolbar copies.
            android.graphics.Paint highlight = new android.graphics.Paint();
            highlight.setColor(0x403b82f6);
            canvas.drawRect(0, 0, contentWidth, contentHeight, highlight);
        }
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
