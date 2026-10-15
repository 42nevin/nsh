// nsh: run nsh apps inside an existing iOS app.
//
// Every function must be called on the main thread.
//
//   NshApp *app = nsh_app_create(self.view.bounds, false);
//   [self.view addSubview:nsh_app_view(app)];
//   NSString *path = [NSBundle.mainBundle pathForResource:@"bundle" ofType:@"qbc"];
//   nsh_app_load_bundle(app, path.UTF8String);

#pragma once

#import <UIKit/UIKit.h>
#include <stdbool.h>

NS_ASSUME_NONNULL_BEGIN

#ifdef __cplusplus
extern "C" {
#endif

/// An embedded nsh app.
typedef struct NshApp NshApp;

/// Creates an app view with `frame`; it then follows its superview's size.
/// `trace` logs per-commit timings. Free with `nsh_app_destroy`.
NshApp *nsh_app_create(CGRect frame, bool trace);

/// The view to add to your hierarchy. Owned by `app`.
UIView *nsh_app_view(NshApp *app);

/// Runs a bundle written by `nsh build` (`bundle.qbc`, or `bundle.js`);
/// assets are read from the `assets` directory next to it. Replaces the
/// running app. Returns false (and logs why) if the file cannot be read.
bool nsh_app_load_bundle(NshApp *app, const char *path);

/// What the apps `app` loads are granted at most: JSON in the form of
/// `nsh.json`'s `capabilities` (`NULL`: what each app declares), e.g.
/// `{"network": {"hosts": ["api.example.com"]}}`. An app's request must be
/// allowed by both its declaration and this grant. Applies from the next
/// load; returns false (and changes nothing) for an invalid grant.
bool nsh_app_set_granted_capabilities(NshApp *app, const char *_Nullable json);

/// The VM limits of the apps `app` loads next: JS heap (MB), stack (KB),
/// the time budget (ms) of loading and of every other call into JS, and
/// the node quota. 0 keeps a default. Running out of memory or going over a
/// quota stops the app and reports it to the error handler.
void nsh_app_set_limits(NshApp *app, uint32_t memory_mb, uint32_t stack_kb, uint32_t load_ms,
                        uint32_t call_ms, uint32_t max_nodes);

/// Development: loads from a `nsh dev` server (`ws://host:port/ws`) and
/// reloads on every change, showing errors over the app. Returns false if
/// the library was built without development support.
bool nsh_app_connect_dev_server(NshApp *app, const char *url);

/// Receives fatal app errors (load failures, uncaught JS exceptions) on
/// the main thread. `context` is passed back unchanged.
void nsh_app_set_error_handler(NshApp *app,
                              void (*_Nullable handler)(void *_Nullable context, const char *message),
                              void *_Nullable context);

/// The status bar the app asked for (`StatusBar`). nsh only controls the
/// status bar of its own windows: return these from your view controller's
/// `preferredStatusBarStyle`, `prefersStatusBarHidden` and
/// `preferredStatusBarUpdateAnimation` to let the embedded app set it.
/// nsh calls `setNeedsStatusBarAppearanceUpdate` on the window's
/// controllers when it changes.
UIStatusBarStyle nsh_status_bar_style(void);
bool nsh_status_bar_hidden(void);
UIStatusBarAnimation nsh_status_bar_animation(void);

/// Stops the app, removes its view and frees `app`.
void nsh_app_destroy(NshApp *_Nullable app);

#ifdef __cplusplus
}
#endif

NS_ASSUME_NONNULL_END
