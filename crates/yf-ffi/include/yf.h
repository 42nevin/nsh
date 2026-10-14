// yf: run yf apps inside an existing iOS app.
//
// Every function must be called on the main thread.
//
//   YFApp *app = yf_app_create(self.view.bounds, false);
//   [self.view addSubview:yf_app_view(app)];
//   NSString *path = [NSBundle.mainBundle pathForResource:@"bundle" ofType:@"qbc"];
//   yf_app_load_bundle(app, path.UTF8String);

#pragma once

#import <UIKit/UIKit.h>
#include <stdbool.h>

NS_ASSUME_NONNULL_BEGIN

#ifdef __cplusplus
extern "C" {
#endif

/// An embedded yf app.
typedef struct YfApp YFApp;

/// Creates an app view with `frame`; it then follows its superview's size.
/// `trace` logs per-commit timings. Free with `yf_app_destroy`.
YFApp *yf_app_create(CGRect frame, bool trace);

/// The view to add to your hierarchy. Owned by `app`.
UIView *yf_app_view(YFApp *app);

/// Runs a bundle written by `yf build` (`bundle.qbc`, or `bundle.js`);
/// assets are read from the `assets` directory next to it. Replaces the
/// running app. Returns false (and logs why) if the file cannot be read.
bool yf_app_load_bundle(YFApp *app, const char *path);

/// Development: loads from a `yf dev` server (`ws://host:port/ws`) and
/// reloads on every change, showing errors over the app. Returns false if
/// the library was built without development support.
bool yf_app_connect_dev_server(YFApp *app, const char *url);

/// Receives fatal app errors (load failures, uncaught JS exceptions) on
/// the main thread. `context` is passed back unchanged.
void yf_app_set_error_handler(YFApp *app,
                              void (*_Nullable handler)(void *_Nullable context, const char *message),
                              void *_Nullable context);

/// The status bar the app asked for (`StatusBar`). yf only controls the
/// status bar of its own windows: return these from your view controller's
/// `preferredStatusBarStyle`, `prefersStatusBarHidden` and
/// `preferredStatusBarUpdateAnimation` to let the embedded app set it.
/// yf calls `setNeedsStatusBarAppearanceUpdate` on the window's
/// controllers when it changes.
UIStatusBarStyle yf_status_bar_style(void);
bool yf_status_bar_hidden(void);
UIStatusBarAnimation yf_status_bar_animation(void);

/// Stops the app, removes its view and frees `app`.
void yf_app_destroy(YFApp *_Nullable app);

#ifdef __cplusplus
}
#endif

NS_ASSUME_NONNULL_END
