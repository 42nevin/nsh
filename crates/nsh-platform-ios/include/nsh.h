// The C ABI under the NativeShell Swift module (`src/embed.rs`). Internal:
// apps use `import NativeShell` (`swift/NativeShell.swift`), which imports
// this module (`NativeShellC`) privately.
//
// Errors are returned as strings owned by the caller (NULL when there is
// none); free them with `nsh_string_free`. App functions must be called on the
// main thread; trust and package functions on any thread.

#pragma once

#import <UIKit/UIKit.h>
#include <stdbool.h>
#include <stdint.h>

NS_ASSUME_NONNULL_BEGIN

#ifdef __cplusplus
extern "C" {
#endif

/// Frees a string returned by these functions.
void nsh_string_free(char *_Nullable s);

/// An embedded nsh app.
typedef struct NshApp NshApp;

/// Creates an app view with `frame`; it then follows its superview's size.
/// `trace` logs per-commit timings. Free with `nsh_app_destroy`.
NshApp *nsh_app_create(CGRect frame, bool trace);

/// The view to add to a hierarchy. Owned by `app`.
UIView *nsh_app_view(NshApp *app);

/// Stops the app, removes its view and frees `app`.
void nsh_app_destroy(NshApp *_Nullable app);

/// Runs a bundle written by `nsh build`, replacing the running app; assets
/// are read from `assets` next to it. Bytecode (`bundle.qbc`) only loads from
/// inside the app's own bundle; elsewhere only JS source does. Returns why it
/// cannot, or NULL.
char *_Nullable nsh_app_load_bundle(NshApp *app, const char *path);

/// What the apps `app` loads are granted at most: JSON in the form of
/// `nsh.json`'s `capabilities` (NULL: what each app declares). Applies from
/// the next load. Returns why the grant is invalid, or NULL.
char *_Nullable nsh_app_set_granted_capabilities(NshApp *app, const char *_Nullable json);

/// The VM limits of the apps `app` loads next: JS heap (MB), stack (KB),
/// the time budgets (ms) of loading and of every other call into JS, and the
/// node quota; 0 keeps a default.
void nsh_app_set_limits(NshApp *app, uint32_t memory_mb, uint32_t stack_kb, uint32_t load_ms,
                        uint32_t call_ms, uint32_t max_nodes);

/// Development: loads from a `nsh dev` server (`ws://host:port/ws`) and
/// reloads on every change. Returns an error without development support,
/// or NULL.
char *_Nullable nsh_app_connect_dev_server(NshApp *app, const char *url);

/// Receives fatal app errors on the main thread, replacing the previous
/// handler; `release` (optional) is called with `context` when the handler
/// is replaced or the app destroyed.
void nsh_app_set_error_handler(NshApp *app,
                               void (*_Nullable handler)(void *_Nullable context, const char *message),
                               void *_Nullable context,
                               void (*_Nullable release)(void *_Nullable context));

/// The status bar the running apps asked for, for a host view controller.
UIStatusBarStyle nsh_status_bar_style(void);
bool nsh_status_bar_hidden(void);
UIStatusBarAnimation nsh_status_bar_animation(void);

/// The keys, revocations and accepted builds that signed packages are
/// verified against.
typedef struct NshTrust NshTrust;

/// A trust store kept in `state_dir` (created if needed). On failure
/// returns NULL and sets `*error`. Free with `nsh_trust_destroy`.
NshTrust *_Nullable nsh_trust_create(const char *state_dir, char *_Nullable *_Nonnull error);

/// Trusts a root public key (base64, `<name>.pub` of `nsh keygen`).
/// Returns why the key is invalid, or NULL.
char *_Nullable nsh_trust_add_root(const NshTrust *trust, const char *public_key);

/// Applies a revocation list (`nsh revoke`) signed by a trusted root and
/// newer than the one the store has. Returns why it is refused, or NULL.
char *_Nullable nsh_trust_update_revocations(const NshTrust *trust, const char *json);

void nsh_trust_destroy(NshTrust *_Nullable trust);

/// A verified package, ready to run.
typedef struct NshPackage NshPackage;

/// Verifies the signed package at `path`, extracts its assets and accepts
/// its build. Slow for large packages: call it off the main thread. On
/// refusal returns NULL and sets `*error`.
NshPackage *_Nullable nsh_trust_open_package(const NshTrust *trust, const char *path,
                                             char *_Nullable *_Nonnull error);

/// The package's app. Strings are owned by `package`.
const char *nsh_package_app_id(const NshPackage *package);
const char *nsh_package_app_name(const NshPackage *package);
const char *nsh_package_app_version(const NshPackage *package);
uint64_t nsh_package_app_build(const NshPackage *package);

/// Runs `package`, replacing the running app. Takes `package`.
void nsh_app_run_package(NshApp *app, NshPackage *package);

/// Frees a package that will not run.
void nsh_package_free(NshPackage *_Nullable package);

#ifdef __cplusplus
}
#endif

NS_ASSUME_NONNULL_END
