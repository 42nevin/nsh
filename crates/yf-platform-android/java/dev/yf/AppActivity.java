package dev.yf;

import android.app.Activity;
import android.os.Bundle;

/** The activity of packaged apps (`yf package --target android`). */
public final class AppActivity extends Activity {
    private YfView view;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        DevActivity.edgeToEdge(this);
        view = new YfView(this);
        setContentView(view);
        view.loadAssetBundle("yf");
    }

    @Override
    public void onConfigurationChanged(android.content.res.Configuration config) {
        super.onConfigurationChanged(config);
        DevActivity.systemBars(this);
    }

    /** Android 12 and older: the app gets the back button first. */
    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        if (view == null || !view.handleBack()) super.onBackPressed();
    }

    @Override
    protected void onDestroy() {
        view.destroy();
        super.onDestroy();
    }
}
