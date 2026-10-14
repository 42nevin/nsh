// An existing UIKit app that embeds a yf app below its own native UI.
//
// Build: see README.md. The yf bundle (`yf build`) is copied into the app
// as `yf/bundle.qbc` (+ `yf/assets`).

import UIKit
import YF

final class EmbedController: UIViewController {
    private var app: OpaquePointer?
    private let reloads = UILabel()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground

        // Native UI of the host app.
        let title = UILabel()
        title.text = "Native host app"
        title.font = .boldSystemFont(ofSize: 24)
        let reload = UIButton(type: .system)
        reload.setTitle("Reload yf", for: .normal)
        reload.addTarget(self, action: #selector(load), for: .touchUpInside)
        reloads.font = .systemFont(ofSize: 13)
        reloads.textColor = .secondaryLabel
        let header = UIStackView(arrangedSubviews: [title, reload, reloads])
        header.axis = .vertical
        header.alignment = .leading
        header.spacing = 6
        header.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(header)

        // The embedded yf app.
        let container = UIView()
        container.translatesAutoresizingMaskIntoConstraints = false
        container.layer.borderColor = UIColor.systemPurple.cgColor
        container.layer.borderWidth = 2
        container.clipsToBounds = true
        view.addSubview(container)

        NSLayoutConstraint.activate([
            header.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 12),
            header.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            container.topAnchor.constraint(equalTo: header.bottomAnchor, constant: 12),
            container.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
            container.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
            container.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -12),
        ])

        let app = yf_app_create(container.bounds, false)
        self.app = app
        let yfView = yf_app_view(app)
        yfView.frame = container.bounds
        container.addSubview(yfView)
        yf_app_set_error_handler(app, { _, message in
            print("yf error: \(String(cString: message))")
        }, nil)
        load()
    }

    private var count = 0

    @objc private func load() {
        guard let app, let path = Bundle.main.path(forResource: "bundle", ofType: "qbc", inDirectory: "yf") else {
            return
        }
        count += 1
        reloads.text = "loaded \(count)×"
        yf_app_load_bundle(app, path)
    }

    // Let the embedded app's `StatusBar` set the status bar.
    override var preferredStatusBarStyle: UIStatusBarStyle { yf_status_bar_style() }
    override var prefersStatusBarHidden: Bool { yf_status_bar_hidden() }
    override var preferredStatusBarUpdateAnimation: UIStatusBarAnimation { yf_status_bar_animation() }

    deinit {
        yf_app_destroy(app)
    }
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = EmbedController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
