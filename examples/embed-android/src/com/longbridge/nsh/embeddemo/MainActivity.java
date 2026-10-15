package com.longbridge.nsh.embeddemo;

import android.app.Activity;
import android.graphics.Color;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.util.Log;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.TextView;
import com.longbridge.nsh.NshView;

/**
 * An existing Android app that embeds a nsh app below its own native UI.
 * The nsh bundle (`nsh build`) is packaged as APK assets under `nsh/`.
 */
public final class MainActivity extends Activity {
    private NshView nsh;
    private int loads;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        float d = getResources().getDisplayMetrics().density;

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        column.setPadding(Math.round(20 * d), Math.round(48 * d), Math.round(20 * d), Math.round(20 * d));
        column.setBackgroundColor(Color.WHITE);

        TextView title = new TextView(this);
        title.setText("Native host app");
        title.setTextSize(24);
        title.setTextColor(Color.BLACK);
        column.addView(title);

        TextView count = new TextView(this);
        Button reload = new Button(this);
        reload.setText("Reload nsh");
        reload.setOnClickListener(v -> {
            nsh.loadAssetBundle("nsh");
            count.setText("loaded " + (++loads) + "×");
        });
        column.addView(reload);
        column.addView(count);

        // The embedded nsh app, in a bordered frame.
        FrameLayout frame = new FrameLayout(this);
        GradientDrawable border = new GradientDrawable();
        border.setStroke(Math.round(2 * d), 0xff8250df);
        frame.setBackground(border);
        frame.setPadding(Math.round(2 * d), Math.round(2 * d), Math.round(2 * d), Math.round(2 * d));
        nsh = new NshView(this);
        nsh.setErrorListener(message -> Log.e("embed-demo", message));
        frame.addView(nsh, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
        column.addView(frame, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1));

        setContentView(column);
        nsh.loadAssetBundle("nsh");
        count.setText("loaded " + (++loads) + "×");
    }

    @Override
    protected void onDestroy() {
        nsh.destroy();
        super.onDestroy();
    }
}
