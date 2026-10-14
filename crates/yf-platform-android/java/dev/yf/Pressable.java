package dev.yf;

import android.view.MotionEvent;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.ViewParent;
import android.widget.HorizontalScrollView;
import android.widget.ScrollView;

/**
 * Press handling for leaf views (text, images): `onPressIn`, `onPressOut`,
 * `onLongPress` and `onPress` with React Native semantics (see `Box`).
 */
final class Pressable {
    private final View view;
    private final Mounter mounter;
    private final int id;
    private float downX, downY;
    private boolean tracking, pressedIn, longPressed;
    private final Runnable pressIn = this::pressIn;
    private final Runnable longPress = this::longPress;

    Pressable(View view, Mounter mounter, int id) {
        this.view = view;
        this.mounter = mounter;
        this.id = id;
    }

    boolean wanted() {
        return mounter.wants(id, Native.PRESS) || mounter.wants(id, Native.LONG_PRESS)
                || mounter.wants(id, Native.PRESS_IN) || mounter.wants(id, Native.PRESS_OUT);
    }

    boolean onTouchEvent(MotionEvent e) {
        if (!wanted()) return false;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                downX = e.getX();
                downY = e.getY();
                tracking = true;
                pressedIn = false;
                longPressed = false;
                if (insideScrollView()) view.postDelayed(pressIn, ViewConfiguration.getTapTimeout());
                else pressIn();
                if (mounter.wants(id, Native.LONG_PRESS)) view.postDelayed(longPress, ViewConfiguration.getLongPressTimeout());
                return true;
            case MotionEvent.ACTION_MOVE: {
                float slop = ViewConfiguration.get(view.getContext()).getScaledTouchSlop();
                float margin = 20 * mounter.density;
                boolean inside = e.getX() >= -margin && e.getY() >= -margin
                        && e.getX() <= view.getWidth() + margin && e.getY() <= view.getHeight() + margin;
                if (tracking && (!inside || (!pressedIn && Math.hypot(e.getX() - downX, e.getY() - downY) > slop * 4))) {
                    tracking = false;
                    pressOut();
                }
                return true;
            }
            case MotionEvent.ACTION_UP:
                if (tracking) {
                    if (!pressedIn) pressIn();
                    boolean press = !longPressed && mounter.wants(id, Native.PRESS);
                    pressOut();
                    if (press) mounter.event(id, Native.PRESS);
                }
                tracking = false;
                return true;
            case MotionEvent.ACTION_CANCEL:
                tracking = false;
                pressOut();
                return true;
        }
        return true;
    }

    /** An accessibility click (TalkBack's double tap). */
    void click() {
        if (mounter.wants(id, Native.PRESS)) mounter.event(id, Native.PRESS);
    }

    private void pressIn() {
        if (pressedIn || !tracking) return;
        pressedIn = true;
        if (mounter.wants(id, Native.PRESS_IN)) mounter.event(id, Native.PRESS_IN);
    }

    private void pressOut() {
        view.removeCallbacks(pressIn);
        view.removeCallbacks(longPress);
        if (!pressedIn) return;
        pressedIn = false;
        if (mounter.wants(id, Native.PRESS_OUT)) mounter.event(id, Native.PRESS_OUT);
    }

    private void longPress() {
        if (!tracking) return;
        if (!pressedIn) pressIn();
        longPressed = true;
        mounter.event(id, Native.LONG_PRESS);
    }

    private boolean insideScrollView() {
        for (ViewParent p = view.getParent(); p != null; p = p.getParent()) {
            if (p instanceof ScrollView || p instanceof HorizontalScrollView) return true;
        }
        return false;
    }
}
