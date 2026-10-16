import AVFoundation
import AppKit
import CoreMedia
import CoreVideo
import Darwin
import Foundation
import QuartzCore

private let videoWidth = 640
private let videoHeight = 360
private let targetFrameRate = 30
private let inputAcknowledgementTimeoutMs = 1_000.0

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
    var outputURL = URL(fileURLWithPath: "results/native-video-presentation.json").standardizedFileURL
    var environmentURL = URL(fileURLWithPath: ".build/environment.txt").standardizedFileURL
    var durationSeconds = 30.0

    static func parse(arguments: [String]) throws -> Options {
        var options = Options()
        var index = 1
        while index < arguments.count {
            switch arguments[index] {
            case "--output":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument("--output") }
                options.outputURL = URL(fileURLWithPath: arguments[index]).standardizedFileURL
            case "--environment":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument("--environment") }
                options.environmentURL = URL(fileURLWithPath: arguments[index]).standardizedFileURL
            case "--duration-seconds":
                index += 1
                guard index < arguments.count else { throw ProbeError.missingArgument("--duration-seconds") }
                guard let duration = Double(arguments[index]), duration > 0, duration <= 3_600 else {
                    throw ProbeError.invalidArgument("--duration-seconds must be in (0, 3600]")
                }
                options.durationSeconds = duration
            case "--help", "-h":
                print("Usage: native-video-presentation-probe [--output PATH] [--environment PATH] [--duration-seconds N]")
                exit(0)
            default:
                throw ProbeError.invalidArgument(arguments[index])
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

private final class InputSampleRecord {
    let sequence: Int
    let kind: String
    let trusted: Bool
    let handlerAt: UInt64
    var acknowledged = false
    var missingAcknowledgement = false
    var timedOut = false
    var frameBoundaryMs: Double?

    init(sequence: Int, kind: String, trusted: Bool, handlerAt: UInt64) {
        self.sequence = sequence
        self.kind = kind
        self.trusted = trusted
        self.handlerAt = handlerAt
    }
}

private final class InputTargetView: NSView {
    var onInput: ((String, Bool) -> Void)?
    var phase: CGFloat = 0

    override var acceptsFirstResponder: Bool { true }
    override var canBecomeKeyView: Bool { true }

    override func mouseDown(with event: NSEvent) {
        _ = event
        window?.makeFirstResponder(self)
        onInput?("manual-click", true)
    }

    override func keyDown(with event: NSEvent) {
        _ = event
        onInput?("manual-key", true)
    }

    override func draw(_ dirtyRect: NSRect) {
        _ = dirtyRect
        NSColor(calibratedWhite: 0.10, alpha: 1).setFill()
        bounds.fill()

        let border = NSBezierPath(roundedRect: bounds.insetBy(dx: 1, dy: 1), xRadius: 10, yRadius: 10)
        NSColor(calibratedRed: 0.43, green: 0.62, blue: 1, alpha: 1).setStroke()
        border.lineWidth = 2
        border.stroke()

        let sweepWidth = bounds.width * 0.24
        let sweepX = (bounds.width + sweepWidth) * phase - sweepWidth
        let sweep = NSRect(x: sweepX, y: 0, width: sweepWidth, height: bounds.height)
        NSColor(calibratedRed: 0.43, green: 0.62, blue: 1, alpha: 0.22).setFill()
        sweep.fill()

        let text = "Input target — click at least ten times or focus and press keys"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 18, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        let size = (text as NSString).size(withAttributes: attributes)
        (text as NSString).draw(
            at: NSPoint(x: max(16, (bounds.width - size.width) / 2), y: (bounds.height - size.height) / 2),
            withAttributes: attributes
        )
    }
}

private final class ProbeRootView: NSView {
    let displayLayer = AVSampleBufferDisplayLayer()
    let inputTarget = InputTargetView(frame: .zero)

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer = CALayer()
        layer?.backgroundColor = NSColor(calibratedWhite: 0.055, alpha: 1).cgColor
        displayLayer.videoGravity = .resizeAspect
        displayLayer.backgroundColor = NSColor.black.cgColor
        layer?.addSublayer(displayLayer)
        addSubview(inputTarget)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layout() {
        super.layout()
        let margin: CGFloat = 40
        let inputHeight: CGFloat = 100
        let inputY: CGFloat = 28
        inputTarget.frame = NSRect(x: margin, y: inputY, width: bounds.width - margin * 2, height: inputHeight)
        let videoY = inputY + inputHeight + 28
        displayLayer.frame = NSRect(x: margin, y: videoY, width: bounds.width - margin * 2, height: bounds.height - videoY - 28)
    }
}

private final class NativeProbeDelegate: NSObject, NSApplicationDelegate, NSWindowDelegate {
    private let options: Options
    private var window: NSWindow?
    private var rootView: ProbeRootView?
    private var frameTimer: Timer?
    private var resourceTimer: Timer?
    private var inputTimeoutTimer: Timer?
    private var phaseTimer: Timer?
    private var timeoutWorkItem: DispatchWorkItem?
    private var readyObserver: NSObjectProtocol?
    private var displayLink: CVDisplayLink?
    private var startNanos: UInt64 = 0
    private var finished = false

    private var formatDescription: CMVideoFormatDescription?
    private var timebase: CMTimebase?
    private var frameIndex = 0
    private var framesGenerated = 0
    private var framesEnqueued = 0
    private var backpressureCount = 0
    private var enqueueFailureCount = 0
    private var firstEnqueueMs: Double?
    private var firstReadyForDisplayMs: Double?
    private var displayTickCount = 0
    private var displayTickIntervalsMs: [Double] = []
    private var lastDisplayTickNanos: UInt64?
    private var setupReadyMs: Double?
    private var failureReason: String?
    private var resourceSamples: [[String: Any]] = []
    private var relevantProcessSamples: [[[String: Any]]] = []
    private var baselineProcessIDs: Set<Int> = []

    private var nextInputSequence = 1
    private var scriptedCount = 0
    private var manualCount = 0
    private var pendingInputs: [InputSampleRecord] = []
    private var inputSamples: [InputSampleRecord] = []

    init(options: Options) {
        self.options = options
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = notification
        startNanos = DispatchTime.now().uptimeNanoseconds
        baselineProcessIDs = allProcessIDs()
        NSApp.setActivationPolicy(.regular)

        let content = ProbeRootView(frame: NSRect(x: 0, y: 0, width: 1_120, height: 760))
        content.inputTarget.onInput = { [weak self] kind, trusted in
            self?.recordInput(kind: kind, trusted: trusted)
        }
        rootView = content

        let newWindow = NSWindow(
            contentRect: content.frame,
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        newWindow.title = "iDroid Native Video Presentation Baseline"
        newWindow.contentView = content
        newWindow.delegate = self
        newWindow.center()
        newWindow.makeKeyAndOrderFront(nil)
        window = newWindow
        NSApp.activate(ignoringOtherApps: true)

        beginProbe()
    }

    func applicationWillTerminate(_ notification: Notification) {
        _ = notification
        stopTimers()
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        _ = sender
        if !finished {
            finish(status: "ABORTED", error: "window closed before the probe completed")
        }
        return true
    }

    private func beginProbe() {
        guard let rootView else {
            finish(status: "FAIL", error: "root view was not created")
            return
        }

        readyObserver = NotificationCenter.default.addObserver(
            forName: Notification.Name("AVSampleBufferDisplayLayerReadyForDisplayDidChangeNotification"),
            object: rootView.displayLayer,
            queue: .main
        ) { [weak self] _ in
            self?.observeReadyForDisplay()
        }

        do {
            try configureDisplayLayer(rootView.displayLayer)
            try configureDisplayLink()
        } catch {
            finish(status: "NOT_TESTED", error: String(describing: error))
            return
        }

        setupReadyMs = elapsedMs()
        enqueueFrame()
        frameTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / Double(targetFrameRate), repeats: true) { [weak self] _ in
            self?.enqueueFrame()
        }
        resourceTimer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.sampleResources()
        }
        inputTimeoutTimer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { [weak self] _ in
            self?.markInputTimeouts()
        }
        phaseTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { [weak self] _ in
            guard let self, let inputTarget = self.rootView?.inputTarget else { return }
            inputTarget.phase = (inputTarget.phase + 0.017).truncatingRemainder(dividingBy: 1)
            inputTarget.needsDisplay = true
        }
        scheduleScriptedInputs()
        sampleResources()

        let timeout = DispatchWorkItem { [weak self] in
            guard let self, !self.finished else { return }
            self.finish(status: "TIMEOUT", error: "native presentation probe exceeded its bounded timeout")
        }
        timeoutWorkItem = timeout
        DispatchQueue.main.asyncAfter(deadline: .now() + options.durationSeconds + 15, execute: timeout)
        DispatchQueue.main.asyncAfter(deadline: .now() + options.durationSeconds) { [weak self] in
            guard let self, !self.finished else { return }
            self.finish(status: "COMPLETED")
        }
    }

    private func configureDisplayLayer(_ displayLayer: AVSampleBufferDisplayLayer) throws {
        var createdTimebase: CMTimebase?
        let timebaseStatus = CMTimebaseCreateWithSourceClock(
            allocator: kCFAllocatorDefault,
            sourceClock: CMClockGetHostTimeClock(),
            timebaseOut: &createdTimebase
        )
        guard timebaseStatus == noErr, let createdTimebase else {
            throw ProbeError.invalidArgument("could not create a host-clock timebase")
        }
        timebase = createdTimebase
        CMTimebaseSetTime(createdTimebase, time: .zero)
        CMTimebaseSetRate(createdTimebase, rate: 1)
        displayLayer.controlTimebase = createdTimebase
        displayLayer.videoGravity = .resizeAspect
        displayLayer.preventsDisplaySleepDuringVideoPlayback = true
    }

    private func configureDisplayLink() throws {
        var link: CVDisplayLink?
        guard CVDisplayLinkCreateWithActiveCGDisplays(&link) == kCVReturnSuccess, let link else {
            throw ProbeError.invalidArgument("CVDisplayLink is unavailable")
        }
        let callback: CVDisplayLinkOutputCallback = { _, _, _, _, _, userInfo in
            guard let userInfo else { return kCVReturnSuccess }
            let delegate = Unmanaged<NativeProbeDelegate>.fromOpaque(userInfo).takeUnretainedValue()
            delegate.displayTick()
            return kCVReturnSuccess
        }
        let callbackStatus = CVDisplayLinkSetOutputCallback(link, callback, Unmanaged.passUnretained(self).toOpaque())
        guard callbackStatus == kCVReturnSuccess else {
            throw ProbeError.invalidArgument("CVDisplayLink callback could not be installed")
        }
        displayLink = link
        guard CVDisplayLinkStart(link) == kCVReturnSuccess else {
            throw ProbeError.invalidArgument("CVDisplayLink could not start")
        }
    }

    private func displayTick() {
        let now = DispatchTime.now().uptimeNanoseconds
        DispatchQueue.main.async { [weak self] in
            guard let self, !self.finished else { return }
            self.recordDisplayTick(at: now)
        }
    }

    private func recordDisplayTick(at now: UInt64) {
        displayTickCount += 1
        if let lastDisplayTickNanos {
            displayTickIntervalsMs.append(Double(now - lastDisplayTickNanos) / 1_000_000)
        }
        lastDisplayTickNanos = now
        resolveInputSamples(at: now)
    }

    private func enqueueFrame() {
        guard !finished, let displayLayer = rootView?.displayLayer else { return }
        framesGenerated += 1
        guard displayLayer.status != .failed else {
            enqueueFailureCount += 1
            failureReason = displayLayer.error?.localizedDescription ?? "display layer entered failed state"
            return
        }
        guard displayLayer.isReadyForMoreMediaData else {
            backpressureCount += 1
            frameIndex += 1
            return
        }

        guard let pixelBuffer = makePixelBuffer(frame: frameIndex) else {
            enqueueFailureCount += 1
            failureReason = "CVPixelBufferCreate failed"
            frameIndex += 1
            return
        }
        if formatDescription == nil {
            var description: CMVideoFormatDescription?
            let status = CMVideoFormatDescriptionCreateForImageBuffer(
                allocator: kCFAllocatorDefault,
                imageBuffer: pixelBuffer,
                formatDescriptionOut: &description
            )
            guard status == noErr, let description else {
                enqueueFailureCount += 1
                failureReason = "CMVideoFormatDescriptionCreateForImageBuffer failed: \(status)"
                frameIndex += 1
                return
            }
            formatDescription = description
        }

        var timing = CMSampleTimingInfo(
            duration: CMTime(value: 1, timescale: CMTimeScale(targetFrameRate)),
            presentationTimeStamp: CMTime(value: CMTimeValue(frameIndex), timescale: CMTimeScale(targetFrameRate)),
            decodeTimeStamp: .invalid
        )
        var sampleBuffer: CMSampleBuffer?
        let sampleStatus = CMSampleBufferCreateReadyWithImageBuffer(
            allocator: kCFAllocatorDefault,
            imageBuffer: pixelBuffer,
            formatDescription: formatDescription!,
            sampleTiming: &timing,
            sampleBufferOut: &sampleBuffer
        )
        guard sampleStatus == noErr, let sampleBuffer else {
            enqueueFailureCount += 1
            failureReason = "CMSampleBufferCreateReadyWithImageBuffer failed: \(sampleStatus)"
            frameIndex += 1
            return
        }

        displayLayer.enqueue(sampleBuffer)
        framesEnqueued += 1
        if firstEnqueueMs == nil {
            firstEnqueueMs = elapsedMs()
        }
        observeReadyForDisplay()
        frameIndex += 1
    }

    private func makePixelBuffer(frame: Int) -> CVPixelBuffer? {
        let attributes: [String: Any] = [
            kCVPixelBufferIOSurfacePropertiesKey as String: [:],
            kCVPixelBufferMetalCompatibilityKey as String: true,
        ]
        var pixelBuffer: CVPixelBuffer?
        let status = CVPixelBufferCreate(
            kCFAllocatorDefault,
            videoWidth,
            videoHeight,
            kCVPixelFormatType_32BGRA,
            attributes as CFDictionary,
            &pixelBuffer
        )
        guard status == kCVReturnSuccess, let pixelBuffer else { return nil }
        guard CVPixelBufferLockBaseAddress(pixelBuffer, []) == kCVReturnSuccess else { return nil }
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, []) }
        guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else { return nil }

        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer)
        let pointer = baseAddress.assumingMemoryBound(to: UInt8.self)
        let barX = (frame * 7) % (videoWidth - 120)
        let barY = 92
        for y in 0..<videoHeight {
            let row = pointer.advanced(by: y * bytesPerRow)
            for x in 0..<videoWidth {
                let offset = x * 4
                let inBar = x >= barX && x < barX + 120 && y >= barY && y < barY + 120
                let stripe = ((x + frame * 3) / 32) % 2 == 0
                if inBar {
                    row[offset] = 0xE0
                    row[offset + 1] = 0x9A
                    row[offset + 2] = 0x43
                    row[offset + 3] = 0xFF
                } else if stripe {
                    row[offset] = 0x24
                    row[offset + 1] = 0x1B
                    row[offset + 2] = 0x2A
                    row[offset + 3] = 0xFF
                } else {
                    row[offset] = 0x16
                    row[offset + 1] = 0x12
                    row[offset + 2] = 0x18
                    row[offset + 3] = 0xFF
                }
            }
        }
        return pixelBuffer
    }

    private func observeReadyForDisplay() {
        guard firstReadyForDisplayMs == nil, let displayLayer = rootView?.displayLayer else { return }
        if displayLayer.isReadyForDisplay {
            firstReadyForDisplayMs = elapsedMs()
        }
    }

    private func scheduleScriptedInputs() {
        var index = 0
        Timer.scheduledTimer(withTimeInterval: 0.020, repeats: true) { [weak self] timer in
            guard let self, !self.finished else {
                timer.invalidate()
                return
            }
            guard index < 100 else {
                timer.invalidate()
                return
            }
            index += 1
            self.scriptedCount += 1
            self.recordInput(kind: "scripted", trusted: false)
        }
    }

    private func recordInput(kind: String, trusted: Bool) {
        guard !finished else { return }
        let sample = InputSampleRecord(
            sequence: nextInputSequence,
            kind: kind,
            trusted: trusted,
            handlerAt: DispatchTime.now().uptimeNanoseconds
        )
        nextInputSequence += 1
        if trusted { manualCount += 1 }
        inputSamples.append(sample)
        pendingInputs.append(sample)
    }

    private func resolveInputSamples(at tickNanos: UInt64) {
        let tickMs = elapsedMs(at: tickNanos)
        var remaining: [InputSampleRecord] = []
        for sample in pendingInputs {
            if sample.handlerAt <= tickNanos {
                sample.acknowledged = true
                sample.frameBoundaryMs = tickMs - elapsedMs(at: sample.handlerAt)
            } else {
                remaining.append(sample)
            }
        }
        pendingInputs = remaining
    }

    private func markInputTimeouts() {
        let now = DispatchTime.now().uptimeNanoseconds
        for sample in pendingInputs {
            if Double(now - sample.handlerAt) / 1_000_000 > inputAcknowledgementTimeoutMs {
                sample.timedOut = true
            }
        }
    }

    private func flushPendingInputs() {
        for sample in pendingInputs {
            sample.missingAcknowledgement = true
            sample.timedOut = true
        }
        pendingInputs.removeAll()
    }

    private func elapsedMs() -> Double {
        elapsedMs(at: monotonicNow())
    }

    private func elapsedMs(at nanos: UInt64) -> Double {
        guard nanos >= startNanos else { return 0 }
        return Double(nanos - startNanos) / 1_000_000
    }

    private func sampleResources() {
        guard let usage = currentTaskUsage() else { return }
        let sample: [String: Any] = [
            "elapsedSeconds": elapsedMs() / 1_000,
            "cpuSeconds": usage.cpuSeconds,
            "residentBytes": usage.residentBytes,
        ]
        resourceSamples.append(sample)
        relevantProcessSamples.append(relevantProcesses(excluding: baselineProcessIDs))
    }

    private func stopTimers() {
        frameTimer?.invalidate()
        resourceTimer?.invalidate()
        inputTimeoutTimer?.invalidate()
        phaseTimer?.invalidate()
        frameTimer = nil
        resourceTimer = nil
        inputTimeoutTimer = nil
        phaseTimer = nil
        if let displayLink {
            CVDisplayLinkStop(displayLink)
            self.displayLink = nil
        }
        if let readyObserver {
            NotificationCenter.default.removeObserver(readyObserver)
            self.readyObserver = nil
        }
        timeoutWorkItem?.cancel()
    }

    private func finish(status: String, error: String? = nil) {
        guard !finished else { return }
        finished = true
        flushPendingInputs()
        stopTimers()
        sampleResources()
        observeReadyForDisplay()

        let completed = status == "COMPLETED"
        let presentationStatus: String
        if firstReadyForDisplayMs != nil {
            presentationStatus = "PASS"
        } else if completed {
            presentationStatus = "FAIL"
        } else {
            presentationStatus = "NOT_TESTED"
        }

        let frameBoundary = summarizeInputs(inputSamples)
        let legacyPresentationRate: Double? = options.durationSeconds > 0
            ? Double(framesEnqueued) / options.durationSeconds
            : nil
        var report: [String: Any] = [
            "experiment": "native-video-presentation",
            "status": status,
            "durationSeconds": options.durationSeconds,
            "platform": [
                "os": ProcessInfo.processInfo.operatingSystemVersionString,
                "processArchitecture": architectureName(),
            ],
            "environmentRecord": readText(at: options.environmentURL),
            "workload": [
                "width": videoWidth,
                "height": videoHeight,
                "targetFrameRate": targetFrameRate,
                "pixelFormat": "32BGRA",
                "source": "deterministic moving uncompressed CVPixelBuffer",
                "presentationAPI": "AVSampleBufferDisplayLayer",
                "decode": "NOT_APPLICABLE: no compressed video is supplied",
            ],
            "presentation": [
                "setupReadyMs": setupReadyMs as Any,
                "firstEnqueueMs": firstEnqueueMs as Any,
                "firstReadyForDisplayMs": firstReadyForDisplayMs as Any,
                "presentationGate": presentationStatus,
                "readyForDisplay": rootView?.displayLayer.isReadyForDisplay ?? false,
                "rendererStatus": rendererStatusName(rootView?.displayLayer.status),
                "rendererError": rootView?.displayLayer.error?.localizedDescription as Any,
                "framesGenerated": framesGenerated,
                "framesEnqueued": framesEnqueued,
                "enqueueBackpressureCount": backpressureCount,
                "enqueueFailureCount": enqueueFailureCount,
                "enqueuedFramesPerSecond": legacyPresentationRate as Any,
                "perFramePresentationFreshness": "NOT_OBSERVABLE: AVSampleBufferDisplayLayer does not expose which sample buffers were displayed",
                "displayTickCount": displayTickCount,
                "displayTickIntervalsMs": summarizeIntervals(displayTickIntervalsMs),
            ],
            "input": [
                "metricVersion": 1,
                "ackTimeoutMs": inputAcknowledgementTimeoutMs,
                "scriptedCount": scriptedCount,
                "manualCount": manualCount,
                "manualInputGate": manualCount > 0 ? "OBSERVED" : "NOT_TESTED",
                "frameBoundaryGate": frameBoundary.gate,
                "scripted": frameBoundary.scripted,
                "manual": frameBoundary.manual,
                "samples": inputSamples.map(inputSampleDictionary),
                "definition": "Elapsed from native mouse/key or scripted probe handler to the next CVDisplayLink tick after the input target state is updated; a display-boundary proxy, not glass-to-glass latency.",
            ],
            "processSamples": resourceSamples,
            "relevantProcessSamples": relevantProcessSamples,
            "processBaselineCount": baselineProcessIDs.count,
            "resourceMeasurementLimits": "processSamples cover this native probe; relevantProcessSamples are best-effort baseline-delta ps matches and are not complete whole-application attribution",
        ]
        if let failureReason { report["failureReason"] = failureReason }
        if let error { report["error"] = error }

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

    private func summarizeInputs(_ samples: [InputSampleRecord]) -> (gate: String, scripted: [String: Any], manual: [String: Any]) {
        func summary(_ values: [InputSampleRecord]) -> [String: Any] {
            let acknowledged = values.filter { $0.acknowledged && $0.frameBoundaryMs != nil }
            let latencies = acknowledged.compactMap(\.frameBoundaryMs)
            return [
                "attemptedCount": values.count,
                "acknowledgedCount": acknowledged.count,
                "missingAcknowledgementCount": values.filter(\.missingAcknowledgement).count,
                "timeoutCount": values.filter(\.timedOut).count,
                "p50Ms": percentile(latencies, fraction: 0.50) as Any,
                "p95Ms": percentile(latencies, fraction: 0.95) as Any,
                "maxMs": latencies.max() as Any,
            ]
        }
        let scripted = samples.filter { !$0.trusted }
        let manual = samples.filter(\.trusted)
        let gate: String
        if samples.isEmpty {
            gate = "NOT_TESTED"
        } else if samples.contains(where: { $0.timedOut || $0.missingAcknowledgement }) {
            gate = "FAIL"
        } else {
            gate = "PASS"
        }
        return (gate, summary(scripted), summary(manual))
    }

    private func inputSampleDictionary(_ sample: InputSampleRecord) -> [String: Any] {
        [
            "sequence": sample.sequence,
            "kind": sample.kind,
            "trusted": sample.trusted,
            "acknowledged": sample.acknowledged,
            "missingAcknowledgement": sample.missingAcknowledgement,
            "timedOut": sample.timedOut,
            "frameBoundaryMs": sample.frameBoundaryMs as Any,
        ]
    }

    private func summarizeIntervals(_ values: [Double]) -> [String: Any] {
        guard !values.isEmpty else {
            return ["count": 0, "p50Ms": NSNull(), "p95Ms": NSNull(), "maxMs": NSNull()]
        }
        return [
            "count": values.count,
            "p50Ms": percentile(values, fraction: 0.50) as Any,
            "p95Ms": percentile(values, fraction: 0.95) as Any,
            "maxMs": values.max()!,
        ]
    }

    private func rendererStatusName(_ status: AVQueuedSampleBufferRenderingStatus?) -> String {
        guard let status else { return "NOT_TESTED" }
        switch status {
        case .unknown: return "unknown"
        case .rendering: return "rendering"
        case .failed: return "failed"
        @unknown default: return "unknown"
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
    do { try process.run() } catch { return [] }
    let data = output.fileHandleForReading.readDataToEndOfFile()
    process.waitUntilExit()
    guard let text = String(data: data, encoding: .utf8) else { return [] }
    return Set(text.split(whereSeparator: \.isNewline).compactMap {
        Int($0.trimmingCharacters(in: .whitespaces))
    })
}

private func relevantProcesses(excluding baselineProcessIDs: Set<Int>) -> [[String: Any]] {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/ps")
    process.arguments = ["-axo", "pid=,ppid=,%cpu=,rss=,comm=,args="]
    let output = Pipe()
    process.standardOutput = output
    process.standardError = FileHandle.nullDevice
    do { try process.run() } catch { return [] }
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
              let residentKilobytes = Int(fields[3]),
              !baselineProcessIDs.contains(pid) else { return nil }
        let command = String(fields[4])
        let arguments = fields.count == 6 ? String(fields[5]) : ""
        let searchable = "\(command) \(arguments)".lowercased()
        guard searchable.contains("gpu") || searchable.contains("windowserver") || searchable.contains("videotoolbox") || searchable.contains("avfoundation") else {
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

private func percentile(_ values: [Double], fraction: Double) -> Double? {
    guard !values.isEmpty else { return nil }
    let sorted = values.sorted()
    let index = min(sorted.count - 1, Int(Double(sorted.count - 1) * fraction))
    return sorted[index]
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

private func monotonicNow() -> UInt64 {
    DispatchTime.now().uptimeNanoseconds
}

@main
private struct Launcher {
    static func main() {
        do {
            let options = try Options.parse(arguments: CommandLine.arguments)
            let application = NSApplication.shared
            let delegate = NativeProbeDelegate(options: options)
            application.delegate = delegate
            application.run()
        } catch {
            fputs("ERROR: \(error)\n", stderr)
            exit(2)
        }
    }
}
