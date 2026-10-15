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

    // Box shadows (px): dx, dy, blur, spread, inset (1) per layer, top
    // layer first (CSS order), and their colors.
    float[] boxShadows = new float[0];
    int[] boxShadowColors = new int[0];
    // Gradients, top layer first.
    Gradient[] gradients = new Gradient[0];
    // `outline*` (px): width (0: none), color, style (0 solid, 1 dashed, 2
    // dotted), offset.
    float outlineWidth, outlineOffset;
    int outlineColor, outlineStyle;

    /** A gradient background (`encode.rs` `effects`). */
    static final class Gradient {
        /** 0 linear (angle), 1 to a corner, 2 radial, 3 conic. */
        int kind;
        float angle;
        boolean right, bottom, circle;
        /** Radial: 0 closest side, 1 closest corner, 2 farthest side, 3 farthest corner. */
        int size;
        float x = 0.5f, y = 0.5f;
        int[] colors;
        float[] positions;
    }

    /** Distance shadow casters are moved away to draw only their shadows. */
    private static final float AWAY = 1 << 14;
    private final Path shadowPath = new Path();

    @Override
    public void draw(Canvas canvas) {
        Rect b = getBounds();
        drawOuterShadows(canvas, b);
        if (Color.alpha(fill) > 0) {
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(fill);
            if (hasShadow()) paint.setShadowLayer(shadowRadius, shadowDx, shadowDy, shadowColor);
            else paint.clearShadowLayer();
            canvas.drawPath(shape(b, 0, path), paint);
            paint.clearShadowLayer();
        }
        drawGradients(canvas, b);
        drawInsetShadows(canvas, b);
        if (!bordersAbove) drawBorders(canvas);
        drawOutline(canvas, b);
    }

    /** CSS blur (twice the deviation) as a Skia shadow radius. */
    private static float shadowRadius(float blur) {
        // Skia: sigma = radius / sqrt(3) + 0.5.
        return Math.max(0.01f, (blur / 2 - 0.5f) * 1.7320508f);
    }

    /** Outer box shadows: only outside the box, as CSS draws them. */
    private void drawOuterShadows(Canvas canvas, Rect b) {
        boolean any = false;
        for (int n = 0; n < boxShadowColors.length; n++) any |= boxShadows[n * 5 + 4] == 0 && Color.alpha(boxShadowColors[n]) > 0;
        if (!any) return;
        canvas.save();
        if (android.os.Build.VERSION.SDK_INT >= 26) canvas.clipOutPath(shape(b, 0, path));
        paint.setStyle(Paint.Style.FILL);
        // Bottom layer first.
        for (int n = boxShadowColors.length - 1; n >= 0; n--) {
            int i = n * 5;
            if (boxShadows[i + 4] != 0) continue;
            int color = boxShadowColors[n];
            if (Color.alpha(color) == 0) continue;
            float spread = boxShadows[i + 3];
            // The caster sits far away; only its shadow lands here.
            paint.setColor(0xff000000);
            paint.setShadowLayer(shadowRadius(boxShadows[i + 2]), boxShadows[i] + AWAY, boxShadows[i + 1], color);
            shadowPath.reset();
            grown(b, spread, shadowPath);
            shadowPath.offset(-AWAY, 0);
            canvas.drawPath(shadowPath, paint);
        }
        paint.clearShadowLayer();
        canvas.restore();
    }

    /** Inset box shadows: inside the padding box, above the background. */
    private void drawInsetShadows(Canvas canvas, Rect b) {
        boolean any = false;
        for (int n = 0; n < boxShadowColors.length; n++) any |= boxShadows[n * 5 + 4] != 0 && Color.alpha(boxShadowColors[n]) > 0;
        if (!any) return;
        float t = borderWidths[0], r = borderWidths[1], bo = borderWidths[2], l = borderWidths[3];
        Rect pad = new Rect(Math.round(b.left + l), Math.round(b.top + t), Math.round(b.right - r), Math.round(b.bottom - bo));
        canvas.save();
        canvas.clipPath(shape(pad, 0, path));
        paint.setStyle(Paint.Style.FILL);
        for (int n = boxShadowColors.length - 1; n >= 0; n--) {
            int i = n * 5;
            if (boxShadows[i + 4] == 0) continue;
            int color = boxShadowColors[n];
            if (Color.alpha(color) == 0) continue;
            float dx = boxShadows[i], dy = boxShadows[i + 1], blur = boxShadows[i + 2], spread = boxShadows[i + 3];
            // A frame around the shifted, shrunk inner shape casts the shadow
            // inwards.
            float reach = Math.abs(dx) + Math.abs(dy) + Math.abs(spread) + blur * 2 + 4;
            shadowPath.reset();
            shadowPath.setFillType(Path.FillType.EVEN_ODD);
            shadowPath.addRect(pad.left - reach, pad.top - reach, pad.right + reach, pad.bottom + reach, Path.Direction.CW);
            Path hole = new Path();
            grown(pad, -spread, hole);
            hole.offset(dx, dy);
            shadowPath.addPath(hole);
            shadowPath.offset(-AWAY, 0);
            paint.setColor(0xff000000);
            paint.setShadowLayer(shadowRadius(blur), AWAY, 0, color);
            canvas.drawPath(shadowPath, paint);
        }
        paint.clearShadowLayer();
        canvas.restore();
    }

    /** The box's rounded shape grown by `d` px (corners grow with it). */
    private void grown(Rect b, float d, Path out) {
        float[] fitted = fit(b);
        rect.set(b.left - d, b.top - d, b.right + d, b.bottom + d);
        for (int i = 0; i < 4; i++) {
            float r = fitted[i] > 0 ? Math.max(0, fitted[i] + d) : 0;
            corners[i * 2] = r;
            corners[i * 2 + 1] = r;
        }
        out.addRoundRect(rect, corners, Path.Direction.CW);
    }

    private final android.graphics.Matrix gradientMatrix = new android.graphics.Matrix();

    private void drawGradients(Canvas canvas, Rect b) {
        if (gradients.length == 0) return;
        float w = Math.max(1, b.width()), h = Math.max(1, b.height());
        paint.setStyle(Paint.Style.FILL);
        Path box = shape(b, 0, path);
        for (int i = gradients.length - 1; i >= 0; i--) {
            Gradient g = gradients[i];
            android.graphics.Shader shader;
            switch (g.kind) {
                case 2: {
                    float cx = b.left + g.x * w, cy = b.top + g.y * h;
                    float ax = g.x * w, ay = g.y * h;
                    float dx, dy;
                    switch (g.size) {
                        case 0: dx = Math.min(ax, w - ax); dy = Math.min(ay, h - ay); break;
                        case 2: dx = Math.max(ax, w - ax); dy = Math.max(ay, h - ay); break;
                        case 1: dx = Math.min(ax, w - ax) * 1.4142135f; dy = Math.min(ay, h - ay) * 1.4142135f; break;
                        default: dx = Math.max(ax, w - ax) * 1.4142135f; dy = Math.max(ay, h - ay) * 1.4142135f;
                    }
                    if (g.circle) {
                        float r = g.size == 0 ? Math.min(dx, dy) : g.size == 2 ? Math.max(dx, dy)
                                : (float) Math.hypot(dx / 1.4142135f, dy / 1.4142135f);
                        dx = dy = r;
                    }
                    shader = new android.graphics.RadialGradient(cx, cy, Math.max(0.5f, dx), g.colors, g.positions,
                            android.graphics.Shader.TileMode.CLAMP);
                    // An ellipse: the circle scaled vertically.
                    gradientMatrix.setScale(1, dy / Math.max(0.5f, dx), cx, cy);
                    shader.setLocalMatrix(gradientMatrix);
                    break;
                }
                case 3: {
                    float cx = b.left + g.x * w, cy = b.top + g.y * h;
                    shader = new android.graphics.SweepGradient(cx, cy, g.colors, g.positions);
                    // Sweeps start right; CSS starts up.
                    gradientMatrix.setRotate(g.angle - 90, cx, cy);
                    shader.setLocalMatrix(gradientMatrix);
                    break;
                }
                default: {
                    float angle = g.angle;
                    if (g.kind == 1) {
                        float a = (float) Math.toDegrees(Math.atan2(h, w));
                        angle = g.right ? (g.bottom ? 90 + a : 90 - a) : (g.bottom ? 270 - a : 270 + a);
                    }
                    double rad = Math.toRadians(angle);
                    float sin = (float) Math.sin(rad), cos = (float) Math.cos(rad);
                    float len = Math.abs(w * sin) + Math.abs(h * cos);
                    float cx = b.left + w / 2, cy = b.top + h / 2;
                    float dx = sin * len / 2, dy = -cos * len / 2;
                    shader = new android.graphics.LinearGradient(cx - dx, cy - dy, cx + dx, cy + dy, g.colors,
                            g.positions, android.graphics.Shader.TileMode.CLAMP);
                }
            }
            paint.setShader(shader);
            paint.setColor(0xff000000);
            canvas.drawPath(box, paint);
            paint.setShader(null);
        }
    }

    private void drawOutline(Canvas canvas, Rect b) {
        if (outlineWidth <= 0 || Color.alpha(outlineColor) == 0) return;
        float w = outlineWidth;
        paint.setStyle(Paint.Style.STROKE);
        paint.setStrokeWidth(w);
        paint.setColor(outlineColor);
        if (outlineStyle == 1) {
            paint.setPathEffect(new DashPathEffect(new float[] {w * 3, w * 3}, 0));
        } else if (outlineStyle == 2) {
            paint.setPathEffect(new DashPathEffect(new float[] {0, w * 2}, 0));
            paint.setStrokeCap(Paint.Cap.ROUND);
        }
        shadowPath.reset();
        grown(b, outlineOffset + w / 2, shadowPath);
        canvas.drawPath(shadowPath, paint);
        paint.setPathEffect(null);
        paint.setStrokeCap(Paint.Cap.BUTT);
    }

    /** The view draws the borders itself, above its content (images, like
     * Core Animation's layer border). */
    boolean bordersAbove;

    void drawBorders(Canvas canvas) {
        if (!hasBorder()) return;
        Rect b = getBounds();
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

    // `transformOrigin` (fractions, or pixels) and `backfaceVisibility`.
    float originX = 0.5f, originY = 0.5f;
    boolean originXFraction = true, originYFraction = true;
    boolean backfaceHidden;

    /** Sets the pivot of `view`'s transform to the origin (after a resize too). */
    void applyPivot(View view) {
        boolean center = originXFraction && originYFraction && originX == 0.5f && originY == 0.5f;
        if (center) {
            if (Build.VERSION.SDK_INT >= 28 && view.isPivotSet()) view.resetPivot();
            return;
        }
        view.setPivotX(originXFraction ? originX * view.getWidth() : originX);
        view.setPivotY(originYFraction ? originY * view.getHeight() : originY);
    }

    /** `visibility: hidden`. */
    boolean hidden;

    /** Hides `view` while it is turned away, with `backfaceVisibility: hidden`,
     * or `visibility: hidden`. */
    void applyBackface(View view) {
        boolean away = hidden;
        if (backfaceHidden && !away) {
            double rx = Math.toRadians(view.getRotationX()), ry = Math.toRadians(view.getRotationY());
            away = Math.cos(rx) * Math.cos(ry) < 0;
        }
        int visibility = away ? View.INVISIBLE : View.VISIBLE;
        if (view.getVisibility() != visibility) view.setVisibility(visibility);
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
