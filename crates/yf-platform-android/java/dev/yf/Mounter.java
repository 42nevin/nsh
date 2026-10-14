package dev.yf;

import android.animation.Animator;
import android.animation.AnimatorListenerAdapter;
import android.animation.TimeInterpolator;
import android.animation.ValueAnimator;
import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.Rect;
import android.text.Layout;
import android.util.SparseArray;
import android.util.SparseLongArray;
import android.view.View;
import android.view.animation.AccelerateDecelerateInterpolator;
import android.view.animation.AccelerateInterpolator;
import android.view.animation.DecelerateInterpolator;
import android.view.animation.LinearInterpolator;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.inputmethod.InputMethodManager;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/**
 * Applies encoded mount batches (crates/yf-platform-android/src/encode.rs)
 * to native views on the UI thread. Opcodes must match `encode.rs`.
 */
final class Mounter {
    private static final int CREATE = 0;
    private static final int INSERT = 1;
    private static final int REMOVE = 2;
    private static final int DELETE = 3;
    private static final int FRAME = 4;
    private static final int VIEW_STYLE = 5;
    private static final int TEXT = 6;
    private static final int EVENTS = 7;
    private static final int IMAGE = 8;
    private static final int SCROLL_CONFIG = 9;
    private static final int SCROLL_CONTENT = 10;
    private static final int SCROLL_BY = 11;
    private static final int REFRESHING = 12;
    private static final int DYNAMIC = 13;
    private static final int GESTURES = 14;
    private static final int INPUT = 15;
    private static final int INPUT_VALUE = 16;
    private static final int INPUT_CONTENT = 17;
    private static final int COMMAND = 18;
    private static final int VIEW_PROPS = 19;
    private static final int LAYOUT_ANIMATION = 20;
    private static final int SCREEN_CONFIG = 21;
    private static final int MODAL_CONFIG = 22;
    private static final int NATIVE_PROPS = 23;
    private static final int STICKY_CELLS = 24;
    private static final int SCROLL_BEHAVIOR = 25;

    /** A screen's options (`encode.rs` `screen_config`). */
    static final class ScreenOptions {
        String title;
        boolean shown = true, transparent, backVisible = true, gesture = true;
        int tint, background, titleColor, animation, left = -1, right = -1, titleView = -1;
        float headerHeight;
    }

    /** Properties that animations can override, as last set by a style. */
    private static final class Dynamic {
        float opacity = 1, tx, ty, sx = 1, sy = 1, rotate, rotateX, rotateY, perspective;
        int background;
    }

    /** One part of a layout animation (`encode.rs` `layout_transition`). */
    private static final class Transition {
        int curve; // 0 linear, 1 ease in, 2 ease out, 3 ease in-out, 4 spring
        float damping;
        int property; // 0 opacity, 1 scale
        long duration, delay;

        TimeInterpolator interpolator() {
            switch (curve) {
                case 0: return new LinearInterpolator();
                case 1: return new AccelerateInterpolator();
                case 2: return new DecelerateInterpolator();
                case 4: {
                    // React Native's spring interpolator.
                    float d = damping;
                    return t -> (float) (1 + Math.pow(2, -10 * t) * Math.sin((t - d / 4) * Math.PI * 2 / d));
                }
                default: return new AccelerateDecelerateInterpolator();
            }
        }
    }

    final YfView host;
    final float density;
    final Context context;
    final Images images;
    final SparseArray<View> views = new SparseArray<>();
    private final SparseArray<Dynamic> statics = new SparseArray<>();
    private final SparseLongArray events = new SparseLongArray();
    /** `hitSlop` of views that have one (px: top, right, bottom, left). */
    private final SparseArray<float[]> slops = new SparseArray<>();
    private final SparseArray<A11y> a11y = new SparseArray<>();
    /** Layout animation of the batch being applied: create, update, delete. */
    private Transition[] layoutAnimation;
    /** Views created by the batch being applied. */
    private final java.util.HashSet<Integer> created = new java.util.HashSet<>();
    /** Running frame / disappearance animations, by view. */
    private final SparseArray<Animator> layoutAnimators = new SparseArray<>();
    private InputBox focused;

    /** The focused input, if any. */
    InputBox focusedInput() {
        return focused;
    }
    /** Modals by id (their root views are in `views`). */
    private final SparseArray<ModalHost> modals = new SparseArray<>();
    /** Screens' options, applied once the batch's stacks are settled. */
    private final SparseArray<ScreenOptions> screenOptions = new SparseArray<>();
    /** Parents of screens, header items and modals (not their superviews). */
    private final android.util.SparseIntArray hosts = new android.util.SparseIntArray();
    private final java.util.LinkedHashSet<Integer> dirtyStacks = new java.util.LinkedHashSet<>();
    private final java.util.LinkedHashSet<Integer> dirtyScreens = new java.util.LinkedHashSet<>();
    private final java.util.LinkedHashSet<Integer> dirtyModals = new java.util.LinkedHashSet<>();
    /** Stacks and shown modals, oldest first (the back button tries the newest). */
    private final java.util.ArrayList<StackBox> stacks = new java.util.ArrayList<>();
    private final java.util.ArrayList<ModalHost> shownModals = new java.util.ArrayList<>();

    /** The surface root (node 1). */
    final Box root;

    Mounter(YfView host, Images images) {
        this.host = host;
        this.context = host.getContext();
        this.density = host.getResources().getDisplayMetrics().density;
        this.images = images;
        root = new Box(context, this, 1);
        views.put(1, root);
    }

    long handle() {
        return host.handle;
    }

    /** The view of node {@code id}, or null. */
    View view(int id) {
        return views.get(id);
    }

    private int px(float dp) {
        return Math.round(dp * density);
    }

    void apply(byte[] bytes, Object[] objects, long seq) {
        ByteBuffer b = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN);
        while (b.hasRemaining()) {
            int op = b.getInt();
            switch (op) {
                case CREATE: create(b.getInt(), b.getInt()); break;
                case INSERT: insert(b.getInt(), b.getInt(), b.getInt()); break;
                case REMOVE: {
                    int id = b.getInt();
                    if (!removeHosted(id)) remove(id);
                    break;
                }
                case DELETE: delete(b.getInt()); break;
                case FRAME: frame(b.getInt(), b.getFloat(), b.getFloat(), b.getFloat(), b.getFloat()); break;
                case VIEW_STYLE: style(b); break;
                case TEXT: {
                    View v = views.get(b.getInt());
                    Object layout = objects[b.getInt()];
                    float x = b.getFloat(), y = b.getFloat(), w = b.getFloat(), h = b.getFloat();
                    if (v instanceof TextBox) {
                        ((TextBox) v).setLayout((Layout) layout, x * density, y * density, w * density, h * density);
                    }
                    break;
                }
                case EVENTS: {
                    int id = b.getInt();
                    long mask = (b.getInt() & 0xffffffffL) | ((long) b.getInt() << 32);
                    events.put(id, mask);
                    View v = views.get(id);
                    if (v instanceof Box) {
                        ((Box) v).events = mask;
                        v.setClickable((mask & (1L << Native.PRESS)) != 0);
                    }
                    if (v instanceof ScrollBox) ((ScrollBox) v).events = mask;
                    boolean presses = (mask & ((1L << Native.PRESS) | (1L << Native.LONG_PRESS)
                            | (1L << Native.PRESS_IN) | (1L << Native.PRESS_OUT))) != 0;
                    if (v instanceof TextBox && presses && ((TextBox) v).press == null) {
                        ((TextBox) v).press = new Pressable(v, this, id);
                    }
                    if (v instanceof ImageBox && presses && ((ImageBox) v).press == null) {
                        ((ImageBox) v).press = new Pressable(v, this, id);
                    }
                    if (v instanceof TextBox || v instanceof ImageBox) v.setClickable(presses);
                    break;
                }
                case IMAGE: {
                    int id = b.getInt(), uri = b.getInt(), mode = b.getInt();
                    image(id, uri >= 0 ? (String) objects[uri] : null, mode);
                    break;
                }
                case SCROLL_CONFIG: {
                    View v = views.get(b.getInt());
                    boolean horizontal = b.getInt() != 0, indicator = b.getInt() != 0;
                    int dismiss = b.getInt();
                    if (v instanceof ScrollBox) ((ScrollBox) v).configure(horizontal, indicator, dismiss);
                    break;
                }
                case SCROLL_BEHAVIOR: {
                    View v = views.get(b.getInt());
                    boolean paging = b.getInt() != 0;
                    float interval = b.getFloat();
                    float[] offsets = new float[b.getInt()];
                    for (int i = 0; i < offsets.length; i++) offsets[i] = b.getFloat();
                    int alignment = b.getInt();
                    float deceleration = b.getFloat();
                    boolean enabled = b.getInt() != 0, bounces = b.getInt() != 0;
                    float top = b.getFloat(), right = b.getFloat(), bottom = b.getFloat(), left = b.getFloat();
                    int taps = b.getInt();
                    boolean oneStep = b.getInt() != 0;
                    if (v instanceof ScrollBox) {
                        ((ScrollBox) v).setBehavior(paging, interval, offsets, alignment, deceleration, enabled, bounces,
                                top, right, bottom, left, taps, oneStep);
                    }
                    break;
                }
                case SCROLL_CONTENT: {
                    View v = views.get(b.getInt());
                    float w = b.getFloat(), h = b.getFloat();
                    if (v instanceof ScrollBox) ((ScrollBox) v).setContentSize(px(w), px(h));
                    break;
                }
                case SCROLL_BY: {
                    View v = views.get(b.getInt());
                    float dx = b.getFloat(), dy = b.getFloat();
                    if (v instanceof ScrollBox) ((ScrollBox) v).scrollByPx(px(dx), px(dy));
                    break;
                }
                case REFRESHING: {
                    View v = views.get(b.getInt());
                    boolean on = b.getInt() != 0;
                    if (v instanceof ScrollBox) ((ScrollBox) v).setRefreshing(on);
                    break;
                }
                case DYNAMIC: {
                    int id = b.getInt();
                    // Bindings changed: back to the static values; the animator
                    // then writes what is still bound.
                    View v = views.get(id);
                    Dynamic d = statics.get(id);
                    if (v != null && d != null) applyDynamic(v, d);
                    break;
                }
                case GESTURES: {
                    View v = views.get(b.getInt());
                    b.getInt(); // pan values: the animator knows them (`Native.hasPan`)
                    int axis = b.getInt();
                    boolean pinch = b.getInt() != 0, rotation = b.getInt() != 0;
                    if (v instanceof Box) {
                        Box box = (Box) v;
                        box.panAxis = axis;
                        box.pinchable = pinch;
                        box.rotatable = rotation;
                    }
                    break;
                }
                case VIEW_PROPS: viewProps(b, objects); break;
                case SCREEN_CONFIG: screenConfig(b, objects); break;
                case NATIVE_PROPS: nativeProps(b, objects); break;
                case STICKY_CELLS: {
                    View v = views.get(b.getInt());
                    int n = b.getInt();
                    int[] cells = new int[n];
                    float[] start = new float[n], end = new float[n];
                    for (int i = 0; i < n; i++) {
                        cells[i] = b.getInt();
                        start[i] = b.getFloat() * density;
                        end[i] = b.getFloat() * density;
                    }
                    if (v instanceof ScrollBox) ((ScrollBox) v).setSticky(cells, start, end);
                    break;
                }
                case MODAL_CONFIG: {
                    ModalHost modal = modals.get(b.getInt());
                    int animation = b.getInt();
                    b.getInt(); // presentation: Android modals cover the window
                    boolean transparent = b.getInt() != 0;
                    if (modal != null) {
                        modal.animation = animation;
                        modal.transparent = transparent;
                    }
                    break;
                }
                case LAYOUT_ANIMATION:
                    layoutAnimation = new Transition[] {transition(b), transition(b), transition(b)};
                    break;
                case INPUT: input(b, objects); break;
                case INPUT_VALUE: {
                    View v = views.get(b.getInt());
                    String text = (String) objects[b.getInt()];
                    int count = b.getInt();
                    if (v instanceof InputBox) ((InputBox) v).setValue(text, count);
                    break;
                }
                case INPUT_CONTENT: {
                    View v = views.get(b.getInt());
                    float x = b.getFloat(), y = b.getFloat(), w = b.getFloat(), h = b.getFloat();
                    if (v instanceof InputBox) {
                        InputBox in = (InputBox) v;
                        Box.Frame f = Box.frame(in);
                        int width = f.right - f.left, height = f.bottom - f.top;
                        in.setContent(px(x), px(y), Math.max(0, width - px(x + w)), Math.max(0, height - px(y + h)));
                    }
                    break;
                }
                case COMMAND: command(b, objects); break;
                default:
                    throw new IllegalStateException("yf: unknown mount opcode " + op);
            }
        }
        if (layoutAnimation != null) {
            appear(layoutAnimation[0]);
            layoutAnimation = null;
        }
        created.clear();
        finishNavigation();
        outputs(Native.afterBatch(handle(), seq));
    }

    private static Transition transition(ByteBuffer b) {
        if (b.getInt() == 0) return null;
        Transition t = new Transition();
        t.curve = b.getInt();
        t.damping = b.getFloat();
        t.property = b.getInt();
        t.duration = (long) b.getFloat();
        t.delay = (long) b.getFloat();
        return t;
    }

    private void create(int id, int kind) {
        View v;
        switch (kind) {
            case 1: v = new TextBox(context, id); break;
            case 2: {
                ImageBox[] self = new ImageBox[1];
                self[0] = new ImageBox(context, id, (bitmap, error) -> imageDone(self[0], bitmap, error));
                v = self[0];
                break;
            }
            case 3: v = new ScrollBox(context, this, id); break;
            case 4: v = new InputBox(context, this, id); break;
            case 5: {
                StackBox stack = new StackBox(context, this, id);
                stacks.add(stack);
                v = stack;
                break;
            }
            case 6: v = new StackBox.ScreenBox(context, this, id); break;
            case 8: {
                headerItems.add(id);
                v = new Box(context, this, id);
                break;
            }
            case 7: {
                ModalHost modal = new ModalHost(context, this, id);
                modals.put(id, modal);
                v = modal.root;
                break;
            }
            case 9: v = new Controls.SwitchBox(context, this, id); break;
            case 10: v = new Controls.SliderBox(context, this, id); break;
            case 11: v = new Controls.ActivityBox(context, this, id); break;
            case 12: v = new WebBox(context, this, id); break;
            case 13: v = new VideoBox(context, this, host, id); break;
            default: v = new Box(context, this, id);
        }
        views.put(id, v);
        if (layoutAnimation != null) created.add(id);
    }

    private static ViewGroup container(View parent) {
        if (parent instanceof StackBox.ScreenBox) return ((StackBox.ScreenBox) parent).root;
        if (parent instanceof ScrollBox) return ((ScrollBox) parent).content;
        if (parent instanceof Box) return ((Box) parent).container();
        return null;
    }

    private void insert(int parentId, int childId, int beforeId) {
        if (insertHosted(parentId, childId, beforeId)) return;
        View parent = views.get(parentId), child = views.get(childId);
        ViewGroup group = container(parent);
        if (group == null || child == null) return;
        Animator running = layoutAnimators.get(childId);
        if (running != null) {
            // Back before its removal finished animating.
            running.cancel();
            Dynamic d = statics.get(childId);
            if (d != null) applyDynamic(child, d);
        }
        detach(child);
        int index = -1;
        if (beforeId >= 0) {
            View before = views.get(beforeId);
            if (before != null) index = group.indexOfChild(before);
        }
        group.addView(child, index);
        Box.place(child);
        if (parent instanceof ScrollBox && ((ScrollBox) parent).hasSticky()) ((ScrollBox) parent).frontSticky();
    }

    private static void detach(View v) {
        if (v == null) return;
        ViewParent p = v.getParent();
        if (p instanceof ViewGroup) ((ViewGroup) p).removeView(v);
    }

    /** Removes a view from its parent, fading it out under a layout animation. */
    private void remove(int id) {
        View v = views.get(id);
        Transition t = layoutAnimation != null ? layoutAnimation[2] : null;
        if (v == null || t == null || v.getParent() == null) {
            detach(v);
            return;
        }
        ValueAnimator a = ValueAnimator.ofFloat(0, 1);
        a.setDuration(t.duration);
        a.setStartDelay(t.delay);
        a.setInterpolator(t.interpolator());
        float alpha = v.getAlpha(), sx = v.getScaleX(), sy = v.getScaleY();
        a.addUpdateListener(x -> {
            float f = 1 - (float) x.getAnimatedValue();
            if (t.property == 1) {
                v.setScaleX(sx * f);
                v.setScaleY(sy * f);
            } else {
                v.setAlpha(alpha * f);
            }
        });
        a.addListener(new AnimatorListenerAdapter() {
            private boolean cancelled;

            @Override
            public void onAnimationCancel(Animator animation) {
                cancelled = true;
            }

            @Override
            public void onAnimationEnd(Animator animation) {
                if (layoutAnimators.get(id) == animation) layoutAnimators.remove(id);
                if (!cancelled) {
                    detach(v);
                    Dynamic d = statics.get(id);
                    if (d != null) applyDynamic(v, d);
                }
            }
        });
        layoutAnimators.put(id, a);
        a.start();
    }

    /** Fades or grows in the views a layout animation created. */
    private void appear(Transition t) {
        if (t == null) return;
        for (int id : created) {
            View v = views.get(id);
            if (v == null || v.getParent() == null) continue;
            float alpha = v.getAlpha(), sx = v.getScaleX(), sy = v.getScaleY();
            if (t.property == 1) {
                v.setScaleX(0);
                v.setScaleY(0);
            } else {
                v.setAlpha(0);
            }
            ValueAnimator a = ValueAnimator.ofFloat(0, 1);
            a.setDuration(t.duration);
            a.setStartDelay(t.delay);
            a.setInterpolator(t.interpolator());
            a.addUpdateListener(x -> {
                float f = (float) x.getAnimatedValue();
                if (t.property == 1) {
                    v.setScaleX(sx * f);
                    v.setScaleY(sy * f);
                } else {
                    v.setAlpha(alpha * f);
                }
            });
            a.start();
        }
    }

    private void delete(int id) {
        removeHosted(id);
        ModalHost modal = modals.get(id);
        if (modal != null) {
            modal.dismiss();
            modals.remove(id);
        }
        View v = views.get(id);
        if (v instanceof StackBox) stacks.remove(v);
        screenOptions.remove(id);
        Animator running = layoutAnimators.get(id);
        if (running != null) running.cancel();
        detach(v);
        if (v instanceof ImageBox) cancelImage((ImageBox) v);
        if (v instanceof WebBox) ((WebBox) v).destroy();
        if (v instanceof VideoBox) ((VideoBox) v).destroy();
        if (v == focused) focused = null;
        views.remove(id);
        headerItems.remove(id);
        headerSizes.remove(id);
        statics.remove(id);
        events.delete(id);
        slops.remove(id);
        a11y.remove(id);
    }

    private void frame(int id, float x, float y, float w, float h) {
        View v = views.get(id);
        if (v == null || id == 1) return;
        if (v instanceof StackBox.ScreenBox) {
            // The screen fills its stack; the core places its root view.
            Box root = ((StackBox.ScreenBox) v).root;
            Box.Frame f = Box.frame(root);
            f.left = 0;
            f.top = 0;
            f.right = px(w);
            f.bottom = px(h);
            Box.place(root);
            return;
        }
        if (headerItems.contains(id)) {
            // A header item: the toolbar places it at its size.
            headerSizes.put(id, new int[] {px(w), px(h)});
            if (v.getParent() instanceof android.widget.Toolbar) {
                ViewGroup.LayoutParams p = v.getLayoutParams();
                p.width = Math.max(1, px(w));
                p.height = Math.max(1, px(h));
                v.setLayoutParams(p);
            }
            return;
        }
        Box.Frame f = Box.frame(v);
        Transition t = layoutAnimation != null ? layoutAnimation[1] : null;
        boolean moves = t != null && !created.contains(id) && f.right > f.left && f.bottom > f.top;
        int[] from = {f.left, f.top, f.right, f.bottom};
        int[] to = {px(x), px(y), px(x + w), px(y + h)};
        Animator running = layoutAnimators.get(id);
        if (running != null && !moves) running.cancel();
        if (moves) {
            ValueAnimator a = ValueAnimator.ofFloat(0, 1);
            a.setDuration(t.duration);
            a.setStartDelay(t.delay);
            a.setInterpolator(t.interpolator());
            a.addUpdateListener(x2 -> {
                float k = (float) x2.getAnimatedValue();
                f.left = Math.round(from[0] + (to[0] - from[0]) * k);
                f.top = Math.round(from[1] + (to[1] - from[1]) * k);
                f.right = Math.round(from[2] + (to[2] - from[2]) * k);
                f.bottom = Math.round(from[3] + (to[3] - from[3]) * k);
                Box.place(v);
            });
            if (running != null) running.cancel();
            layoutAnimators.put(id, a);
            a.start();
        } else {
            f.left = to[0];
            f.top = to[1];
            f.right = to[2];
            f.bottom = to[3];
            Box.place(v);
        }
        if (v instanceof ImageBox) loadImage((ImageBox) v);
    }

    private Decor decor(View v) {
        if (v instanceof StackBox.ScreenBox) return ((StackBox.ScreenBox) v).root.decor;
        if (v instanceof StackBox) return ((StackBox) v).decor;
        if (v instanceof Box) return ((Box) v).decor;
        if (v instanceof TextBox) return ((TextBox) v).decor;
        if (v instanceof ImageBox) return ((ImageBox) v).decor;
        if (v instanceof ScrollBox) return ((ScrollBox) v).decor;
        if (v instanceof InputBox) return ((InputBox) v).decor;
        return null;
    }

    private void style(ByteBuffer b) {
        int id = b.getInt();
        View v = views.get(id);
        int background = b.getInt();
        float[] radii = {b.getFloat(), b.getFloat(), b.getFloat(), b.getFloat()};
        float[] borderWidths = {b.getFloat(), b.getFloat(), b.getFloat(), b.getFloat()};
        int[] borderColors = {b.getInt(), b.getInt(), b.getInt(), b.getInt()};
        int borderStyle = b.getInt();
        float opacity = b.getFloat();
        boolean overflow = b.getInt() != 0;
        int shadowColor = b.getInt();
        float sdx = b.getFloat(), sdy = b.getFloat(), sOpacity = b.getFloat(), sRadius = b.getFloat();
        Dynamic d = new Dynamic();
        d.tx = b.getFloat();
        d.ty = b.getFloat();
        d.sx = b.getFloat();
        d.sy = b.getFloat();
        d.rotate = b.getFloat();
        d.rotateX = b.getFloat();
        d.rotateY = b.getFloat();
        d.perspective = b.getFloat();
        boolean hasTint = b.getInt() != 0;
        int tint = b.getInt();
        d.opacity = opacity;
        d.background = background;
        if (v == null) return;
        statics.put(id, d);
        Decor decor = decor(v);
        if (decor != null) {
            for (int i = 0; i < 4; i++) {
                decor.radii[i] = radii[i] * density;
                decor.borderWidths[i] = borderWidths[i] * density;
                decor.borderColors[i] = borderColors[i];
            }
            decor.borderStyle = borderStyle;
            int alpha = Math.round(((shadowColor >>> 24) & 0xff) * Math.max(0, Math.min(1, sOpacity)));
            decor.shadowColor = (alpha << 24) | (shadowColor & 0x00ffffff);
            decor.shadowDx = sdx * density;
            decor.shadowDy = sdy * density;
            decor.shadowRadius = Math.max(0.01f, sRadius * density);
        }
        if (v instanceof Box) {
            ((Box) v).clip = overflow;
            ((Box) v).updateClip();
        } else if (v instanceof ScrollBox) {
            decor.clip(v, true);
        } else if (v instanceof TextBox || v instanceof InputBox) {
            decor.clip(v, overflow);
        }
        if (v instanceof ImageBox) ((ImageBox) v).setTint(hasTint ? tint : 0);
        applyDynamic(v, d);
        v.invalidate();
    }

    private void applyDynamic(View v, Dynamic d) {
        v.setAlpha(d.opacity);
        v.setTranslationX(d.tx * density);
        v.setTranslationY(d.ty * density);
        v.setScaleX(d.sx);
        v.setScaleY(d.sy);
        v.setRotation(d.rotate);
        v.setRotationX(d.rotateX);
        v.setRotationY(d.rotateY);
        setPerspective(v, d.perspective);
        Decor decor = decor(v);
        if (decor != null && decor.fill != d.background) {
            decor.fill = d.background;
            v.invalidate();
        }
    }

    /** `perspective` as a camera distance (React Native's normalization). */
    private void setPerspective(View v, float term) {
        if (term == 0) term = 0.00078125f;
        float distance = -1 / term;
        v.setCameraDistance(density * density * distance * (float) Math.sqrt(5));
    }

    /** The opacity a view returns to after a press (`activeOpacity`). */
    float baseOpacity(int id) {
        Dynamic d = statics.get(id);
        return d != null ? d.opacity : 1;
    }

    boolean hasHitSlop() {
        return slops.size() > 0;
    }

    float[] hitSlop(int id) {
        return slops.get(id);
    }

    private void viewProps(ByteBuffer b, Object[] objects) {
        int id = b.getInt();
        View v = views.get(id);
        int pointerEvents = b.getInt();
        float[] slop = {b.getFloat() * density, b.getFloat() * density, b.getFloat() * density, b.getFloat() * density};
        float activeOpacity = b.getFloat();
        A11y a = new A11y(this, id);
        a.accessible = b.getInt();
        a.label = string(objects, b.getInt());
        a.hint = string(objects, b.getInt());
        a.role = b.getInt();
        a.state = b.getInt();
        a.hasValue = b.getInt() != 0;
        if (a.hasValue) {
            a.min = b.getFloat();
            a.max = b.getFloat();
            a.now = b.getFloat();
            a.valueText = string(objects, b.getInt());
        }
        a.hidden = b.getInt() != 0;
        a.liveRegion = b.getInt();
        int actions = b.getInt();
        a.actionNames = new String[actions];
        a.actionLabels = new String[actions];
        for (int i = 0; i < actions; i++) {
            a.actionNames[i] = string(objects, b.getInt());
            a.actionLabels[i] = string(objects, b.getInt());
        }
        a.testId = string(objects, b.getInt());
        if (v == null) return;
        if (slop[0] != 0 || slop[1] != 0 || slop[2] != 0 || slop[3] != 0) slops.put(id, slop);
        else slops.remove(id);
        if (v instanceof Box) {
            Box box = (Box) v;
            box.pointerEvents = pointerEvents;
            box.activeOpacity = activeOpacity;
        }
        if (v instanceof TextBox) ((TextBox) v).a11y = a;
        a11y.put(id, a);
        a.apply(v);
    }

    private static String string(Object[] objects, int index) {
        return index >= 0 ? (String) objects[index] : null;
    }

    /** Applies `[animating, (view, prop, value)*]` from the animator. */
    void outputs(float[] out) {
        if (out == null || out.length == 0) return;
        for (int i = 1; i + 2 < out.length; i += 3) {
            View v = views.get(Float.floatToRawIntBits(out[i]));
            if (v == null) continue;
            float value = out[i + 2];
            switch ((int) out[i + 1]) {
                case 0: v.setAlpha(Math.max(0, Math.min(1, value))); break;
                case 1: v.setTranslationX(value * density); break;
                case 2: v.setTranslationY(value * density); break;
                case 3: v.setScaleX(value); v.setScaleY(value); break;
                case 4: v.setScaleX(value); break;
                case 5: v.setScaleY(value); break;
                case 6: v.setRotation(value); break;
                case 8: v.setRotationX(value); break;
                case 9: v.setRotationY(value); break;
                case 10: setPerspective(v, value); break;
                case 7: {
                    Decor decor = decor(v);
                    int color = Float.floatToRawIntBits(value);
                    if (decor != null && decor.fill != color) {
                        decor.fill = color;
                        v.invalidate();
                    }
                    break;
                }
                default: break;
            }
        }
        if (out[0] != 0) host.requestFrames();
    }

    // --- Navigation ----------------------------------------------------------

    /** Screens, header items and modals are not subviews of their parent. */
    private boolean insertHosted(int parentId, int childId, int beforeId) {
        View parent = views.get(parentId), child = views.get(childId);
        if (child instanceof StackBox.ScreenBox && parent instanceof StackBox) {
            StackBox stack = (StackBox) parent;
            stack.screens.remove((Integer) childId);
            int pos = beforeId >= 0 ? stack.screens.indexOf(beforeId) : -1;
            stack.screens.add(pos >= 0 ? pos : stack.screens.size(), childId);
            dirtyStacks.add(parentId);
        } else if (modals.get(childId) != null) {
            dirtyModals.add(childId);
        } else if (parent instanceof StackBox.ScreenBox && child instanceof Box && headerItem(childId)) {
            dirtyScreens.add(parentId);
        } else {
            return false;
        }
        hosts.put(childId, parentId);
        return true;
    }

    /** Header items are views created as kind 8; they stay `Box`es. */
    private final java.util.HashSet<Integer> headerItems = new java.util.HashSet<>();

    /** Sizes of header items (px), placed by toolbars. */
    final SparseArray<int[]> headerSizes = new SparseArray<>();

    private boolean headerItem(int id) {
        return headerItems.contains(id);
    }

    private boolean removeHosted(int id) {
        int parentId = hosts.get(id, -1);
        if (parentId < 0) return false;
        hosts.delete(id);
        View parent = views.get(parentId), child = views.get(id);
        if (parent instanceof StackBox) {
            StackBox stack = (StackBox) parent;
            stack.screens.remove((Integer) id);
            stack.popped.remove((Integer) id);
            dirtyStacks.add(parentId);
        } else if (modals.get(id) != null) {
            dirtyModals.add(id);
        } else if (child != null) {
            detach(child);
            dirtyScreens.add(parentId);
        }
        return true;
    }

    private void screenConfig(ByteBuffer b, Object[] objects) {
        int id = b.getInt();
        ScreenOptions o = new ScreenOptions();
        o.title = string(objects, b.getInt());
        o.shown = b.getInt() != 0;
        b.getInt(); // large titles: iOS only
        o.transparent = b.getInt() != 0;
        o.tint = b.getInt() != 0 ? b.getInt() : skip(b);
        o.background = b.getInt() != 0 ? b.getInt() : skip(b);
        o.titleColor = b.getInt() != 0 ? b.getInt() : skip(b);
        o.backVisible = b.getInt() != 0;
        o.gesture = b.getInt() != 0;
        o.animation = b.getInt();
        o.left = b.getInt();
        o.right = b.getInt();
        o.titleView = b.getInt();
        o.headerHeight = b.getFloat();
        screenOptions.put(id, o);
        dirtyScreens.add(id);
        int stack = hosts.get(id, -1);
        if (stack >= 0) dirtyStacks.add(stack);
    }

    private static int skip(ByteBuffer b) {
        b.getInt();
        return 0;
    }

    /** Pushes and pops screens, applies headers, shows and hides modals. */
    private void finishNavigation() {
        for (int id : dirtyStacks) {
            View v = views.get(id);
            if (v instanceof StackBox) {
                StackBox stack = (StackBox) v;
                for (int s : stack.screens) dirtyScreens.add(s);
                stack.reconcile();
            }
        }
        dirtyStacks.clear();
        for (int id : dirtyScreens) {
            View v = views.get(id);
            ScreenOptions o = screenOptions.get(id);
            if (!(v instanceof StackBox.ScreenBox) || o == null) continue;
            View stack = views.get(hosts.get(id, -1));
            boolean canGoBack = stack instanceof StackBox && ((StackBox) stack).screens.indexOf(id) > 0;
            ((StackBox.ScreenBox) v).configure(this, o.title, o.shown, o.transparent, o.tint, o.background,
                    o.titleColor, o.backVisible, o.gesture, o.animation, o.left, o.right, o.titleView,
                    o.headerHeight, canGoBack);
            reportScreenInsets(id, o);
        }
        dirtyScreens.clear();
        for (int id : dirtyModals) {
            ModalHost modal = modals.get(id);
            if (modal == null) continue;
            if (hosts.get(id, -1) >= 0) {
                modal.show();
                shownModals.remove(modal);
                shownModals.add(modal);
            } else {
                modal.dismiss();
                shownModals.remove(modal);
            }
        }
        dirtyModals.clear();
        updateBack();
    }

    /** A screen's insets below its header, for `useSafeAreaInsets()`. */
    private void reportScreenInsets(int id, ScreenOptions o) {
        float top = o.shown && !o.transparent ? 0 : host.safeTop();
        insets(id, top, host.safeRight(), host.safeBottom(), host.safeLeft());
    }

    /** Reports `onInsets` (dp): left / right packed in the count. */
    void insets(int id, float top, float right, float bottom, float left) {
        int packed = (Math.round(left) << 16) | (Math.round(right) & 0xffff);
        Native.event(handle(), id, Native.INSETS, null, packed, top, bottom);
    }

    void rootSize(int id, float w, float h, float top, float right, float bottom, float left) {
        Native.rootSize(handle(), id, w, h, top, right, bottom, left);
        insets(id, top, right, bottom, left);
    }

    /** Whether the back button means something to the app now. */
    boolean wantsBack() {
        if (!shownModals.isEmpty()) return true;
        for (StackBox s : stacks) {
            if (s.canPop()) return true;
        }
        return wants(1, Native.BACK_PRESS);
    }

    void updateBack() {
        host.setBackHandling(wantsBack());
    }

    boolean hasShownModal() {
        return !shownModals.isEmpty();
    }

    /** The stack the back button pops, if any. */
    StackBox backStack() {
        for (int i = stacks.size() - 1; i >= 0; i--) {
            if (stacks.get(i).canPop()) return stacks.get(i);
        }
        return null;
    }

    /** The back button: the newest modal asks to close, else the newest
     * stack pops, else JS hears about it. Returns false if nothing did. */
    boolean handleBack() {
        if (!shownModals.isEmpty()) {
            event(shownModals.get(shownModals.size() - 1).id, Native.REQUEST_CLOSE);
            return true;
        }
        StackBox stack = backStack();
        if (stack != null) {
            stack.pop();
            return true;
        }
        if (wants(1, Native.BACK_PRESS)) {
            event(1, Native.BACK_PRESS);
            return true;
        }
        return false;
    }

    // --- Images ------------------------------------------------------------

    private void image(int id, String uri, int mode) {
        View v = views.get(id);
        if (!(v instanceof ImageBox)) return;
        ImageBox image = (ImageBox) v;
        image.resizeMode = mode;
        if (uri == null ? image.uri == null : uri.equals(image.uri)) {
            image.invalidate();
            return;
        }
        cancelImage(image);
        image.setBitmap(null);
        image.uri = uri;
        loadImage(image);
    }

    private void loadImage(ImageBox image) {
        if (image.uri == null || image.requestedPx > 0) return;
        Box.Frame f = Box.frame(image);
        int maxPx = Math.max(f.right - f.left, f.bottom - f.top);
        if (maxPx <= 0) return;
        image.requestedPx = maxPx;
        Bitmap cached = images.request(image.uri, maxPx, image.callback);
        if (cached != null) {
            image.requestedPx = 0;
            image.setBitmap(cached);
            loaded(image, cached);
        }
    }

    private void cancelImage(ImageBox image) {
        if (image.uri != null && image.requestedPx > 0) images.cancel(image.uri, image.requestedPx, image.callback);
        image.requestedPx = 0;
    }

    private void imageDone(ImageBox image, Bitmap bitmap, String error) {
        image.requestedPx = 0;
        int id = image.id;
        if (views.get(id) != image) return;
        if (bitmap != null) {
            image.setBitmap(bitmap);
            loaded(image, bitmap);
        } else if (wants(id, Native.ERROR)) {
            event(id, Native.ERROR, error, 0, 0, 0);
        }
    }

    private void loaded(ImageBox image, Bitmap bitmap) {
        if (wants(image.id, Native.LOAD)) {
            event(image.id, Native.LOAD, null, 0, bitmap.getWidth() / density, bitmap.getHeight() / density);
        }
    }

    // --- Inputs and commands -------------------------------------------------

    private void input(ByteBuffer b, Object[] objects) {
        View v = views.get(b.getInt());
        float size = b.getFloat();
        int weight = b.getInt();
        boolean italic = b.getInt() != 0;
        int color = b.getInt();
        int familyIndex = b.getInt();
        float spacing = b.getFloat();
        int align = b.getInt();
        String placeholder = (String) objects[b.getInt()];
        boolean hasPlaceholderColor = b.getInt() != 0;
        int placeholderColor = b.getInt();
        boolean secure = b.getInt() != 0;
        int keyboard = b.getInt(), returnKey = b.getInt(), capitalize = b.getInt();
        boolean autoCorrect = b.getInt() != 0, multiline = b.getInt() != 0;
        int maxLength = b.getInt();
        boolean editable = b.getInt() != 0, autoFocus = b.getInt() != 0, hasSelection = b.getInt() != 0;
        int selection = b.getInt();
        boolean blurOnSubmit = b.getInt() != 0;
        if (!(v instanceof InputBox)) return;
        String family = familyIndex >= 0 ? (String) objects[familyIndex] : null;
        ((InputBox) v).configure(size, weight, italic, color, family, spacing, align, placeholder,
                hasPlaceholderColor, placeholderColor, secure, keyboard, returnKey, capitalize, autoCorrect,
                multiline, maxLength, editable, autoFocus, hasSelection, selection, blurOnSubmit);
    }

    private void command(ByteBuffer b, Object[] objects) {
        int id = b.getInt(), kind = b.getInt();
        View v = views.get(id);
        switch (kind) {
            case 0:
                if (v instanceof InputBox) ((InputBox) v).focus();
                break;
            case 1:
                if (v instanceof InputBox) {
                    ((InputBox) v).blur();
                } else if (id == 1) {
                    View f = host.findFocus();
                    if (f != null && f != host) {
                        InputMethodManager imm = (InputMethodManager) context.getSystemService(Context.INPUT_METHOD_SERVICE);
                        imm.hideSoftInputFromWindow(f.getWindowToken(), 0);
                        host.requestFocus();
                    }
                }
                break;
            case 2: {
                float x = b.getFloat(), y = b.getFloat();
                boolean animated = b.getInt() != 0;
                if (v instanceof ScrollBox) ((ScrollBox) v).scrollToPx(px(x), px(y), animated);
                break;
            }
            case 3: host.exitApp(); break;
            case 4: case 5: case 6: case 7:
                if (v instanceof WebBox) ((WebBox) v).command(kind, null);
                break;
            case 8: case 9: {
                String text = (String) objects[b.getInt()];
                if (v instanceof WebBox) ((WebBox) v).command(kind, text);
                break;
            }
            case 10: {
                float seconds = b.getFloat();
                if (v instanceof VideoBox) ((VideoBox) v).seek(seconds);
                break;
            }
            case 11: {
                int style = b.getInt();
                boolean hidden = b.getInt() != 0;
                b.getInt(); // animated: the system animates bar changes
                Integer background = optColor(b);
                SystemUi.statusBar(context, style, hidden, background);
                break;
            }
            case 12: {
                int dialog = b.getInt();
                String title = optString(b, objects), message = optString(b, objects);
                boolean cancelable = b.getInt() != 0, sheet = b.getInt() != 0;
                int n = b.getInt();
                String[] texts = new String[n];
                int[] styles = new int[n];
                for (int i = 0; i < n; i++) {
                    texts[i] = (String) objects[b.getInt()];
                    styles[i] = b.getInt();
                }
                SystemUi.dialog(this, dialog, title, message, cancelable, sheet, texts, styles);
                break;
            }
            case 13: {
                String text = (String) objects[b.getInt()];
                SystemUi.toast(context, text, b.getInt() != 0);
                break;
            }
            default: break;
        }
    }

    private static Integer optColor(ByteBuffer b) {
        boolean set = b.getInt() != 0;
        int color = b.getInt();
        return set ? color : null;
    }

    private static String optString(ByteBuffer b, Object[] objects) {
        int i = b.getInt();
        return i >= 0 ? (String) objects[i] : null;
    }

    /** `encode.rs` `native_props`. */
    private void nativeProps(ByteBuffer b, Object[] objects) {
        View v = views.get(b.getInt());
        switch (b.getInt()) {
            case 0: {
                boolean value = b.getInt() != 0, disabled = b.getInt() != 0;
                Integer on = optColor(b), off = optColor(b), thumb = optColor(b);
                if (v instanceof Controls.SwitchBox) ((Controls.SwitchBox) v).configure(value, disabled, on, off, thumb);
                break;
            }
            case 1: {
                float value = b.getFloat(), min = b.getFloat(), max = b.getFloat(), step = b.getFloat();
                boolean disabled = b.getInt() != 0;
                Integer minTrack = optColor(b), maxTrack = optColor(b), thumb = optColor(b);
                if (v instanceof Controls.SliderBox) {
                    ((Controls.SliderBox) v).configure(value, min, max, step, disabled, minTrack, maxTrack, thumb);
                }
                break;
            }
            case 2: {
                boolean animating = b.getInt() != 0;
                Integer color = optColor(b);
                b.getInt(); // large: the frame sizes it
                b.getInt(); // hidesWhenStopped: always hidden when stopped
                if (v instanceof Controls.ActivityBox) ((Controls.ActivityBox) v).configure(animating, color);
                break;
            }
            case 3: {
                Object[] source;
                switch (b.getInt()) {
                    case 1: source = new Object[] {1, objects[b.getInt()]}; break;
                    case 2: {
                        Object html = objects[b.getInt()];
                        source = new Object[] {2, html, optString(b, objects)};
                        break;
                    }
                    default: source = new Object[] {0};
                }
                String injected = optString(b, objects), before = optString(b, objects), ua = optString(b, objects);
                boolean scroll = b.getInt() != 0, js = b.getInt() != 0;
                if (v instanceof WebBox) ((WebBox) v).configure(source, injected, before, ua, scroll, js);
                break;
            }
            case 4: {
                String source = optString(b, objects);
                boolean paused = b.getInt() != 0, muted = b.getInt() != 0;
                float volume = b.getFloat(), rate = b.getFloat();
                boolean repeat = b.getInt() != 0;
                int mode = b.getInt();
                boolean controls = b.getInt() != 0;
                float interval = b.getFloat();
                if (v instanceof VideoBox) {
                    ((VideoBox) v).configure(source, paused, muted, volume, rate, repeat, mode, controls, interval);
                }
                break;
            }
            default: throw new IllegalStateException("yf: unknown native props");
        }
    }

    void inputFocused(InputBox input, boolean focused) {
        if (focused) {
            this.focused = input;
            host.post(this::revealFocused);
        } else if (this.focused == input) {
            this.focused = null;
        }
    }

    /** The scroll view padded to clear the keyboard. */
    private int avoided = -1;

    /**
     * The software keyboard's top edge moved, in screen coordinates
     * ({@code Integer.MAX_VALUE} when hidden): pads the scroll view around
     * the focused input so it can clear the keyboard (in the surface or a
     * modal window).
     */
    void keyboard(int screenTop) {
        ScrollBox scroll = focused == null ? null : scrollAround(focused);
        boolean hidden = screenTop == Integer.MAX_VALUE;
        if (avoided >= 0) {
            View previous = views.get(avoided);
            if (previous instanceof ScrollBox && (previous != scroll || hidden)) {
                ((ScrollBox) previous).setContentPaddingBottom(0);
            }
            avoided = -1;
        }
        if (scroll == null || hidden) return;
        int[] at = new int[2];
        scroll.getLocationOnScreen(at);
        int covered = Math.max(0, at[1] + scroll.getHeight() - screenTop);
        scroll.setContentPaddingBottom(covered);
        avoided = scroll.id;
        host.post(this::revealFocused);
    }

    private void revealFocused() {
        if (focused == null) return;
        int margin = px(12);
        ScrollBox scroll = scrollAround(focused);
        if (scroll != null) {
            scroll.reveal(focused, margin);
        } else {
            Rect r = new Rect(0, -margin, focused.getWidth(), focused.getHeight() + margin);
            focused.requestRectangleOnScreen(r, false);
        }
    }

    private ScrollBox scrollAround(View v) {
        ViewParent p = v.getParent();
        while (p != null) {
            if (p instanceof ScrollBox) return (ScrollBox) p;
            p = p.getParent();
        }
        return null;
    }

    // --- Events --------------------------------------------------------------

    boolean wants(int id, int kind) {
        return (events.get(id) & (1L << kind)) != 0;
    }

    boolean hasPan(int id) {
        return Native.hasPan(handle(), id);
    }

    void event(int id, int kind) {
        Native.event(handle(), id, kind, null, 0, 0, 0);
    }

    /** An event whose payload is one {@code Boolean}, {@code Double} or {@code String}. */
    void eventValue(int id, int kind, Object value) {
        Native.eventMap(handle(), id, kind, null, new Object[] {value});
    }

    /** An event with an object payload. */
    void eventMap(int id, int kind, String[] keys, Object[] values) {
        Native.eventMap(handle(), id, kind, keys, values);
    }

    void event(int id, int kind, String text, int count, float x, float y) {
        Native.event(handle(), id, kind, text, count, x, y);
    }

    void scroll(int id, float x, float y, int applied) {
        outputs(Native.scroll(handle(), id, x, y, applied));
    }

    void pan(int id, int state, float tx, float ty, float vx, float vy) {
        outputs(Native.pan(handle(), id, state, tx, ty, vx, vy));
    }

    void transformGesture(int id, boolean rotation, int state, float amount, float velocity) {
        outputs(Native.transformGesture(handle(), id, rotation, state, amount, velocity));
    }
}
