// NativeShell: embedding nsh apps in iOS apps.
//
//     let app = NshAppView(frame: container.bounds)
//     container.addSubview(app)
//     app.onError = { message in print(message) }
//     try app.loadBundle(at: Bundle.main.url(forResource: "bundle", withExtension: "qbc", subdirectory: "nsh")!)
//
// Signed packages (`nsh publish`), e.g. downloaded updates:
//
//     let trust = try NshTrust(stateDirectory: applicationSupport.appendingPathComponent("nsh"))
//     try trust.addRoot("<root.pub>")
//     try await app.loadPackage(at: downloaded, trust: trust)
//
// Built over the C ABI of `src/embed.rs` (`include/nsh.h`), imported
// privately: apps only see this module.

import UIKit
internal import NativeShellC

/// Why nsh refused something: an invalid bundle, grant, key or revocation
/// list, a refused package, a missing development build.
public struct NshError: Error, LocalizedError, CustomStringConvertible, Sendable {
    /// What went wrong.
    public let message: String

    public var description: String { message }
    public var errorDescription: String? { message }
}

/// Throws the error string `error` returned, if any (freeing it).
private func check(_ error: UnsafeMutablePointer<CChar>?) throws {
    guard let error else { return }
    let message = String(cString: error)
    nsh_string_free(error)
    throw NshError(message: message)
}

/// The VM limits of an app (`nil`: the default).
public struct NshLimits: Sendable, Equatable {
    /// JS heap in MB (default 256). Running out stops the app.
    public var memoryMB: UInt32?
    /// Interpreter stack in KB (default 4096, also the maximum).
    public var stackKB: UInt32?
    /// Time to evaluate the bundle, in seconds (default 10).
    public var loadBudget: TimeInterval?
    /// Time of any other call into JS (one event, timer or frame), in
    /// seconds (default 2). An overrun call is cut off.
    public var callBudget: TimeInterval?
    /// Nodes the app may keep (default 200 000).
    public var maxNodes: UInt32?

    public init(
        memoryMB: UInt32? = nil,
        stackKB: UInt32? = nil,
        loadBudget: TimeInterval? = nil,
        callBudget: TimeInterval? = nil,
        maxNodes: UInt32? = nil
    ) {
        self.memoryMB = memoryMB
        self.stackKB = stackKB
        self.loadBudget = loadBudget
        self.callBudget = callBudget
        self.maxNodes = maxNodes
    }
}

/// Holds an error handler for the C callback.
private final class ErrorHandlerBox {
    let handler: (String) -> Void
    init(_ handler: @escaping (String) -> Void) { self.handler = handler }
}

/// A view running a nsh app. Add it to any hierarchy, then load a bundle,
/// a signed package or (with `NativeShellDev.xcframework`) a dev server.
/// Loading again replaces the running app; the app stops when the view is
/// deallocated.
@MainActor
public final class NshAppView: UIView {
    private let app: OpaquePointer

    /// Creates the view. `trace` logs per-commit timings.
    public init(frame: CGRect = .zero, trace: Bool = false) {
        app = nsh_app_create(frame, trace)
        super.init(frame: frame)
        attach()
    }

    public required init?(coder: NSCoder) {
        app = nsh_app_create(.zero, false)
        super.init(coder: coder)
        attach()
    }

    private func attach() {
        let content = nsh_app_view(app)
        content.frame = bounds
        content.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(content)
    }

    deinit {
        // UIKit releases views on the main thread.
        nsh_app_destroy(app)
    }

    /// Receives fatal app errors (load failures, uncaught JS exceptions) on
    /// the main thread. They are always logged; development builds also
    /// show them over the app.
    public var onError: ((String) -> Void)? {
        didSet {
            guard let onError else {
                nsh_app_set_error_handler(app, nil, nil, nil)
                return
            }
            let box = Unmanaged.passRetained(ErrorHandlerBox(onError)).toOpaque()
            nsh_app_set_error_handler(
                app,
                { context, message in
                    let box = Unmanaged<ErrorHandlerBox>.fromOpaque(context!).takeUnretainedValue()
                    box.handler(String(cString: message))
                },
                box,
                { context in Unmanaged<ErrorHandlerBox>.fromOpaque(context!).release() }
            )
        }
    }

    /// The VM limits of the apps loaded from now on.
    public var limits = NshLimits() {
        didSet {
            let ms = { (seconds: TimeInterval?) -> UInt32 in
                seconds.map { UInt32(clamping: Int(($0 * 1000).rounded())) } ?? 0
            }
            nsh_app_set_limits(
                app, limits.memoryMB ?? 0, limits.stackKB ?? 0, ms(limits.loadBudget),
                ms(limits.callBudget), limits.maxNodes ?? 0)
        }
    }

    /// Grants the apps loaded from now on at most these capabilities: JSON in
    /// the form of `nsh.json`'s `capabilities` (`nil`: what each app
    /// declares). A request must be allowed by both.
    public func grantCapabilities(_ json: String?) throws {
        try check(nsh_app_set_granted_capabilities(app, json))
    }

    /// Runs a bundle written by `nsh build`; assets come from `assets` next
    /// to it. Bytecode (`bundle.qbc`) loads only from inside the app bundle;
    /// elsewhere load `bundle.js`, or use signed packages.
    public func loadBundle(at url: URL) throws {
        try check(nsh_app_load_bundle(app, url.path))
    }

    /// Runs a verified package (``NshTrust/openPackage(at:)``).
    public func run(_ package: NshPackage) {
        guard let handle = package.take() else {
            onError?("the package already ran")
            return
        }
        nsh_app_run_package(app, handle)
    }

    /// Verifies the signed package at `url` with `trust` (off the main
    /// thread) and runs it. Throws why it was refused; the running app stays.
    @discardableResult
    public func loadPackage(at url: URL, trust: NshTrust) async throws -> NshPackage.Info {
        let package = try await trust.openPackage(at: url)
        run(package)
        return package.info
    }

    /// Development: loads from a `nsh dev` server (`ws://host:port/ws`) and
    /// reloads on every change. Throws without development support
    /// (`NativeShell.xcframework` rather than `NativeShellDev`).
    public func connectDevServer(_ url: URL) throws {
        try check(nsh_app_connect_dev_server(app, url.absoluteString))
    }

    /// The status bar style the running apps asked for: return it from the
    /// host view controller's `preferredStatusBarStyle`.
    public static var preferredStatusBarStyle: UIStatusBarStyle { nsh_status_bar_style() }

    /// Whether the running apps hid the status bar (`prefersStatusBarHidden`).
    public static var prefersStatusBarHidden: Bool { nsh_status_bar_hidden() }

    /// How status bar changes animate (`preferredStatusBarUpdateAnimation`).
    public static var preferredStatusBarUpdateAnimation: UIStatusBarAnimation {
        nsh_status_bar_animation()
    }
}

/// The root keys, revocations and accepted builds that signed packages are
/// verified against. The newest revocation list and the highest build
/// accepted per app persist in its state directory, and packages' assets are
/// extracted under it. Thread-safe.
public final class NshTrust: @unchecked Sendable {
    private let handle: OpaquePointer

    /// Opens (or creates) the store in `stateDirectory`, e.g. under
    /// Application Support. It trusts no key until `addRoot`.
    public init(stateDirectory: URL) throws {
        var error: UnsafeMutablePointer<CChar>?
        guard let handle = nsh_trust_create(stateDirectory.path, &error) else {
            try check(error)
            throw NshError(message: "cannot open the trust store")
        }
        self.handle = handle
    }

    deinit {
        nsh_trust_destroy(handle)
    }

    /// Trusts a root public key (base64, `<name>.pub` from `nsh keygen`):
    /// packages whose release key it certified load.
    public func addRoot(_ publicKey: String) throws {
        try check(nsh_trust_add_root(handle, publicKey.trimmingCharacters(in: .whitespacesAndNewlines)))
    }

    /// Applies a revocation list (the JSON `nsh revoke` writes) signed by a
    /// trusted root; it must be newer than the one the store has.
    public func updateRevocations(_ json: String) throws {
        try check(nsh_trust_update_revocations(handle, json))
    }

    /// Verifies the signed package at `url` (`.nshpkg`, from `nsh publish`)
    /// off the calling thread: its release key must be certified by a trusted
    /// root for its app id, not revoked, and its build not older than one
    /// accepted before. Extracts its assets and accepts it (older builds of
    /// the app are refused from now on). Run it with `NshAppView.run`.
    public func openPackage(at url: URL) async throws -> NshPackage {
        let path = url.path
        return try await Task.detached(priority: .userInitiated) {
            var error: UnsafeMutablePointer<CChar>?
            guard let package = nsh_trust_open_package(self.handle, path, &error) else {
                try check(error)
                throw NshError(message: "refused package")
            }
            return NshPackage(package)
        }.value
    }
}

/// A verified signed package, ready to run once (`NshAppView.run`).
public final class NshPackage: @unchecked Sendable {
    /// The app a package holds.
    public struct Info: Sendable, Equatable {
        /// Its id (`com.example.app`).
        public let id: String
        /// Its display name.
        public let name: String
        /// Its version string.
        public let version: String
        /// Its build number.
        public let build: UInt64
    }

    /// The app it holds.
    public let info: Info
    private var handle: OpaquePointer?
    private let lock = NSLock()

    fileprivate init(_ handle: OpaquePointer) {
        self.handle = handle
        info = Info(
            id: String(cString: nsh_package_app_id(handle)),
            name: String(cString: nsh_package_app_name(handle)),
            version: String(cString: nsh_package_app_version(handle)),
            build: nsh_package_app_build(handle))
    }

    fileprivate func take() -> OpaquePointer? {
        lock.lock()
        defer { lock.unlock() }
        let taken = handle
        handle = nil
        return taken
    }

    deinit {
        nsh_package_free(handle)
    }
}
