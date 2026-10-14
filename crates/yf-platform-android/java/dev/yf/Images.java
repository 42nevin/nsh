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
    interface Callback {
        void done(Bitmap bitmap, String error);
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
    private static final LruCache<String, Bitmap> CACHE =
            new LruCache<String, Bitmap>((int) Math.min(Runtime.getRuntime().maxMemory() / 8, 96L << 20)) {
                @Override
                protected int sizeOf(String key, Bitmap bitmap) {
                    return bitmap.getAllocationByteCount();
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

    static String key(String uri, int maxPx) {
        return maxPx + "@" + uri;
    }

    /** Requests `uri` decoded to at most `maxPx` on its longer side. */
    Bitmap request(String uri, int maxPx, Callback callback) {
        String key = key(uri, maxPx);
        Bitmap cached = CACHE.get(key);
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
            Bitmap bitmap = null;
            String error = null;
            try {
                byte[] bytes = fetch(uri);
                bitmap = decode(bytes, maxPx);
                if (bitmap == null) error = "cannot decode " + uri;
            } catch (Exception e) {
                error = String.valueOf(e);
            }
            Bitmap result = bitmap;
            String message = error;
            main.post(() -> finish(key, result, message));
        });
        return null;
    }

    /** `callback` no longer wants `uri`; cancels the load if nobody does. */
    void cancel(String uri, int maxPx, Callback callback) {
        String key = key(uri, maxPx);
        Inflight request = inflight.get(key);
        if (request == null) return;
        request.waiters.remove(callback);
        if (request.waiters.isEmpty()) {
            request.task.cancel(true);
            inflight.remove(key);
        }
    }

    private void finish(String key, Bitmap bitmap, String error) {
        Inflight request = inflight.remove(key);
        if (bitmap != null) CACHE.put(key, bitmap);
        if (request == null) return;
        for (Callback c : request.waiters) c.done(bitmap, error);
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

    private static Bitmap decode(byte[] bytes, int maxPx) {
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(bytes, 0, bytes.length, bounds);
        BitmapFactory.Options options = new BitmapFactory.Options();
        int longest = Math.max(bounds.outWidth, bounds.outHeight);
        int sample = 1;
        // Power-of-two downsampling while the result stays at least maxPx.
        while (maxPx > 0 && longest / (sample * 2) >= maxPx) sample *= 2;
        options.inSampleSize = sample;
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.length, options);
    }
}
