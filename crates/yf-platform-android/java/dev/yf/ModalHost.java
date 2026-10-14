package dev.yf;

import android.animation.Animator;
import android.animation.AnimatorListenerAdapter;
import android.annotation.SuppressLint;
import android.app.Dialog;
import android.content.Context;
import android.graphics.drawable.ColorDrawable;
import android.os.Build;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager;
import android.view.animation.DecelerateInterpolator;

/**
 * `<modal>`: content presented in its own dialog window over the app. The
 * core lays the content out at the size this window reports.
 */
final class ModalHost {
    final Mounter mounter;
    final int id;
    /** The modal's root view, placed by the core. */
    final Box root;
    private final Container container;
    private Dialog dialog;
    int animation; // 0 slide, 1 fade, 2 none
    boolean transparent;
    private boolean closing;

    ModalHost(Context context, Mounter mounter, int id) {
        this.mounter = mounter;
        this.id = id;
        root = new Box(context, mounter, id);
        container = new Container(context);
        container.addView(root);
        mirror(mounter.rtl);
    }

    /** Lays the modal's widgets out right to left (or not). */
    void mirror(boolean rtl) {
        container.setLayoutDirection(rtl ? View.LAYOUT_DIRECTION_RTL : View.LAYOUT_DIRECTION_LTR);
    }

    TouchTracker touches() {
        return container.touches;
    }

    boolean shown() {
        return dialog != null && !closing;
    }

    void show() {
        if (dialog != null && !closing) return;
        closing = false;
        Context context = mounter.context;
        dialog = new Dialog(context, android.R.style.Theme_DeviceDefault_Light_NoActionBar);
        if (container.getParent() instanceof ViewGroup) ((ViewGroup) container.getParent()).removeView(container);
        dialog.setContentView(container);
        Window window = dialog.getWindow();
        if (window != null) {
            window.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT);
            window.setBackgroundDrawable(new ColorDrawable(0));
            window.setWindowAnimations(0);
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
            window.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS);
            window.setStatusBarColor(0);
            window.setNavigationBarColor(0);
            if (Build.VERSION.SDK_INT >= 30) window.setDecorFitsSystemWindows(false);
            else window.getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
        container.setBackgroundColor(transparent ? 0 : 0xffffffff);
        dialog.setCancelable(false);
        dialog.setOnKeyListener((d, keyCode, event) -> {
            if (keyCode == KeyEvent.KEYCODE_BACK && event.getAction() == KeyEvent.ACTION_UP) {
                requestClose();
                return true;
            }
            return keyCode == KeyEvent.KEYCODE_BACK;
        });
        if (Build.VERSION.SDK_INT >= 33) {
            dialog.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                    android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, this::requestClose);
        }
        dialog.show();
        container.setAlpha(animation == 1 ? 0 : 1);
        container.post(() -> {
            if (animation == 2) {
                mounter.event(id, Native.SHOW);
                return;
            }
            if (animation == 0) container.setTranslationY(container.getHeight());
            container.animate().alpha(1).translationY(0).setDuration(300)
                    .setInterpolator(new DecelerateInterpolator(1.5f))
                    .setListener(new AnimatorListenerAdapter() {
                        @Override
                        public void onAnimationEnd(Animator a) {
                            container.animate().setListener(null);
                            mounter.event(id, Native.SHOW);
                        }
                    }).start();
        });
    }

    /** The back button: the app decides (`visible`). */
    private void requestClose() {
        mounter.event(id, Native.REQUEST_CLOSE);
    }

    void dismiss() {
        if (dialog == null || closing) return;
        closing = true;
        Dialog d = dialog;
        Runnable done = () -> {
            d.dismiss();
            if (dialog == d) dialog = null;
            closing = false;
            mounter.event(id, Native.DISMISS);
        };
        if (animation == 2) {
            done.run();
            return;
        }
        container.animate()
                .alpha(animation == 1 ? 0 : 1)
                .translationY(animation == 0 ? container.getHeight() : 0)
                .setDuration(250)
                .setListener(new AnimatorListenerAdapter() {
                    @Override
                    public void onAnimationEnd(Animator a) {
                        container.animate().setListener(null);
                        done.run();
                    }
                }).start();
    }

    /** Reports the window's size and insets: the content is laid out at them. */
    @SuppressLint("ViewConstructor")
    private final class Container extends ViewGroup {
        private int[] reported = {0, 0, 0, 0, 0, 0};

        private final TouchTracker touches;

        Container(Context context) {
            super(context);
            touches = new TouchTracker(mounter, this);
            mounter.touchTrackers.add(touches);
        }

        @Override
        public boolean dispatchTouchEvent(android.view.MotionEvent e) {
            boolean tracking = touches.onTouch(e);
            return super.dispatchTouchEvent(e) || tracking;
        }

        @Override
        protected void onMeasure(int widthSpec, int heightSpec) {
            setMeasuredDimension(MeasureSpec.getSize(widthSpec), MeasureSpec.getSize(heightSpec));
            Box.Frame f = Box.frame(root);
            root.measure(MeasureSpec.makeMeasureSpec(f.right - f.left, MeasureSpec.EXACTLY),
                    MeasureSpec.makeMeasureSpec(f.bottom - f.top, MeasureSpec.EXACTLY));
        }

        @Override
        protected void onLayout(boolean changed, int l, int t, int r, int b) {
            Box.Frame f = Box.frame(root);
            root.layout(f.left, f.top, f.right, f.bottom);
            report();
        }

        private int keyboard;

        @Override
        public WindowInsets onApplyWindowInsets(WindowInsets insets) {
            report();
            if (Build.VERSION.SDK_INT >= 30) {
                int ime = insets.getInsets(WindowInsets.Type.ime()).bottom;
                if (ime != keyboard) {
                    keyboard = ime;
                    int[] at = new int[2];
                    getLocationOnScreen(at);
                    mounter.keyboard(ime > 0 ? at[1] + getHeight() - ime : Integer.MAX_VALUE);
                }
            }
            return super.onApplyWindowInsets(insets);
        }

        private void report() {
            WindowInsets insets = getRootWindowInsets();
            int top = 0, right = 0, bottom = 0, left = 0;
            if (insets != null) {
                if (Build.VERSION.SDK_INT >= 30) {
                    android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                    top = bars.top;
                    right = bars.right;
                    bottom = bars.bottom;
                    left = bars.left;
                } else {
                    top = insets.getSystemWindowInsetTop();
                    right = insets.getSystemWindowInsetRight();
                    bottom = insets.getSystemWindowInsetBottom();
                    left = insets.getSystemWindowInsetLeft();
                }
            }
            int[] now = {getWidth(), getHeight(), top, right, bottom, left};
            if (now[0] == 0 || java.util.Arrays.equals(now, reported)) return;
            reported = now;
            float d = mounter.density;
            mounter.rootSize(id, now[0] / d, now[1] / d, top / d, right / d, bottom / d, left / d);
        }
    }
}
