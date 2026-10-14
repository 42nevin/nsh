package dev.yf;

import android.graphics.Paint;
import android.graphics.Typeface;
import android.text.Layout;
import android.text.PrecomputedText;
import android.text.SpannableString;
import android.text.Spanned;
import android.text.StaticLayout;
import android.text.TextPaint;
import android.text.TextUtils;
import android.text.style.AbsoluteSizeSpan;
import android.text.style.ForegroundColorSpan;
import android.text.style.LineHeightSpan;
import android.text.style.MetricAffectingSpan;
import android.text.style.StrikethroughSpan;
import android.text.style.UnderlineSpan;
import java.io.File;
import java.io.FileOutputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;

/**
 * Text typesetting for the engine thread (crates/yf-platform-android/src/text.rs).
 * Sizes cross the boundary in dp; layouts are made in px for drawing.
 */
public final class TextLayouts {
    private TextLayouts() {}

    /** Where registered font files are written (set by YfView). */
    static volatile File cacheDir;

    private static final class Face {
        final int weight;
        final boolean italic;
        final Typeface typeface;

        Face(int weight, boolean italic, Typeface typeface) {
            this.weight = weight;
            this.italic = italic;
            this.typeface = typeface;
        }
    }

    private static final HashMap<String, List<Face>> FAMILIES = new HashMap<>();

    /** Registers a font file; returns an error message, or null. */
    static String registerFont(String family, byte[] data, int weight, boolean italic) {
        try {
            File dir = new File(cacheDir, "yf-fonts");
            dir.mkdirs();
            File file = new File(dir, Integer.toHexString(java.util.Arrays.hashCode(data)) + ".ttf");
            if (!file.exists()) {
                try (FileOutputStream out = new FileOutputStream(file)) {
                    out.write(data);
                }
            }
            Typeface typeface = Typeface.createFromFile(file);
            synchronized (FAMILIES) {
                List<Face> faces = FAMILIES.computeIfAbsent(family, k -> new ArrayList<>());
                faces.add(new Face(weight, italic, typeface));
            }
            // Resolutions made before this font existed are stale.
            synchronized (RESOLVED) {
                RESOLVED.clear();
            }
            return null;
        } catch (Exception e) {
            return String.valueOf(e);
        }
    }

    /** Resolved typefaces by family, weight and italic. */
    private static final HashMap<String, Typeface> RESOLVED = new HashMap<>();

    /** The typeface of a family (registered, system or default) at a weight. */
    static Typeface typeface(String family, int weight, boolean italic) {
        String key = family + '/' + weight + (italic ? "i" : "");
        synchronized (RESOLVED) {
            Typeface cached = RESOLVED.get(key);
            if (cached != null) return cached;
        }
        Typeface resolved = resolve(family, weight, italic);
        synchronized (RESOLVED) {
            RESOLVED.put(key, resolved);
        }
        return resolved;
    }

    private static Typeface resolve(String family, int weight, boolean italic) {
        Typeface base = null;
        if (family != null) {
            synchronized (FAMILIES) {
                List<Face> faces = FAMILIES.get(family);
                if (faces != null) {
                    Face best = null;
                    int bestScore = Integer.MAX_VALUE;
                    for (Face f : faces) {
                        int score = Math.abs(f.weight - weight) + (f.italic == italic ? 0 : 1000);
                        if (score < bestScore) {
                            best = f;
                            bestScore = score;
                        }
                    }
                    if (best != null) {
                        // Exact faces are used as is; others are synthesized.
                        if (best.weight == weight && best.italic == italic) return best.typeface;
                        base = best.typeface;
                    }
                }
            }
            if (base == null) base = Typeface.create(family, Typeface.NORMAL);
        }
        if (base == null) base = Typeface.DEFAULT;
        return Typeface.create(base, Math.max(1, Math.min(1000, weight)), italic);
    }

    // Span ints: start, end, weight, flags, color, decoration color, family.
    private static final int SPAN_INTS = 7;
    private static final int ITALIC = 1;
    private static final int UNDERLINE = 2;
    private static final int STRIKE = 4;

    static final class Prepared {
        final CharSequence text;
        final TextPaint paint;
        final Layout.Alignment alignment;
        final boolean justify;
        final int maxLines;
        final TextUtils.TruncateAt ellipsize;
        final float density;
        /** Natural width (px), computed once. */
        private int desired = -1;
        /** Recent layouts by width: measuring and the final layout reuse them. */
        private final StaticLayout[] layouts = new StaticLayout[4];
        private int next;

        Prepared(CharSequence text, TextPaint paint, Layout.Alignment alignment, boolean justify,
                 int maxLines, TextUtils.TruncateAt ellipsize, float density) {
            this.text = text;
            this.paint = paint;
            this.alignment = alignment;
            this.justify = justify;
            this.maxLines = maxLines;
            this.ellipsize = ellipsize;
            this.density = density;
        }

        int desired() {
            if (desired < 0) {
                float w;
                if (android.os.Build.VERSION.SDK_INT >= 29 && text instanceof PrecomputedText && singleParagraph()) {
                    // Already measured: no shaping again.
                    w = ((PrecomputedText) text).getWidth(0, text.length());
                } else {
                    w = Layout.getDesiredWidth(text, paint);
                }
                desired = (int) Math.ceil(w);
            }
            return desired;
        }

        private boolean singleParagraph() {
            return TextUtils.indexOf(text, '\n') < 0;
        }

        StaticLayout layout(int widthPx) {
            widthPx = Math.max(0, widthPx);
            for (StaticLayout l : layouts) if (l != null && l.getWidth() == widthPx) return l;
            // Break strategy and hyphenation match the PrecomputedText params,
            // so the builder reuses its glyph measurements.
            StaticLayout.Builder b = StaticLayout.Builder.obtain(text, 0, text.length(), paint, widthPx)
                    .setAlignment(alignment)
                    .setIncludePad(false)
                    .setLineSpacing(0f, 1f)
                    .setBreakStrategy(Layout.BREAK_STRATEGY_SIMPLE)
                    .setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE);
            if (justify) b.setJustificationMode(Layout.JUSTIFICATION_MODE_INTER_WORD);
            if (maxLines > 0) {
                b.setMaxLines(maxLines);
                if (ellipsize != null) b.setEllipsize(ellipsize);
            }
            StaticLayout l = b.build();
            layouts[next] = l;
            next = (next + 1) % layouts.length;
            return l;
        }
    }

    static Object prepare(String text, int[] ints, float[] floats, String[] families, int align,
                          int maxLines, int ellipsize, float density, int shadowColor, float shadowDx,
                          float shadowDy, float shadowRadius) {
        SpannableString s = new SpannableString(text);
        TextPaint paint = new TextPaint(Paint.ANTI_ALIAS_FLAG);
        paint.density = density;
        if ((shadowColor >>> 24) != 0) {
            // A zero radius would remove the shadow layer.
            paint.setShadowLayer(Math.max(0.01f, shadowRadius * density), shadowDx * density, shadowDy * density, shadowColor);
        }
        int spans = ints.length / SPAN_INTS;
        for (int i = 0; i < spans; i++) {
            int o = i * SPAN_INTS;
            int start = ints[o], end = Math.min(ints[o + 1], text.length());
            if (start >= end) continue;
            int weight = ints[o + 2], flags = ints[o + 3], color = ints[o + 4];
            String family = ints[o + 6] >= 0 ? families[ints[o + 6]] : null;
            float size = floats[i * 3] * density;
            float lineHeight = floats[i * 3 + 1] * density;
            float spacing = floats[i * 3 + 2] * density;
            Typeface typeface = typeface(family, weight, (flags & ITALIC) != 0);
            if (i == 0) {
                // Base metrics (empty lines, ellipsis) follow the first run.
                paint.setTextSize(size);
                paint.setTypeface(typeface);
            }
            int f = Spanned.SPAN_EXCLUSIVE_EXCLUSIVE;
            s.setSpan(new AbsoluteSizeSpan(Math.round(size)), start, end, f);
            s.setSpan(new ForegroundColorSpan(color), start, end, f);
            s.setSpan(new FontSpan(typeface, spacing), start, end, f);
            if ((flags & UNDERLINE) != 0) s.setSpan(new UnderlineSpan(), start, end, f);
            if ((flags & STRIKE) != 0) s.setSpan(new StrikethroughSpan(), start, end, f);
            if (lineHeight > 0) s.setSpan(new ExactLineHeight(lineHeight), start, end, Spanned.SPAN_INCLUSIVE_INCLUSIVE);
        }
        Layout.Alignment alignment;
        switch (align) {
            case 2: alignment = Layout.Alignment.ALIGN_CENTER; break;
            case 3: alignment = Layout.Alignment.ALIGN_OPPOSITE; break;
            default: alignment = Layout.Alignment.ALIGN_NORMAL;
        }
        TextUtils.TruncateAt truncate;
        switch (ellipsize) {
            case 1: truncate = TextUtils.TruncateAt.START; break;
            case 2: truncate = TextUtils.TruncateAt.MIDDLE; break;
            case 3: truncate = null; break;
            default: truncate = TextUtils.TruncateAt.END;
        }
        // Glyphs are measured once here; every layout width reuses them.
        PrecomputedText.Params params = new PrecomputedText.Params.Builder(paint)
                .setBreakStrategy(Layout.BREAK_STRATEGY_SIMPLE)
                .setHyphenationFrequency(Layout.HYPHENATION_FREQUENCY_NONE)
                .build();
        CharSequence measured = PrecomputedText.create(s, params);
        return new Prepared(measured, paint, alignment, align == 4, maxLines, truncate, density);
    }

    /** Size in dp, packed as two float bit patterns (width high). */
    static long measure(Object prepared, float maxWidthDp) {
        Prepared p = (Prepared) prepared;
        int width = p.desired();
        if (!Float.isNaN(maxWidthDp)) {
            // Text that fits lays out as at its natural width: same layout.
            width = Math.min(width, (int) Math.floor(maxWidthDp * p.density));
        }
        StaticLayout layout = p.layout(width);
        // Without an ellipsis StaticLayout ignores maxLines: count the lines
        // here (the view clips the rest).
        int lines = layout.getLineCount();
        if (p.maxLines > 0) lines = Math.min(lines, p.maxLines);
        float max = 0;
        for (int i = 0; i < lines; i++) max = Math.max(max, layout.getLineWidth(i));
        float w = (float) Math.ceil(Math.min(max, width)) / p.density;
        float h = layout.getLineTop(lines) / p.density;
        return ((long) Float.floatToRawIntBits(w) << 32) | (Float.floatToRawIntBits(h) & 0xffffffffL);
    }

    /** The final layout, in px, for a box of the given dp size. */
    static Object layout(Object prepared, float widthDp, float heightDp) {
        Prepared p = (Prepared) prepared;
        int width = (int) Math.ceil(widthDp * p.density);
        // Left-aligned LTR text that fits draws the same in a wider box:
        // keep the measured layout.
        if (p.alignment == Layout.Alignment.ALIGN_NORMAL && !p.justify && width >= p.desired()) {
            StaticLayout natural = p.layout(p.desired());
            if (natural.getParagraphDirection(0) == Layout.DIR_LEFT_TO_RIGHT) return natural;
        }
        return p.layout(width);
    }

    /** Typeface and letter spacing of a run. */
    private static final class FontSpan extends MetricAffectingSpan {
        private final Typeface typeface;
        private final float spacingPx;

        FontSpan(Typeface typeface, float spacingPx) {
            this.typeface = typeface;
            this.spacingPx = spacingPx;
        }

        private void apply(TextPaint paint) {
            paint.setTypeface(typeface);
            if (spacingPx != 0 && paint.getTextSize() > 0) paint.setLetterSpacing(spacingPx / paint.getTextSize());
        }

        @Override
        public void updateMeasureState(TextPaint paint) {
            apply(paint);
        }

        @Override
        public void updateDrawState(TextPaint paint) {
            apply(paint);
        }
    }

    /** `lineHeight`: every line exactly this tall, extra space split around the glyphs. */
    private static final class ExactLineHeight implements LineHeightSpan {
        private final int height;

        ExactLineHeight(float height) {
            this.height = (int) Math.ceil(height);
        }

        @Override
        public void chooseHeight(CharSequence text, int start, int end, int spanstartv, int lineHeight,
                                 Paint.FontMetricsInt fm) {
            int current = fm.descent - fm.ascent;
            if (current <= 0) return;
            int extra = height - current;
            int top = extra / 2;
            fm.ascent -= top;
            fm.top = fm.ascent;
            fm.descent += extra - top;
            fm.bottom = fm.descent;
        }
    }
}
