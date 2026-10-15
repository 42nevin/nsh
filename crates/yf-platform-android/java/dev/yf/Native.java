package dev.yf;

/** JNI entry points of libyf.so (crates/yf-platform-android/src/host.rs). */
final class Native {
    static {
        System.loadLibrary("yf");
    }

    private Native() {}

    // Event kinds, in `yf_core::EventKind` order.
    static final int PRESS = 0;
    static final int LONG_PRESS = 1;
    static final int PAN_START = 2;
    static final int PAN_END = 3;
    static final int REFRESH = 4;
    static final int END_REACHED = 5;
    static final int SCROLL_END = 6;
    static final int LOAD = 7;
    static final int ERROR = 8;
    static final int CHANGE_TEXT = 9;
    static final int SUBMIT_EDITING = 10;
    static final int FOCUS = 11;
    static final int BLUR = 12;
    static final int PRESS_IN = 13;
    static final int PRESS_OUT = 14;
    static final int LAYOUT = 15;
    static final int PINCH_START = 16;
    static final int PINCH_END = 17;
    static final int ROTATION_START = 18;
    static final int ROTATION_END = 19;
    static final int ACCESSIBILITY_ACTION = 20;
    static final int VALUE_CHANGE = 21;
    static final int SLIDING_COMPLETE = 22;
    static final int DISMISS = 23;
    static final int REQUEST_CLOSE = 24;
    static final int SHOW = 25;
    static final int INSETS = 26;
    static final int LOAD_START = 27;
    static final int MESSAGE = 28;
    static final int NAVIGATION_STATE_CHANGE = 29;
    static final int PROGRESS = 30;
    static final int END = 31;
    static final int BUFFER = 32;
    static final int VIEWABLE_ITEMS_CHANGED = 33;
    static final int BACK_PRESS = 34;
    static final int SCROLL = 35;
    static final int SCROLL_BEGIN_DRAG = 36;
    static final int SCROLL_END_DRAG = 37;
    static final int MOMENTUM_SCROLL_BEGIN = 38;
    static final int MOMENTUM_SCROLL_END = 39;
    static final int DIALOG = 40;
    static final int TOUCH = 41;
    static final int SELECTION_CHANGE = 42;
    static final int KEY_PRESS = 43;
    static final int LOAD_END = 44;

    static native long create(YfView view, float density, boolean trace);

    static native void destroy(long handle);

    static native void load(long handle, byte[] code, boolean bytecode, String sourceMap);

    static native boolean connectDevServer(long handle, String url);

    /** Root size in dp. */
    static native void resize(long handle, float width, float height);

    /**
     * values: safe area top, right, bottom, left, keyboard height, keyboard
     * animation duration (s), font scale, 1 if the language is right to left.
     */
    static native void environment(long handle, float[] values, boolean dark);

    static native void event(long handle, int id, int kind, String text, int count, float x, float y);

    /**
     * An event with an object payload ({@code Double}, {@code Boolean} or
     * {@code String} values), or a single value when {@code keys} is null.
     */
    static native void eventMap(long handle, int id, int kind, String[] keys, Object[] values);

    /** Applies the animation items of batches up to {@code seq}; returns bound values. */
    static native float[] afterBatch(long handle, long seq);

    static native float[] tick(long handle, long frameNanos);

    /**
     * A scroll view's offset, after applying {@code applied} of the scrolls
     * the core asked for. Returns bound values when a scroll value changed,
     * else null.
     */
    static native float[] scroll(long handle, int id, float x, float y, int applied);

    /**
     * Touches for the JS responder system. phase: 0 start, 1 move, 2 end,
     * 3 cancel. Per finger: id, target node, path length (in {@code paths},
     * innermost first), pageX, pageY, locationX, locationY (in
     * {@code coords}, dp) and whether it changed. {@code time}: ms.
     */
    static native void touch(long handle, int phase, int[] ids, int[] targets, int[] pathLengths, int[] paths,
            float[] coords, boolean[] changed, long time);

    static native boolean hasPan(long handle, int id);

    /** state: 0 began, 1 moved, 2 ended, 3 cancelled. */
    static native float[] pan(long handle, int id, int state, float tx, float ty, float vx, float vy);

    /** A pinch (scale) or two-finger rotation (degrees); state as for pan. */
    static native float[] transformGesture(long handle, int id, boolean rotation, int state, float amount, float velocity);

    /** The window of modal {@code id} is {@code width} x {@code height} dp. */
    static native void rootSize(long handle, int id, float width, float height, float top, float right, float bottom, float left);

    /** A display frame began after {@code YfView.requestJsFrame}. */
    static native void frame(long handle);
}
