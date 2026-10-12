import AppKit
import Darwin

// The native app owns the local server; the editor still runs in the user's browser.
final class Launcher: NSObject, NSApplicationDelegate {
    private var server: Process?
    private var log: FileHandle?
    private var logURL: URL?
    private var statusItem: NSStatusItem?
    private var signals: [DispatchSourceSignal] = []
    private var quitting = false
    private var lastErrorShown = false
    private let arguments: [String]

    init(arguments: [String]) { self.arguments = arguments }

    private var backend: URL {
        Bundle.main.bundleURL.appendingPathComponent("Contents/MacOS/gitleaf-server")
    }

    private func environment() -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        // Finder's PATH omits common Git, TeX Live and Homebrew installations.
        let candidates = [env["PATH"] ?? "", "/opt/homebrew/bin", "/usr/local/bin",
                          "/Library/TeX/texbin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
        var paths: [String] = []
        for path in candidates.flatMap({ $0.split(separator: ":").map(String.init) }) {
            if !paths.contains(path) { paths.append(path) }
        }
        env["PATH"] = paths.joined(separator: ":")
        return env
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let menu = NSMenu()
        let open = NSMenuItem(title: "Open GitLeaf in Browser", action: #selector(openBrowser), keyEquivalent: "o")
        open.target = self
        menu.addItem(open)
        let logs = NSMenuItem(title: "Show GitLeaf Log", action: #selector(showLog), keyEquivalent: "l")
        logs.target = self
        menu.addItem(logs)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit GitLeaf", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        let main = NSMenu()
        let appMenu = NSMenuItem()
        appMenu.submenu = menu
        main.addItem(appMenu)
        NSApp.mainMenu = main
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        statusItem?.button?.image = NSImage(systemSymbolName: "leaf", accessibilityDescription: "GitLeaf")
        statusItem?.menu = menu.copy() as? NSMenu

        // Start the child before installing ignored signal dispositions in the launcher.
        startServer()

        // A normal Quit and signals from the system both drain the backend's requests.
        for number in [SIGTERM, SIGINT] {
            signal(number, SIG_IGN)
            let source = DispatchSource.makeSignalSource(signal: number, queue: .main)
            source.setEventHandler {
                // AppKit may enter a nested run loop while Quit awaits the server.
                // Start Quit outside the dispatch callback so completion can reach the main queue.
                RunLoop.main.perform(inModes: [.common]) { NSApp.terminate(nil) }
            }
            source.resume()
            signals.append(source)
        }
    }

    private func startServer() {
        do {
            let folder = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Logs/GitLeaf")
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let destination = folder.appendingPathComponent("gitleaf.log")
            logURL = destination
            // Only startup/shutdown diagnostics are logged; don't keep growing the file forever.
            if let size = try? destination.resourceValues(forKeys: [.fileSizeKey]).fileSize, size > 2 * 1024 * 1024 {
                let previous = folder.appendingPathComponent("gitleaf.previous.log")
                try? FileManager.default.removeItem(at: previous)
                try FileManager.default.moveItem(at: destination, to: previous)
            }
            if !FileManager.default.fileExists(atPath: destination.path) {
                FileManager.default.createFile(atPath: destination.path, contents: nil)
            }
            log = try FileHandle(forWritingTo: destination)
            log?.seekToEndOfFile()
            log?.write(Data("\n--- GitLeaf started \(Date()) ---\n".utf8))
            let process = Process()
            process.executableURL = backend
            process.arguments = arguments
            process.environment = environment()
            process.currentDirectoryURL = FileManager.default.homeDirectoryForCurrentUser
            process.standardOutput = log
            process.standardError = log
            process.standardInput = FileHandle.nullDevice
            process.terminationHandler = { [weak self] process in
                DispatchQueue.main.async { self?.serverStopped(process) }
            }
            server = process
            try process.run()
        } catch {
            server = nil
            showFailure(error.localizedDescription)
        }
    }

    private func serverStopped(_ process: Process) {
        log?.write(Data("Local server stopped (status \(process.terminationStatus)).\n".utf8))
        if quitting { return }
        if process.terminationStatus == 0 {
            // A matching server was already running and opened the browser for us.
            NSApp.terminate(nil)
            return
        }
        let contents = logURL.flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? ""
        let latest = contents.components(separatedBy: "--- GitLeaf started ").last ?? contents
        showFailure(String(latest.suffix(3000)))
    }

    private func showFailure(_ message: String) {
        guard !lastErrorShown else { return }
        lastErrorShown = true
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "GitLeaf could not start"
        alert.informativeText = message + "\n\nYour papers are unchanged. See the setup guide or log for help."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Open Setup Guide")
        alert.addButton(withTitle: "Show Log")
        alert.addButton(withTitle: "Close")
        let choice = alert.runModal()
        if choice == .alertFirstButtonReturn {
            NSWorkspace.shared.open(URL(string: "https://github.com/kmarkiv/gitleaf#get-started")!)
        } else if choice == .alertSecondButtonReturn { showLog() }
        NSApp.terminate(nil)
    }

    @objc func openBrowser() {
        // Ask the binary to verify the localhost instance before opening the browser.
        guard !quitting else { return }
        let process = Process()
        process.executableURL = backend
        process.arguments = arguments.filter { $0 != "--no-open" }
        var env = environment()
        env.removeValue(forKey: "GITLEAF_NO_OPEN")
        process.environment = env
        process.currentDirectoryURL = FileManager.default.homeDirectoryForCurrentUser
        process.standardOutput = log
        process.standardError = log
        do { try process.run() } catch { showFailure(error.localizedDescription) }
    }

    @objc func showLog() {
        if let url = logURL { NSWorkspace.shared.open(url) }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        openBrowser()
        return false
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let process = server, process.isRunning else { return .terminateNow }
        if !quitting {
            quitting = true
            log?.write(Data("Quit requested; waiting for active work to finish.\n".utf8))
            process.terminate()
            DispatchQueue.global(qos: .userInitiated).async {
                process.waitUntilExit()
                DispatchQueue.main.async { sender.reply(toApplicationShouldTerminate: true) }
            }
        }
        return .terminateLater
    }

    func applicationWillTerminate(_ notification: Notification) {
        log?.closeFile()
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let launcher = Launcher(arguments: Array(CommandLine.arguments.dropFirst()))
app.delegate = launcher
withExtendedLifetime(launcher) { app.run() }
