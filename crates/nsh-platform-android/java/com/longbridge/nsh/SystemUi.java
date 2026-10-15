package com.longbridge.nsh;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.content.ContextWrapper;
import android.content.res.Configuration;
import android.os.Build;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.Toast;

/** System UI driven from JS: the status bar, alerts / action sheets and toasts. */
final class SystemUi {
    private SystemUi() {}

    static Activity activity(Context context) {
        while (context instanceof ContextWrapper) {
            if (context instanceof Activity) return (Activity) context;
            context = ((ContextWrapper) context).getBaseContext();
        }
        return null;
    }

    /**
     * The status bar of the activity's window. style: 0 default (follows
     * the appearance), 1 light content, 2 dark content.
     */
    @SuppressWarnings("deprecation")
    static void statusBar(Context context, int style, boolean hidden, Integer background) {
        Activity activity = activity(context);
        if (activity == null) return;
        Window window = activity.getWindow();
        boolean night = (context.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK)
                == Configuration.UI_MODE_NIGHT_YES;
        boolean darkIcons = style == 2 || (style == 0 && !night);
        // Before edge-to-edge (API 35) the bar has a color of its own.
        if (background != null && Build.VERSION.SDK_INT < 35) window.setStatusBarColor(background);
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController c = window.getInsetsController();
            if (c == null) return;
            int light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS;
            c.setSystemBarsAppearance(darkIcons ? light : 0, light);
            if (hidden) {
                c.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                c.hide(WindowInsets.Type.statusBars());
            } else {
                c.show(WindowInsets.Type.statusBars());
            }
        } else {
            View decor = window.getDecorView();
            int flags = decor.getSystemUiVisibility();
            flags = darkIcons ? flags | View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR : flags & ~View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR;
            flags = hidden ? flags | View.SYSTEM_UI_FLAG_FULLSCREEN : flags & ~View.SYSTEM_UI_FLAG_FULLSCREEN;
            decor.setSystemUiVisibility(flags);
        }
    }

    /**
     * An alert, or with {@code sheet} a list of actions. Answers with
     * {@link Native#DIALOG}: the button index, or -1 when dismissed.
     * Alerts map buttons as React Native does: the last is positive, the
     * one before negative, the one before that neutral (three at most).
     * styles: 0 default, 1 cancel, 2 destructive.
     */
    static void dialog(Mounter mounter, int dialogId, String title, String message, boolean cancelable, boolean sheet,
            String[] texts, int[] styles) {
        Activity activity = activity(mounter.context);
        if (activity == null || activity.isFinishing()) {
            answer(mounter, dialogId, -1);
            return;
        }
        boolean[] answered = {false};
        AlertDialog.Builder b = new AlertDialog.Builder(activity);
        if (title != null && !title.isEmpty()) b.setTitle(title);
        b.setCancelable(cancelable);
        if (sheet) {
            // The cancel option is the dismissal (a tap outside / back).
            int count = 0;
            for (int s : styles) if (s != 1) count++;
            CharSequence[] items = new CharSequence[count];
            int[] indices = new int[count];
            for (int i = 0, j = 0; i < texts.length; i++) {
                if (styles[i] == 1) continue;
                items[j] = texts[i];
                indices[j++] = i;
            }
            // A list dialog shows no message.
            if ((title == null || title.isEmpty()) && message != null) b.setTitle(message);
            b.setItems(items, (d, which) -> {
                answered[0] = true;
                answer(mounter, dialogId, indices[which]);
            });
            b.setCancelable(true);
        } else {
            if (message != null) b.setMessage(message);
            int n = texts.length;
            if (n >= 1) {
                b.setPositiveButton(texts[n - 1], (d, w) -> {
                    answered[0] = true;
                    answer(mounter, dialogId, n - 1);
                });
            }
            if (n >= 2) {
                b.setNegativeButton(texts[n - 2], (d, w) -> {
                    answered[0] = true;
                    answer(mounter, dialogId, n - 2);
                });
            }
            if (n >= 3) {
                b.setNeutralButton(texts[n - 3], (d, w) -> {
                    answered[0] = true;
                    answer(mounter, dialogId, n - 3);
                });
            }
        }
        b.setOnDismissListener(d -> {
            if (!answered[0]) {
                answered[0] = true;
                answer(mounter, dialogId, -1);
            }
        });
        b.show();
    }

    private static void answer(Mounter mounter, int dialogId, int index) {
        mounter.eventMap(1, Native.DIALOG, new String[] {"id", "index"},
                new Object[] {(double) dialogId, (double) index});
    }

    static void toast(Context context, String text, boolean longer) {
        Toast.makeText(context, text, longer ? Toast.LENGTH_LONG : Toast.LENGTH_SHORT).show();
    }
}
