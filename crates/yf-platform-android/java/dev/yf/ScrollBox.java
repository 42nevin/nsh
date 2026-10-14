package dev.yf;

import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.Rect;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.ProgressBar;
import android.widget.ScrollView;

/**
 * `<scroll>` and lists: a platform ScrollView (native fling and overscroll
 * effect) whose content box is sized by the core, plus pull to refresh,
 * paging and snapping, drag / momentum events and
 * `keyboardShouldPersistTaps`.
 */
@SuppressLint("ViewConstructor")
final class ScrollBox extends FrameLayout implements Node {
    final Mounter mounter;
    final int id;
    final Decor decor = new Decor();
    long events;
    boolean horizontal;
    int dismissMode; // 0 none, 1 on drag, 2 interactive
    private FrameLayout scroller;
    final Content content;
    private final ProgressBar spinner;
    private boolean refreshing;
    /** Content offset while pulling or refreshing (px). */
    private float pull;
    private boolean pulling;
    private float startY;
    private boolean touching;
    private final Runnable settled = this::settled;

    // `ScrollBehavior`; offsets and lengths in dp.
    private boolean paging, enabled = true, bounces = true, oneStep;
    private float snapInterval, deceleration;
    private float[] snapOffsets = new float[0];
    private int snapAlignment; // 0 start, 1 center, 2 end
    private int keyboardTaps; // 0 never, 1 always, 2 handled
    /** `contentInset` (px) and the room kept for the keyboard below it. */
    private int insetTop, insetRight, insetBottom, insetLeft, keyboardPadding;

    // The current gesture.
    /** The user dragged the content. */
    private boolean dragging;
    /** The scroll view flung (or snapped) after the drag: gliding. */
    private boolean flung, momentum;
    /** This touch only dismisses the keyboard (`keyboardShouldPersistTaps`); -1: it does not. */
    private int tapMode = -1;
    private boolean swallowing;

    /** The scrolled content, sized by the core (`ScrollContent`). */
    static final class Content extends Box {
        int contentWidth, contentHeight;

        Content(Context context, Mounter mounter, int id) {
            super(context, mounter, id);
        }

        @Override
        protected void onMeasure(int widthSpec, int heightSpec) {
            int w = Math.max(contentWidth, MeasureSpec.getSize(widthSpec));
            int h = Math.max(contentHeight, MeasureSpec.getSize(heightSpec));
            super.onMeasure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY));
        }
    }

    ScrollBox(Context context, Mounter mounter, int id) {
        super(context);
        this.mounter = mounter;
        this.id = id;
        setBackground(decor);
        content = new Content(context, mounter, id);
        spinner = new ProgressBar(context);
        spinner.setIndeterminate(true);
        spinner.setVisibility(GONE);
        int size = Math.round(28 * mounter.density);
        addView(spinner, new LayoutParams(size, size, android.view.Gravity.CENTER_HORIZONTAL | android.view.Gravity.TOP));
        content.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> payOwed());
        build();
    }

    @Override
    public int nodeId() {
        return id;
    }

    private void build() {
        if (scroller != null) {
            scroller.removeView(content);
            removeView(scroller);
        }
        scroller = horizontal ? new HScroll(getContext()) : new VScroll(getContext());
        scroller.setOverScrollMode(bounces ? OVER_SCROLL_IF_CONTENT_SCROLLS : OVER_SCROLL_NEVER);
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
        scroller.setClipToPadding(false);
        // A scroll view inside another one scrolls first, then hands the
        // rest to its parent (else the parent takes every drag along its axis).
        scroller.setNestedScrollingEnabled(true);
        scroller.setVerticalScrollBarEnabled(!horizontal);
        scroller.setHorizontalScrollBarEnabled(horizontal);
        scroller.addView(content, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        // The view or its content changed size: scroll what was clamped.
        scroller.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> payOwed());
        scroller.setOnScrollChangeListener((v, x, y, ox, oy) -> {
            if (stickyCells.length > 0) pinSticky();
            if (touching && !dragging && !swallowing) {
                dragging = true;
                // Caught while gliding: the glide ended.
                if (momentum) emit(Native.MOMENTUM_SCROLL_END);
                momentum = false;
                emit(Native.SCROLL_BEGIN_DRAG);
            } else if (swallowing) {
                dragging = true;
            }
            float d = mounter.density;
            mounter.scroll(id, x / d, y / d, applied);
            removeCallbacks(settled);
            postDelayed(settled, 120);
        });
        addView(scroller, 0, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
    }

    void configure(boolean horizontal, boolean indicator, int dismissMode) {
        if (horizontal != this.horizontal) {
            this.horizontal = horizontal;
            build();
        }
        scroller.setVerticalScrollBarEnabled(indicator && !horizontal);
        scroller.setHorizontalScrollBarEnabled(indicator && horizontal);
        this.dismissMode = dismissMode;
    }

    void setContentSize(int width, int height) {
        content.contentWidth = width;
        content.contentHeight = height;
        // Sized right away: scrolling in the same batch (anchoring, a
        // jump to an item) is clamped to the content's current size.
        int w = Math.max(width, scroller.getWidth()), h = Math.max(height, scroller.getHeight());
        content.measure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY));
        content.layout(0, 0, w, h);
        content.requestLayout();
    }

    /** Sticky cells: node ids and their travel (px), from `StickyCells`. */
    private int[] stickyCells = new int[0];
    private float[] stickyStart = new float[0], stickyEnd = new float[0];

    void setSticky(int[] cells, float[] start, float[] end) {
        for (int old : stickyCells) {
            boolean kept = false;
            for (int c : cells) kept |= c == old;
            View v = mounter.view(old);
            if (!kept && v != null) {
                if (horizontal) v.setTranslationX(0); else v.setTranslationY(0);
            }
        }
        stickyCells = cells;
        stickyStart = start;
        stickyEnd = end;
        frontSticky();
        pinSticky();
    }

    /** Keeps sticky cells above the others (also after new cells arrive). */
    void frontSticky() {
        for (int c : stickyCells) {
            View v = mounter.view(c);
            if (v != null && v.getParent() == content && content.indexOfChild(v) != content.getChildCount() - 1) {
                v.bringToFront();
            }
        }
    }

    boolean hasSticky() {
        return stickyCells.length > 0;
    }

    /** Moves the sticky cells with the scroll offset. */
    private void pinSticky() {
        float offset = horizontal ? scroller.getScrollX() : scroller.getScrollY();
        for (int i = 0; i < stickyCells.length; i++) {
            View v = mounter.view(stickyCells[i]);
            if (v == null) continue;
            float t = Math.max(0, Math.min(offset - stickyStart[i], stickyEnd[i] - stickyStart[i]));
            if (horizontal) v.setTranslationX(t); else v.setTranslationY(t);
        }
    }

    /** Room below the content (keyboard avoidance), under `contentInset`. */
    void setContentPaddingBottom(int px) {
        keyboardPadding = px;
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
    }

    /** `ScrollBehavior` (lengths in dp). */
    void setBehavior(boolean paging, float snapInterval, float[] snapOffsets, int snapAlignment, float deceleration,
            boolean enabled, boolean bounces, float top, float right, float bottom, float left, int keyboardTaps,
            boolean oneStep) {
        this.paging = paging;
        this.snapInterval = snapInterval;
        this.snapOffsets = snapOffsets;
        this.snapAlignment = snapAlignment;
        this.deceleration = deceleration;
        this.enabled = enabled;
        this.bounces = bounces;
        this.keyboardTaps = keyboardTaps;
        this.oneStep = oneStep;
        float d = mounter.density;
        insetTop = Math.round(top * d);
        insetRight = Math.round(right * d);
        insetBottom = Math.round(bottom * d);
        insetLeft = Math.round(left * d);
        scroller.setOverScrollMode(bounces ? OVER_SCROLL_IF_CONTENT_SCROLLS : OVER_SCROLL_NEVER);
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
    }

    // --- Paging and snapping ---------------------------------------------

    private boolean snaps() {
        return paging || snapInterval > 0 || snapOffsets.length > 0;
    }

    /** The scroll view's own fling (px/s): snaps instead when snapping. */
    private boolean fling(int velocity) {
        flung = true;
        if (!snaps()) return false;
        snap(velocity);
        return true;
    }

    /** Glides to the snap point for a release at {@code velocity} (px/s). */
    private void snap(int velocity) {
        float d = mounter.density;
        boolean h = horizontal;
        float offset = (h ? scroller.getScrollX() : scroller.getScrollY()) / d;
        float viewport = (h ? scroller.getWidth() : scroller.getHeight()) / d;
        float length = (h ? content.getWidth() + scroller.getPaddingLeft() + scroller.getPaddingRight()
                : content.getHeight() + scroller.getPaddingTop() + scroller.getPaddingBottom()) / d;
        float v = velocity / d / 1000f;
        // Where UIKit's deceleration would glide (velocity kept per ms).
        float rate = deceleration > 0 ? deceleration : (snapInterval > 0 || snapOffsets.length > 0) ? 0.99f : 0.998f;
        float projected = offset + v * rate / (1 - rate);
        float target = snapTarget(offset, projected, v, viewport, length);
        int px = Math.round(target * d);
        if (scroller instanceof ScrollView) ((ScrollView) scroller).smoothScrollTo(0, px);
        else ((HorizontalScrollView) scroller).smoothScrollTo(px, 0);
    }

    /** `ScrollBehavior::snap_target` (yf-core), in dp. */
    private float snapTarget(float offset, float projected, float velocity, float viewport, float content) {
        float max = Math.max(0, content - viewport);
        float[] points;
        if (snapInterval > 0) {
            float shift = snapAlignment == 1 ? (viewport - snapInterval) / 2 : snapAlignment == 2 ? viewport - snapInterval : 0;
            int n = (int) Math.max(0, Math.ceil((max + shift) / snapInterval));
            points = new float[n + 1];
            for (int i = 0; i <= n; i++) points[i] = Math.max(0, Math.min(max, i * snapInterval - shift));
        } else if (snapOffsets.length > 0) {
            points = new float[snapOffsets.length];
            for (int i = 0; i < points.length; i++) points[i] = Math.max(0, Math.min(max, snapOffsets[i]));
        } else {
            float page = Math.max(1, viewport);
            int n = (int) Math.ceil(max / page);
            points = new float[n + 1];
            for (int i = 0; i <= n; i++) points[i] = Math.min(max, i * page);
        }
        final float slow = 0.3f; // dp per ms: slower releases settle at the nearest point
        boolean step = oneStep || (paging && snapInterval <= 0 && snapOffsets.length == 0);
        float from = step ? offset : projected;
        Float target = null;
        if (velocity > slow) {
            for (float p : points) {
                if (step ? p > from + 0.5f : p >= from - 0.5f) { target = p; break; }
            }
        } else if (velocity < -slow) {
            for (int i = points.length - 1; i >= 0; i--) {
                float p = points[i];
                if (step ? p < from - 0.5f : p <= from + 0.5f) { target = p; break; }
            }
        } else {
            for (float p : points) {
                if (target == null || Math.abs(p - from) < Math.abs(target - from)) target = p;
            }
        }
        return target != null ? target : velocity > 0 ? max : 0;
    }

    /** The platform scroll views, with `scrollEnabled` and snapping. */
    private final class VScroll extends ScrollView {
        VScroll(Context context) {
            super(context);
        }

        @Override
        public boolean onInterceptTouchEvent(MotionEvent e) {
            return enabled && super.onInterceptTouchEvent(e);
        }

        @SuppressLint("ClickableViewAccessibility")
        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (!enabled) return false;
            boolean up = beforeTouch(e);
            boolean handled = super.onTouchEvent(e);
            if (up) released();
            return handled;
        }

        @Override
        public void fling(int velocity) {
            if (!ScrollBox.this.fling(velocity)) super.fling(velocity);
        }
    }

    private final class HScroll extends HorizontalScrollView {
        HScroll(Context context) {
            super(context);
        }

        @Override
        public boolean onInterceptTouchEvent(MotionEvent e) {
            return enabled && super.onInterceptTouchEvent(e);
        }

        @SuppressLint("ClickableViewAccessibility")
        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (!enabled) return false;
            boolean up = beforeTouch(e);
            boolean handled = super.onTouchEvent(e);
            if (up) released();
            return handled;
        }

        @Override
        public void fling(int velocity) {
            if (!ScrollBox.this.fling(velocity)) super.fling(velocity);
        }
    }

    /** Before the scroll view sees {@code e}: whether it ends the touch. */
    private boolean beforeTouch(MotionEvent e) {
        int action = e.getActionMasked();
        boolean up = action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL;
        if (up) flung = false;
        return up;
    }

    /** The finger lifted off the scroll view (after its fling, if any). */
    private void released() {
        if (!dragging) return;
        if (!flung && snaps()) {
            snap(0);
            flung = true;
        }
        emit(Native.SCROLL_END_DRAG);
        if (flung) {
            momentum = true;
            emit(Native.MOMENTUM_SCROLL_BEGIN);
        }
    }

    private void emit(int kind) {
        if (!mounter.wants(id, kind)) return;
        float d = mounter.density;
        mounter.event(id, kind, null, 0, scroller.getScrollX() / d, scroller.getScrollY() / d);
    }

    /**
     * Scrolls a descendant into the area above the bottom padding (the
     * keyboard). `requestRectangleOnScreen` cannot: ScrollView ignores its
     * padding when it decides whether a rectangle is visible.
     */
    void reveal(View v, int margin) {
        Rect r = new Rect(0, -margin, v.getWidth(), v.getHeight() + margin);
        scroller.offsetDescendantRectToMyCoords(v, r);
        int top = scroller.getScrollY();
        int bottom = top + scroller.getHeight() - scroller.getPaddingBottom();
        int dy = r.bottom > bottom ? Math.min(r.bottom - bottom, r.top - top) : Math.min(0, r.top - top);
        if (dy == 0) return;
        if (scroller instanceof ScrollView) ((ScrollView) scroller).smoothScrollBy(0, dy);
        else scroller.scrollBy(0, dy);
    }

    /** Scrolls the core asked for that were applied (reported with offsets). */
    private int applied;

    /**
     * The part of the last {@code scrollByPx} that the scroll view clamped
     * (px): the view or its content is about to change size (e.g. while a
     * layout animation resizes it) and catches up when it does.
     */
    private int owedX, owedY;

    void scrollByPx(int dx, int dy) {
        applied++;
        int x = scroller.getScrollX() + dx + owedX, y = scroller.getScrollY() + dy + owedY;
        scroller.scrollTo(x, y);
        owedX = x - scroller.getScrollX();
        owedY = y - scroller.getScrollY();
    }

    /** Pays what clamping kept back, as far as the current size allows. */
    private void payOwed() {
        if (owedX == 0 && owedY == 0) return;
        int x = scroller.getScrollX() + owedX, y = scroller.getScrollY() + owedY;
        scroller.scrollTo(x, y);
        owedX = x - scroller.getScrollX();
        owedY = y - scroller.getScrollY();
    }

    void scrollToPx(int x, int y, boolean animated) {
        applied++;
        if (scroller instanceof ScrollView) {
            if (animated) ((ScrollView) scroller).smoothScrollTo(x, y);
            else scroller.scrollTo(x, y);
        } else {
            if (animated) ((HorizontalScrollView) scroller).smoothScrollTo(x, y);
            else scroller.scrollTo(x, y);
        }
    }

    /** Scrolling came to rest (no API for it: no change for a moment). */
    private void settled() {
        if (touching) {
            postDelayed(settled, 120);
            return;
        }
        if (momentum) {
            momentum = false;
            emit(Native.MOMENTUM_SCROLL_END);
        }
        float d = mounter.density;
        mounter.event(id, Native.SCROLL_END, null, 0, scroller.getScrollX() / d, scroller.getScrollY() / d);
    }

    // --- Pull to refresh -------------------------------------------------

    private boolean canPull() {
        return !horizontal && (events & (1L << Native.REFRESH)) != 0 && scroller.getScrollY() == 0;
    }

    @Override
    public boolean dispatchTouchEvent(MotionEvent e) {
        int action = e.getActionMasked();
        touching = action != MotionEvent.ACTION_UP && action != MotionEvent.ACTION_CANCEL;
        if (action == MotionEvent.ACTION_DOWN) {
            // The user scrolls now: what was owed no longer applies.
            owedX = owedY = 0;
            dragging = false;
            tapMode = tapMode(e);
            // "never": the tap reaches nothing but the scroll view.
            swallowing = tapMode == 0;
        }
        if (action == MotionEvent.ACTION_MOVE && dismissMode != 0) {
            float slop = ViewConfiguration.get(getContext()).getScaledTouchSlop();
            if (Math.abs(e.getY() - startY) > slop) hideKeyboard();
        }
        if (action == MotionEvent.ACTION_DOWN) startY = e.getY();
        boolean handled;
        if (swallowing) {
            MotionEvent local = MotionEvent.obtain(e);
            local.offsetLocation(-scroller.getLeft() - scroller.getTranslationX(), -scroller.getTop() - scroller.getTranslationY());
            scroller.onTouchEvent(local);
            local.recycle();
            handled = true;
        } else {
            handled = super.dispatchTouchEvent(e);
        }
        if (action == MotionEvent.ACTION_UP && tapMode >= 0 && !dragging
                && (tapMode == 0 || !handlesTap(this, e.getX(), e.getY()))) {
            hideKeyboard();
        }
        if (!touching) {
            swallowing = false;
            tapMode = -1;
        }
        return handled;
    }

    /**
     * How a touch treats the keyboard ({@code keyboardShouldPersistTaps}):
     * -1 when no input is focused or the touch is on an input, else the
     * mode of the outermost scroll view that does not let taps through.
     */
    private int tapMode(MotionEvent e) {
        if (mounter.focusedInput() == null || inputAt(this, e.getX(), e.getY())) return -1;
        for (ViewParent p = getParent(); p != null; p = p.getParent()) {
            if (p instanceof ScrollBox && ((ScrollBox) p).keyboardTaps != 1) return -1;
        }
        return keyboardTaps == 1 ? -1 : keyboardTaps;
    }

    /** Whether an input is under ({@code x}, {@code y}) of {@code group}. */
    private static boolean inputAt(ViewGroup group, float x, float y) {
        View v = childAt(group, x, y);
        if (v == null) return false;
        if (v instanceof InputBox) return true;
        return v instanceof ViewGroup && inputAt((ViewGroup) v, localX(group, v, x), localY(group, v, y));
    }

    /** Whether something under ({@code x}, {@code y}) of {@code group} handles taps. */
    private static boolean handlesTap(ViewGroup group, float x, float y) {
        View v = childAt(group, x, y);
        if (v == null) return false;
        if (v.isClickable() || v.isLongClickable() || v instanceof InputBox || v instanceof android.widget.SeekBar) return true;
        return v instanceof ViewGroup && handlesTap((ViewGroup) v, localX(group, v, x), localY(group, v, y));
    }

    /** The topmost visible child of {@code group} under ({@code x}, {@code y}). */
    private static View childAt(ViewGroup group, float x, float y) {
        for (int i = group.getChildCount() - 1; i >= 0; i--) {
            View c = group.getChildAt(i);
            if (c.getVisibility() != VISIBLE) continue;
            float lx = localX(group, c, x), ly = localY(group, c, y);
            if (lx >= 0 && ly >= 0 && lx < c.getWidth() && ly < c.getHeight()) return c;
        }
        return null;
    }

    private static float localX(ViewGroup group, View child, float x) {
        return x + group.getScrollX() - child.getLeft() - child.getTranslationX();
    }

    private static float localY(ViewGroup group, View child, float y) {
        return y + group.getScrollY() - child.getTop() - child.getTranslationY();
    }

    private void hideKeyboard() {
        View focused = findFocus();
        if (focused == null) return;
        InputMethodManager imm = (InputMethodManager) getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        imm.hideSoftInputFromWindow(focused.getWindowToken(), 0);
        mounter.host.requestFocus();
    }

    @Override
    public boolean onInterceptTouchEvent(MotionEvent e) {
        if (refreshing || !canPull()) return false;
        if (e.getActionMasked() == MotionEvent.ACTION_MOVE) {
            float dy = e.getY() - startY;
            float slop = ViewConfiguration.get(getContext()).getScaledTouchSlop();
            if (dy > slop) {
                pulling = true;
                startY = e.getY();
                return true;
            }
        }
        return false;
    }

    @SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(MotionEvent e) {
        if (!pulling) return false;
        float d = mounter.density;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_MOVE:
                // Resistance like a rubber band.
                setPull(Math.max(0, (e.getY() - startY) * 0.5f));
                spinner.setVisibility(VISIBLE);
                spinner.setAlpha(Math.min(1, pull / (64 * d)));
                return true;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                pulling = false;
                if (pull >= 64 * d) {
                    setRefreshing(true);
                    mounter.event(id, Native.REFRESH);
                } else {
                    animatePull(0, () -> spinner.setVisibility(GONE));
                }
                return true;
        }
        return true;
    }

    void setRefreshing(boolean on) {
        if (on == refreshing) return;
        refreshing = on;
        float d = mounter.density;
        if (on) {
            spinner.setVisibility(VISIBLE);
            spinner.setAlpha(1);
            animatePull(56 * d, null);
        } else {
            animatePull(0, () -> spinner.setVisibility(GONE));
        }
    }

    private void setPull(float px) {
        pull = px;
        scroller.setTranslationY(px);
        spinner.setTranslationY(Math.max(0, px / 2 - spinner.getHeight() / 2f));
    }

    private void animatePull(float to, Runnable end) {
        android.animation.ValueAnimator a = android.animation.ValueAnimator.ofFloat(pull, to);
        a.setDuration(250);
        a.addUpdateListener(v -> setPull((float) v.getAnimatedValue()));
        if (end != null) {
            a.addListener(new android.animation.AnimatorListenerAdapter() {
                @Override
                public void onAnimationEnd(android.animation.Animator animation) {
                    end.run();
                }
            });
        }
        a.start();
    }
}
