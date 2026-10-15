package dev.yf;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.os.Handler;
import android.os.Looper;
import android.util.LruCache;
import java.io.FileInputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;

/**
 * Loads images off the UI thread, decoded at (about) their display size,
 * cached in memory, with concurrent requests for the same image coalesced
 * and downloads nobody waits for cancelled.
 */
final class Images {
    /** {@code image}: null on errors. */
    interface Callback {
        void done(Image image, String error);
    }

    /** A decoded image and the pixel size of its source (before downsampling). */
    static final class Image {
        /** A {@link Bitmap}, or an animated {@code Drawable} (started by the view). */
        final Object content;
        final int width, height;

        Image(Object content, int width, int height) {
            this.content = content;
            this.width = width;
            this.height = height;
        }
    }

    /** Reads `asset:/…` URIs (APK assets or the dev server). */
    interface AssetReader {
        byte[] read(String uri);
    }

    private static final ExecutorService POOL = Executors.newFixedThreadPool(4, r -> {
        Thread t = new Thread(r, "yf-images");
        t.setDaemon(true);
        return t;
    });
    /** Bitmaps only. */
    private static final LruCache<String, Image> CACHE =
            new LruCache<String, Image>((int) Math.min(Runtime.getRuntime().maxMemory() / 8, 96L << 20)) {
                @Override
                protected int sizeOf(String key, Image image) {
                    return ((Bitmap) image.content).getAllocationByteCount();
                }
            };

    private final Handler main = new Handler(Looper.getMainLooper());
    private final AssetReader assets;
    private final HashMap<String, Inflight> inflight = new HashMap<>();

    private static final class Inflight {
        final List<Callback> waiters = new ArrayList<>();
        Future<?> task;
    }

    Images(AssetReader assets) {
        this.assets = assets;
    }

    static String key(String uri, int maxPx, int blurPx) {
        return blurPx > 0 ? maxPx + "@" + blurPx + "@" + uri : maxPx + "@" + uri;
    }

    /**
     * Requests `uri` decoded to at most `maxPx` on its longer side, blurred
     * by `blurPx`: a cached bitmap at once, or null and a callback later.
     * Animated images are not cached (each view plays its own).
     */
    Image request(String uri, int maxPx, int blurPx, Callback callback) {
        String key = key(uri, maxPx, blurPx);
        Image cached = CACHE.get(key);
        if (cached != null) return cached;
        Inflight existing = inflight.get(key);
        if (existing != null) {
            existing.waiters.add(callback);
            return null;
        }
        Inflight request = new Inflight();
        request.waiters.add(callback);
        inflight.put(key, request);
        request.task = POOL.submit(() -> {
            Image image = null;
            String error = null;
            try {
                byte[] bytes = fetch(uri);
                image = animated(bytes) ? decodeAnimated(bytes, maxPx) : null;
                if (image == null) image = decode(bytes, maxPx, blurPx);
                if (image == null) error = "cannot decode " + uri;
            } catch (Exception e) {
                error = String.valueOf(e);
            }
            Image result = image;
            String message = error;
            main.post(() -> finish(key, result, message));
        });
        return null;
    }

    /** GIF, animated WebP (VP8X with the animation flag) or APNG (acTL). */
    private static boolean animated(byte[] b) {
        if (b.length > 6 && b[0] == 'G' && b[1] == 'I' && b[2] == 'F') return true;
        if (b.length > 21 && b[0] == 'R' && b[1] == 'I' && b[2] == 'F' && b[3] == 'F'
                && b[8] == 'W' && b[9] == 'E' && b[10] == 'B' && b[11] == 'P'
                && b[12] == 'V' && b[13] == 'P' && b[14] == '8' && b[15] == 'X') {
            return (b[20] & 0x02) != 0;
        }
        if (b.length > 8 && (b[0] & 0xff) == 0x89 && b[1] == 'P' && b[2] == 'N' && b[3] == 'G') {
            // acTL comes before the first IDAT.
            for (int i = 8; i + 8 < b.length; ) {
                int length = ((b[i] & 0xff) << 24) | ((b[i + 1] & 0xff) << 16) | ((b[i + 2] & 0xff) << 8) | (b[i + 3] & 0xff);
                String type = new String(b, i + 4, 4, java.nio.charset.StandardCharsets.US_ASCII);
                if (type.equals("acTL")) return true;
                if (type.equals("IDAT") || length < 0) return false;
                i += 12 + length;
            }
        }
        return false;
    }

    /** An animated drawable (API 28+), downsampled like bitmaps. */
    private static Image decodeAnimated(byte[] bytes, int maxPx) throws java.io.IOException {
        if (android.os.Build.VERSION.SDK_INT < 28) return null;
        android.graphics.ImageDecoder.Source source =
                android.graphics.ImageDecoder.createSource(java.nio.ByteBuffer.wrap(bytes));
        int[] size = new int[2];
        android.graphics.drawable.Drawable d = android.graphics.ImageDecoder.decodeDrawable(source, (decoder, info, src) -> {
            size[0] = info.getSize().getWidth();
            size[1] = info.getSize().getHeight();
            int longest = Math.max(size[0], size[1]);
            int sample = 1;
            while (maxPx > 0 && longest / (sample * 2) >= maxPx) sample *= 2;
            decoder.setTargetSampleSize(sample);
        });
        return d instanceof android.graphics.drawable.AnimatedImageDrawable ? new Image(d, size[0], size[1]) : null;
    }

    /** `callback` no longer wants `uri`; cancels the load if nobody does. */
    void cancel(String uri, int maxPx, int blurPx, Callback callback) {
        String key = key(uri, maxPx, blurPx);
        Inflight request = inflight.get(key);
        if (request == null) return;
        request.waiters.remove(callback);
        if (request.waiters.isEmpty()) {
            request.task.cancel(true);
            inflight.remove(key);
        }
    }

    private void finish(String key, Image image, String error) {
        Inflight request = inflight.remove(key);
        if (image != null && image.content instanceof Bitmap) CACHE.put(key, image);
        if (request == null) return;
        for (int i = 0; i < request.waiters.size(); i++) {
            Image shared = image;
            // Each view plays its own animation.
            if (i > 0 && image != null && image.content instanceof android.graphics.drawable.Drawable) {
                android.graphics.drawable.Drawable.ConstantState state =
                        ((android.graphics.drawable.Drawable) image.content).getConstantState();
                if (state != null) shared = new Image(state.newDrawable(), image.width, image.height);
            }
            request.waiters.get(i).done(shared, error);
        }
    }

    private byte[] fetch(String uri) throws Exception {
        if (uri.startsWith("asset:/")) {
            byte[] bytes = assets.read(uri);
            if (bytes == null) throw new java.io.FileNotFoundException(uri);
            return bytes;
        }
        if (uri.startsWith("http://") || uri.startsWith("https://")) {
            HttpURLConnection c = (HttpURLConnection) new URL(uri).openConnection();
            try {
                c.setConnectTimeout(30_000);
                c.setReadTimeout(60_000);
                return Net.readAll(c.getInputStream());
            } finally {
                c.disconnect();
            }
        }
        String path = uri.startsWith("file://") ? uri.substring(7) : uri;
        try (InputStream in = new FileInputStream(path)) {
            return Net.readAll(in);
        }
    }

    private static Image decode(byte[] bytes, int maxPx, int blurPx) {
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);
        BitmapFactory.Options options = new BitmapFactory.Options();
        int longest = Math.max(bounds.outWidth, bounds.outHeight);
        int sample = 1;
        // Power-of-two downsampling while the result stays at least maxPx.
        while (maxPx > 0 && longest / (sample * 2) >= maxPx) sample *= 2;
        options.inSampleSize = sample;
        Bitmap bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
        if (bitmap == null) return null;
        if (blurPx > 0) bitmap = Blur.blurred(bitmap, blurPx);
        return new Image(bitmap, bounds.outWidth, bounds.outHeight);
    }
}
