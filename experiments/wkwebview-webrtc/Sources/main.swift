import AppKit
import Darwin
import Foundation
import WebKit

private enum ProbeError: Error, CustomStringConvertible {
    case invalidArgument(String)
    case missingArgument(String)

    var description: String {
        switch self {
        case let .invalidArgument(value):
            return "invalid argument: \(value)"
        case let .missingArgument(name):
            return "missing value for \(name)"
        }
    }
}

private struct Options {
    var htmlURL = URL(fileURLWithPath: "Web/index.html").standardizedFileURL
    var outputURL = URL(fileURLWithPath: "results/wkwebview-loopback.json").standardizedFileURL
    var environmentURL = URL(fileURLWithPath: ".build/environment.txt").standardizedFileURL
    var durationSeconds = 30.0

    static func parse(arguments: [String]) throws -> Options {
        var options = Options()
        var index = 1

        while index < arguments.count {
            let argument = arguments[index]
            switch argument {
            case "--html":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument(argument) }
                options.htmlURL = URL(fileURLWithPath: arguments[index]).standardizedFileURL
            case "--output":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument(argument) }
                options.outputURL = URL(fileURLWithPath: arguments[index]).standardizedFileURL
            case "--environment":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument(argument) }
                options.environmentURL = URL(fileURLWithPath: arguments[index]).standardizedFileURL
            case "--duration-seconds":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument(argument) }
                guard let duration = Double(arguments[index]), duration > 0, duration <= 3_600 else {
                    throw ProbeError.invalidArgument("--duration-seconds must be in (0, 3600]")
                }
                options.durationSeconds = duration
            case "--help", "-h":
                print("Usage: wkwebview-webrtc-probe [--html PATH] [--output PATH] [--environment PATH] [--duration-seconds N]")
                exit(0)
            default:
                throw ProbeError.invalidArgument(argument)
            }
            index += 1
        }

        return options
    }
}

private struct TaskUsage {
    let cpuSeconds: Double
    let residentBytes: UInt64
}

private final class ProbeDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate, WKNavigationDelegate,
    WKScriptMessageHandler {
    private let options: Options
    private var window: NSWindow?
    private var webView: WKWebView?
    private var sampleTimer: Timer?
    private var timeoutWorkItem: DispatchWorkItem?
    private var samples: [[String: Any]] = []
    private var webKitProcessSamples: [[[String: Any]]] = []
    private var previousUsage: TaskUsage?
    private var previousSampleDate: Date?
    private var runStartedAt: Date?
    private var baselineProcessIDs: Set<Int> = []
    private var finished = false

    init(options: Options) {
        self.options = options
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = notification
        NSApp.setActivationPolicy(.regular)

        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.mediaTypesRequiringUserActionForPlayback = []
        configuration.userContentController.add(self, name: "probe")

        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = self
        view.setValue(false, forKey: "drawsBackground")
        webView = view

        let newWindow = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1_120, height: 760),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        newWindow.title = "iDroid Kill Switch 1 — WKWebView WebRTC"
        newWindow.contentView = view
        newWindow.delegate = self
        newWindow.center()
        newWindow.makeKeyAndOrderFront(nil)
        window = newWindow
        NSApp.activate(ignoringOtherApps: true)
        baselineProcessIDs = allProcessIDs()

        guard FileManager.default.fileExists(atPath: options.htmlURL.path) else {
            finish(status: "NOT_TESTED", error: "HTML file does not exist: \(options.htmlURL.path)")
            return
        }

        view.loadFileURL(options.htmlURL, allowingReadAccessTo: options.htmlURL.deletingLastPathComponent())
    }

    func applicationWillTerminate(_ notification: Notification) {
        _ = notification
        sampleTimer?.invalidate()
        timeoutWorkItem?.cancel()
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        _ = sender
        if !finished {
            finish(status: "ABORTED", error: "window closed before the probe completed")
        }
        return true
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        _ = webView
        _ = navigation
        let duration = String(format: "%.3f", options.durationSeconds)
        // Keep the returned value bridgeable. `startProbe` is async and its
        // Promise is not an Objective-C JavaScript result type.
        self.webView?.evaluateJavaScript("window.startProbe(\(duration)); 0;") { [weak self] _, error in
            if let error {
                self?.finish(status: "NOT_TESTED", error: "startProbe injection failed: \(error.localizedDescription)")
            }
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        _ = webView
        _ = navigation
        finish(status: "NOT_TESTED", error: "WKWebView navigation failed: \(error.localizedDescription)")
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        _ = webView
        _ = navigation
        finish(status: "NOT_TESTED", error: "WKWebView provisional navigation failed: \(error.localizedDescription)")
    }

    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        _ = userContentController
        guard let body = message.body as? [String: Any], let type = body["type"] as? String else {
            finish(status: "FAIL", error: "probe message was not a dictionary with a type")
            return
        }

        switch type {
        case "started":
            beginSampling()
        case "result":
            let result = body["result"] as? [String: Any] ?? [:]
            let status = body["status"] as? String ?? "COMPLETED"
            finish(status: status, pageResult: result, error: body["error"] as? String)
        case "error":
            finish(status: "FAIL", error: body["error"] as? String ?? "page reported an unspecified error")
        default:
            finish(status: "FAIL", error: "unknown probe message type: \(type)")
        }
    }

    private func beginSampling() {
        guard runStartedAt == nil else { return }
        runStartedAt = Date()
        previousUsage = nil
        previousSampleDate = nil
        sampleProcess()
        sampleTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.sampleProcess()
        }

        let timeout = DispatchWorkItem { [weak self] in
            guard let self, !self.finished else { return }
            self.finish(status: "TIMEOUT", error: "page did not return a result before the native timeout")
        }
        timeoutWorkItem = timeout
        DispatchQueue.main.asyncAfter(deadline: .now() + options.durationSeconds + 15, execute: timeout)
    }

    private func sampleProcess() {
        guard let usage = currentTaskUsage() else { return }
        let now = Date()
        var cpuPercent: Double?
        if let previousUsage, let previousSampleDate {
            let wallSeconds = now.timeIntervalSince(previousSampleDate)
            if wallSeconds > 0 {
                cpuPercent = max(0, (usage.cpuSeconds - previousUsage.cpuSeconds) / wallSeconds * 100)
            }
        }

        let elapsed = runStartedAt.map { now.timeIntervalSince($0) } ?? 0
        var sample: [String: Any] = [
            "elapsedSeconds": elapsed,
            "residentBytes": usage.residentBytes,
        ]
        if let cpuPercent {
            sample["cpuPercent"] = cpuPercent
        } else {
            sample["cpuPercent"] = NSNull()
        }
        samples.append(sample)
        webKitProcessSamples.append(relevantWebKitProcesses(excluding: baselineProcessIDs))
        previousUsage = usage
        previousSampleDate = now
    }

    private func finish(
        status: String,
        pageResult: [String: Any] = [:],
        error: String? = nil
    ) {
        guard !finished else { return }
        finished = true
        sampleTimer?.invalidate()
        timeoutWorkItem?.cancel()
        sampleProcess()

        var report: [String: Any] = [
            "experiment": "kill-switch-1-wkwebview-webrtc",
            "status": status,
            "durationSeconds": options.durationSeconds,
            "platform": [
                "os": ProcessInfo.processInfo.operatingSystemVersionString,
                "processArchitecture": architectureName(),
            ],
            "environmentRecord": readText(at: options.environmentURL),
            "processSamples": samples,
            "webKitProcessSamples": webKitProcessSamples,
            "webKitProcessBaselineCount": baselineProcessIDs.count,
            "resourceMeasurementLimits": "processSamples cover this native host; webKitProcessSamples are best-effort baseline-delta ps matches for WebKit/WebContent/GPU names and are not a complete whole-application attribution",
            "webview": pageResult,
        ]
        if let error {
            report["error"] = error
        }

        do {
            let parent = options.outputURL.deletingLastPathComponent()
            try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
            let data = try JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: options.outputURL, options: .atomic)
            print("Probe report: \(options.outputURL.path)")
        } catch {
            fputs("ERROR: could not write probe report: \(error)\n", stderr)
        }

        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
            NSApp.terminate(nil)
        }
    }
}

private func currentTaskUsage() -> TaskUsage? {
    var info = mach_task_basic_info()
    var count = mach_msg_type_number_t(MemoryLayout<mach_task_basic_info>.size) / 4
    let result = withUnsafeMutablePointer(to: &info) { pointer in
        pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(MACH_TASK_BASIC_INFO), $0, &count)
        }
    }

    guard result == KERN_SUCCESS else { return nil }
    let user = Double(info.user_time.seconds) + Double(info.user_time.microseconds) / 1_000_000
    let system = Double(info.system_time.seconds) + Double(info.system_time.microseconds) / 1_000_000
    return TaskUsage(cpuSeconds: user + system, residentBytes: UInt64(info.resident_size))
}

private func allProcessIDs() -> Set<Int> {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/ps")
    process.arguments = ["-axo", "pid="]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice

    do {
        try process.run()
    } catch {
        return []
    }

    let data = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard let text = String(data: data, encoding: .utf8) else { return [] }
    return Set(text.split(whereSeparator: \.isNewline).compactMap { Int($0.trimmingCharacters(in: .whitespaces)) })
}

private func relevantWebKitProcesses(excluding baselineProcessIDs: Set<Int>) -> [[String: Any]] {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/ps")
    process.arguments = ["-axo", "pid=,ppid=,%cpu=,rss=,comm=,args="]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice

    do {
        try process.run()
    } catch {
        return []
    }

    let data = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard let text = String(data: data, encoding: .utf8) else { return [] }

    return text.split(whereSeparator: \.isNewline).compactMap { line in
        let fields = line.split(
            maxSplits: 5,
            omittingEmptySubsequences: true,
            whereSeparator: { $0 == " " || $0 == "\t" }
        )
        guard fields.count >= 5,
              let pid = Int(fields[0]),
              let parentPid = Int(fields[1]),
              let cpuPercent = Double(fields[2]),
              let residentKilobytes = Int(fields[3]) else {
            return nil
        }

        guard !baselineProcessIDs.contains(pid) else { return nil }

        let command = String(fields[4])
        let arguments = fields.count == 6 ? String(fields[5]) : ""
        let searchable = "\(command) \(arguments)".lowercased()
        guard searchable.contains("webkit") || searchable.contains("webcontent") || searchable.contains("gpu") else {
            return nil
        }

        return [
            "pid": pid,
            "parentPid": parentPid,
            "cpuPercent": cpuPercent,
            "residentKilobytes": residentKilobytes,
            "command": command,
            "arguments": arguments,
        ]
    }
}

private func readText(at url: URL) -> String {
    (try? String(contentsOf: url, encoding: .utf8)) ?? "NOT_AVAILABLE"
}

private func architectureName() -> String {
    #if arch(arm64)
    return "arm64"
    #elseif arch(x86_64)
    return "x86_64"
    #else
    return "unknown"
    #endif
}

@main
private struct Launcher {
    static func main() {
        do {
            let options = try Options.parse(arguments: CommandLine.arguments)
            let application = NSApplication.shared
            let delegate = ProbeDelegate(options: options)
            application.delegate = delegate
            application.run()
        } catch {
            fputs("ERROR: \(error)\n", stderr)
            exit(2)
        }
    }
}
