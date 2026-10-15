package dev.nsh;

import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** `fetch` on HttpURLConnection (crates/nsh-platform-android/src/net.rs). */
final class Net {
    private Net() {}

    private static final ExecutorService POOL = Executors.newCachedThreadPool(r -> {
        Thread t = new Thread(r, "nsh-net");
        t.setDaemon(true);
        return t;
    });
    private static final ConcurrentHashMap<Long, HttpURLConnection> ACTIVE = new ConcurrentHashMap<>();

    static native void nativeDone(long key, int status, String url, boolean redirected, String[] headers,
                                  byte[] body, String error);

    static void fetch(long key, String method, String url, String[] headers, byte[] body, boolean follow) {
        POOL.execute(() -> run(key, method, url, headers, body, follow));
    }

    static void cancel(long key) {
        HttpURLConnection connection = ACTIVE.remove(key);
        if (connection != null) connection.disconnect();
    }

    private static void run(long key, String method, String url, String[] headers, byte[] body, boolean follow) {
        HttpURLConnection c = null;
        try {
            c = (HttpURLConnection) new URL(url).openConnection();
            ACTIVE.put(key, c);
            c.setInstanceFollowRedirects(follow);
            c.setRequestMethod(method);
            c.setConnectTimeout(30_000);
            c.setReadTimeout(60_000);
            for (int i = 0; i + 1 < headers.length; i += 2) c.addRequestProperty(headers[i], headers[i + 1]);
            if (body != null) {
                c.setDoOutput(true);
                c.setFixedLengthStreamingMode(body.length);
                try (OutputStream out = c.getOutputStream()) {
                    out.write(body);
                }
            }
            int status = c.getResponseCode();
            InputStream in = status >= 400 ? c.getErrorStream() : c.getInputStream();
            byte[] bytes = in == null ? new byte[0] : readAll(in);
            List<String> flat = new ArrayList<>();
            for (Map.Entry<String, List<String>> e : c.getHeaderFields().entrySet()) {
                if (e.getKey() == null) continue; // the status line
                for (String v : e.getValue()) {
                    flat.add(e.getKey());
                    flat.add(v);
                }
            }
            String finalUrl = c.getURL().toString();
            nativeDone(key, status, finalUrl, !finalUrl.equals(url), flat.toArray(new String[0]), bytes, null);
        } catch (Exception e) {
            nativeDone(key, 0, url, false, new String[0], null, String.valueOf(e.getMessage() != null ? e.getMessage() : e));
        } finally {
            ACTIVE.remove(key);
            if (c != null) c.disconnect();
        }
    }

    static byte[] readAll(InputStream in) throws java.io.IOException {
        try (InputStream input = in) {
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buf = new byte[16 * 1024];
            int n;
            while ((n = input.read(buf)) > 0) out.write(buf, 0, n);
            return out.toByteArray();
        }
    }
}
