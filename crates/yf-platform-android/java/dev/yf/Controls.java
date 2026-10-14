package dev.yf;

import android.annotation.SuppressLint;
import android.content.Context;
import android.content.res.ColorStateList;
import android.view.View;
import android.widget.CompoundButton;
import android.widget.ProgressBar;
import android.widget.SeekBar;
import android.widget.Switch;

/** `<switch>`, `<slider>` and `<activityindicator>`: platform widgets filling a box. */
final class Controls {
    private Controls() {}

    private static final int[][] CHECKED = {{android.R.attr.state_checked}, {}};

    /**
     * Widgets' default colors come from their drawables, and a null tint list
     * clears them: tints are left alone until a color is set, and a removed
     * color falls back to the theme's.
     */
    static final class Tint {
        private final android.view.View view;
        private final int themeAttr;
        private boolean set;

        Tint(android.view.View view, int themeAttr) {
            this.view = view;
            this.themeAttr = themeAttr;
        }

        /** The tint list to apply for {@code color}, or null to leave it. */
        ColorStateList resolve(Integer color) {
            if (color != null) {
                set = true;
                return ColorStateList.valueOf(color);
            }
            if (!set) return null;
            set = false;
            android.util.TypedValue v = new android.util.TypedValue();
            view.getContext().getTheme().resolveAttribute(themeAttr, v, true);
            return v.resourceId != 0
                    ? view.getContext().getColorStateList(v.resourceId)
                    : ColorStateList.valueOf(v.data);
        }
    }

    /** A box whose single native widget fills it (the core lays out no children). */
    @SuppressLint("ViewConstructor")
    static class WidgetBox extends Box {
        final View widget;

        WidgetBox(Context context, Mounter mounter, int id, View widget) {
            super(context, mounter, id);
            this.widget = widget;
            addView(widget);
        }

        @Override
        protected void onSizeChanged(int w, int h, int oldw, int oldh) {
            super.onSizeChanged(w, h, oldw, oldh);
            Box.Frame f = Box.frame(widget);
            f.left = 0;
            f.top = 0;
            f.right = w;
            f.bottom = h;
            Box.place(widget);
        }
    }

    @SuppressLint("ViewConstructor")
    static final class SwitchBox extends WidgetBox implements CompoundButton.OnCheckedChangeListener {
        private final Switch toggle;
        private final Tint track, thumbTint;
        private boolean applying;

        SwitchBox(Context context, Mounter mounter, int id) {
            super(context, mounter, id, new Switch(context));
            toggle = (Switch) widget;
            toggle.setOnCheckedChangeListener(this);
            track = new Tint(toggle, android.R.attr.colorControlNormal);
            thumbTint = new Tint(toggle, android.R.attr.colorControlActivated);
        }

        void configure(boolean value, boolean disabled, Integer trackOn, Integer trackOff, Integer thumb) {
            applying = true;
            toggle.setChecked(value);
            applying = false;
            toggle.setEnabled(!disabled);
            if (trackOn != null || trackOff != null) {
                track.resolve(0);
                int on = trackOn != null ? trackOn : 0xff009688, off = trackOff != null ? trackOff : 0xff9e9e9e;
                toggle.setTrackTintList(new ColorStateList(CHECKED, new int[] {on, off}));
            } else {
                ColorStateList t = track.resolve(null);
                if (t != null) toggle.setTrackTintList(t);
            }
            ColorStateList t = thumbTint.resolve(thumb);
            if (t != null) toggle.setThumbTintList(t);
        }

        @Override
        public void onCheckedChanged(CompoundButton button, boolean checked) {
            // The core reverts it unless JS keeps the new value (controlled).
            if (!applying) mounter.eventValue(id, Native.VALUE_CHANGE, checked);
        }
    }

    @SuppressLint("ViewConstructor")
    static final class SliderBox extends WidgetBox implements SeekBar.OnSeekBarChangeListener {
        /** Positions of a continuous slider. */
        private static final int RESOLUTION = 10000;
        private final SeekBar bar;
        private final Tint progressTint, backgroundTint, thumbTint;
        private float min, max = 1, step;
        private boolean tracking;

        SliderBox(Context context, Mounter mounter, int id) {
            super(context, mounter, id, new SeekBar(context));
            bar = (SeekBar) widget;
            bar.setOnSeekBarChangeListener(this);
            progressTint = new Tint(bar, android.R.attr.colorControlActivated);
            backgroundTint = new Tint(bar, android.R.attr.colorControlNormal);
            thumbTint = new Tint(bar, android.R.attr.colorControlActivated);
        }

        private float increment() {
            return step > 0 ? step : (max - min) / RESOLUTION;
        }

        private float value() {
            return Math.min(max, min + bar.getProgress() * increment());
        }

        void configure(float value, float min, float max, float step, boolean disabled,
                Integer minTrack, Integer maxTrack, Integer thumb) {
            this.min = min;
            this.max = Math.max(min, max);
            this.step = step;
            float inc = increment();
            bar.setMax(inc > 0 ? Math.round((this.max - min) / inc) : 0);
            if (!tracking && inc > 0) bar.setProgress(Math.round((value - min) / inc));
            bar.setEnabled(!disabled);
            ColorStateList t = progressTint.resolve(minTrack);
            if (t != null) bar.setProgressTintList(t);
            if ((t = backgroundTint.resolve(maxTrack)) != null) bar.setProgressBackgroundTintList(t);
            if ((t = thumbTint.resolve(thumb)) != null) bar.setThumbTintList(t);
        }

        @Override
        public void onProgressChanged(SeekBar seekBar, int progress, boolean fromUser) {
            if (fromUser) mounter.eventValue(id, Native.VALUE_CHANGE, (double) value());
        }

        @Override
        public void onStartTrackingTouch(SeekBar seekBar) {
            tracking = true;
        }

        @Override
        public void onStopTrackingTouch(SeekBar seekBar) {
            tracking = false;
            double v = value();
            mounter.eventValue(id, Native.VALUE_CHANGE, v);
            if (mounter.wants(id, Native.SLIDING_COMPLETE)) mounter.eventValue(id, Native.SLIDING_COMPLETE, v);
        }
    }

    @SuppressLint("ViewConstructor")
    static final class ActivityBox extends WidgetBox {
        private final ProgressBar spinner;
        private final Tint tint;

        ActivityBox(Context context, Mounter mounter, int id) {
            super(context, mounter, id, new ProgressBar(context));
            spinner = (ProgressBar) widget;
            spinner.setIndeterminate(true);
            tint = new Tint(spinner, android.R.attr.colorControlActivated);
        }

        void configure(boolean animating, Integer color) {
            ColorStateList t = tint.resolve(color);
            if (t != null) spinner.setIndeterminateTintList(t);
            // A stopped indeterminate spinner cannot be drawn still: like
            // React Native on Android, it is hidden.
            spinner.setVisibility(animating ? View.VISIBLE : View.INVISIBLE);
        }
    }
}
