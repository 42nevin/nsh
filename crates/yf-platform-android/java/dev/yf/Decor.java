package dev.yf;

import android.graphics.Canvas;
import android.graphics.ColorFilter;
import android.graphics.DashPathEffect;
import android.graphics.Outline;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PixelFormat;
import android.graphics.Rect;
import android.graphics.RectF;
import android.graphics.drawable.Drawable;
import android.os.Build;
import android.view.View;
import android.view.ViewOutlineProvider;

/**
 * Background, border, rounded corners and drop shadow of a view, drawn as
 * its background drawable. Units are px.
 */
final class Decor extends Drawable {
    int fill;
    /** Top left, top right, bottom right, bottom left. */
    final float[] radii = new float[4];
    /** Top, right, bottom, left (px). */
    final float[] borderWidths = new float[4];
    /** Top, right, bottom, left. */
    final int[] borderColors = new int[4];
    /** 0 solid, 1 dashed, 2 dotted. */
    int borderStyle;
    int shadowColor;
    float shadowDx;
    float shadowDy;
    float shadowRadius;

    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
    private final Path path = new Path();
    private final Path inner = new Path();
    private final Path side = new Path();
    private final RectF rect = new RectF();
    private final float[] corners = new float[8];

    boolean hasShadow() {
        return Color.alpha(shadowColor) > 0 && Color.alpha(fill) > 0;
    }

    boolean uniformRadius() {
        return radii[0] == radii[1] && radii[1] == radii[2] && radii[2] == radii[3];
    }

    boolean rounded() {
        return radii[0] > 0 || radii[1] > 0 || radii[2] > 0 || radii[3] > 0;
    }

    /** The rounded outline of `bounds`, inset by `inset`. */
    Path shape(Rect bounds, float inset, Path out) {
        rect.set(bounds.left + inset, bounds.top + inset, bounds.right - inset, bounds.bottom - inset);
        for (int i = 0; i < 4; i++) {
            float r = Math.max(0, fit(bounds)[i] - inset);
            corners[i * 2] = r;
            corners[i * 2 + 1] = r;
        }
        out.reset();
        out.addRoundRect(rect, corners, Path.Direction.CW);
        return out;
    }

    private final float[] fitted = new float[4];

    /** Radii scaled down so adjacent corners do not overlap (CSS). */
    private float[] fit(Rect b) {
        float w = b.width(), h = b.height();
        float f = 1;
        f = Math.min(f, ratio(w, radii[0], radii[1]));
        f = Math.min(f, ratio(w, radii[3], radii[2]));
        f = Math.min(f, ratio(h, radii[0], radii[3]));
        f = Math.min(f, ratio(h, radii[1], radii[2]));
        for (int i = 0; i < 4; i++) fitted[i] = radii[i] * f;
        return fitted;
    }

    private static float ratio(float side, float a, float b) {
        return a + b > side ? side / (a + b) : 1;
    }

    @Override
    public void draw(Canvas canvas) {
        Rect b = getBounds();
        if (Color.alpha(fill) > 0) {
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(fill);
            if (hasShadow()) paint.setShadowLayer(shadowRadius, shadowDx, shadowDy, shadowColor);
            else paint.clearShadowLayer();
            canvas.drawPath(shape(b, 0, path), paint);
            paint.clearShadowLayer();
        }
        if (!hasBorder()) return;
        if (uniformBorder()) {
            float w = borderWidths[0];
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(w);
            paint.setColor(borderColors[0]);
            if (borderStyle == 1) {
                paint.setPathEffect(new DashPathEffect(new float[] {w * 3, w * 3}, 0));
                paint.setStrokeCap(Paint.Cap.BUTT);
            } else if (borderStyle == 2) {
                // Zero-length dashes with round caps draw dots.
                paint.setPathEffect(new DashPathEffect(new float[] {0, w * 2}, 0));
                paint.setStrokeCap(Paint.Cap.ROUND);
            }
            canvas.drawPath(shape(b, w / 2, path), paint);
            paint.setPathEffect(null);
            paint.setStrokeCap(Paint.Cap.BUTT);
            return;
        }
        // Sides differ: fill the ring between the outer and inner edges, one
        // side's trapezoid at a time.
        ring(b);
        paint.setStyle(Paint.Style.FILL);
        float w = b.width(), h = b.height();
        float t = borderWidths[0], r = borderWidths[1], bo = borderWidths[2], l = borderWidths[3];
        float[][] sides = {
            {0, 0, w, 0, w - r, t, l, t},
            {w, 0, w, h, w - r, h - bo, w - r, t},
            {w, h, 0, h, l, h - bo, w - r, h - bo},
            {0, h, 0, 0, l, t, l, h - bo},
        };
        for (int i = 0; i < 4; i++) {
            if (borderWidths[i] <= 0 || Color.alpha(borderColors[i]) == 0) continue;
            float[] p = sides[i];
            side.reset();
            side.moveTo(b.left + p[0], b.top + p[1]);
            for (int k = 2; k < 8; k += 2) side.lineTo(b.left + p[k], b.top + p[k + 1]);
            side.close();
            canvas.save();
            canvas.clipPath(side);
            paint.setColor(borderColors[i]);
            canvas.drawPath(path, paint);
            canvas.restore();
        }
    }

    boolean hasBorder() {
        for (int i = 0; i < 4; i++) {
            if (borderWidths[i] > 0 && Color.alpha(borderColors[i]) > 0) return true;
        }
        return false;
    }

    private boolean uniformBorder() {
        for (int i = 1; i < 4; i++) {
            if (borderWidths[i] != borderWidths[0] || borderColors[i] != borderColors[0]) return false;
        }
        return true;
    }

    private final float[] innerRadii = new float[8];

    /** The area between the outer edge and the inner edge of the borders,
     * into `path` (even-odd). Inner corners are elliptical where adjacent
     * sides differ. */
    private void ring(Rect b) {
        shape(b, 0, path);
        float t = borderWidths[0], r = borderWidths[1], bo = borderWidths[2], l = borderWidths[3];
        float[] radii = fit(b);
        float[][] corner = {{l, t}, {r, t}, {r, bo}, {l, bo}};
        for (int i = 0; i < 4; i++) {
            innerRadii[i * 2] = Math.max(0, radii[i] - corner[i][0]);
            innerRadii[i * 2 + 1] = Math.max(0, radii[i] - corner[i][1]);
        }
        rect.set(b.left + l, b.top + t, b.right - r, b.bottom - bo);
        inner.reset();
        if (rect.width() > 0 && rect.height() > 0) inner.addRoundRect(rect, innerRadii, Path.Direction.CW);
        path.setFillType(Path.FillType.EVEN_ODD);
        path.addPath(inner);
    }

    /** Clips `view` to the rounded shape (outline clipping where possible). */
    void clip(View view, boolean clip) {
        view.setClipToOutline(clip && (uniformRadius() || Build.VERSION.SDK_INT >= 33));
        view.setOutlineProvider(new ViewOutlineProvider() {
            @Override
            public void getOutline(View v, Outline outline) {
                Rect r = new Rect(0, 0, v.getWidth(), v.getHeight());
                if (uniformRadius()) {
                    outline.setRoundRect(r, fit(r)[0]);
                } else if (Build.VERSION.SDK_INT >= 30) {
                    outline.setPath(shape(r, 0, new Path()));
                } else {
                    outline.setRect(r);
                }
            }
        });
        view.invalidateOutline();
    }

    /** Odd corners before API 33 cannot clip by outline: clip in dispatchDraw. */
    boolean needsPathClip(boolean clip) {
        return clip && !uniformRadius() && Build.VERSION.SDK_INT < 33;
    }

    @Override
    public void setAlpha(int alpha) {}

    @Override
    public void setColorFilter(ColorFilter filter) {}

    @Override
    @SuppressWarnings("deprecation")
    public int getOpacity() {
        return PixelFormat.TRANSLUCENT;
    }

    /** android.graphics.Color without the import clash in callers. */
    static final class Color {
        static int alpha(int c) {
            return c >>> 24;
        }
    }
}
