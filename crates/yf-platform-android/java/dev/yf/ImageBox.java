package dev.yf;

import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapShader;
import android.graphics.Canvas;
import android.graphics.Matrix;
import android.graphics.Paint;
import android.graphics.Path;
import android.graphics.PorterDuff;
import android.graphics.PorterDuffColorFilter;
import android.graphics.Rect;
import android.graphics.Shader;
import android.view.View;

/** An image drawn inside the view's rounded shape (always clipped). */
@SuppressLint("ViewConstructor")
final class ImageBox extends View implements Node {
    final Decor decor = new Decor();
    final int id;
    int resizeMode; // 0 cover, 1 contain, 2 stretch, 3 center
    String uri;
    /** Size the pending load decodes at. */
    int requestedPx;
    final Images.Callback callback;
    private Bitmap bitmap;
    private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG | Paint.FILTER_BITMAP_FLAG);
    private final Matrix matrix = new Matrix();
    private final Path path = new Path();
    private final android.graphics.RectF dest = new android.graphics.RectF();

    ImageBox(Context context, int id, Images.Callback callback) {
        super(context);
        this.id = id;
        this.callback = callback;
        setBackground(decor);
    }

    @Override
    public int nodeId() {
        return id;
    }

    /** Press handling, once the view has press listeners. */
    Pressable press;

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

    /** `tintColor`: draws the image's opaque pixels in one color (0: none). */
    void setTint(int color) {
        paint.setColorFilter((color >>> 24) == 0 ? null : new PorterDuffColorFilter(color, PorterDuff.Mode.SRC_IN));
        invalidate();
    }

    void setBitmap(Bitmap bitmap) {
        this.bitmap = bitmap;
        paint.setShader(bitmap == null ? null : new BitmapShader(bitmap, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP));
        invalidate();
    }

    @Override
    protected void onDraw(Canvas canvas) {
        if (bitmap == null || getWidth() == 0 || getHeight() == 0) return;
        float w = getWidth(), h = getHeight(), bw = bitmap.getWidth(), bh = bitmap.getHeight();
        matrix.reset();
        switch (resizeMode) {
            case 2:
                matrix.setScale(w / bw, h / bh);
                break;
            case 3:
                matrix.setTranslate((w - bw) / 2, (h - bh) / 2);
                break;
            default: {
                float scale = resizeMode == 1 ? Math.min(w / bw, h / bh) : Math.max(w / bw, h / bh);
                matrix.setScale(scale, scale);
                matrix.postTranslate((w - bw * scale) / 2, (h - bh * scale) / 2);
            }
        }
        paint.getShader().setLocalMatrix(matrix);
        // Only where the image is: the shader clamps (repeats edge pixels)
        // outside it.
        dest.set(0, 0, bw, bh);
        matrix.mapRect(dest);
        canvas.save();
        canvas.clipRect(dest);
        canvas.drawPath(decor.shape(new Rect(0, 0, getWidth(), getHeight()), 0, path), paint);
        canvas.restore();
    }
}
