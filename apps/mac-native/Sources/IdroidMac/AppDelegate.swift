import AppKit
import CoreImage
import Foundation

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var controller: MacController?
    private var pairWindow: NSWindow?
    private var stateTimer: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        statusItem.button?.title = "iDroid: Searching"
        do {
            let controller = try MacController()
            self.controller = controller
            controller.stateChanged = { [weak self] state in
                DispatchQueue.main.async {
                    self?.statusItem.button?.title = "iDroid: \(state)"
                    self?.refreshPairWindow()
                }
            }
            controller.start()
            installMenu()
            stateTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) {
                [weak self] _ in self?.refreshPairWindow()
            }
        } catch {
            statusItem.button?.title = "iDroid: identity error"
            installMenu(error: error.localizedDescription)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        stateTimer?.invalidate()
        controller?.stop()
    }

    private func installMenu(error: String? = nil) {
        let menu = NSMenu()
        let pair = NSMenuItem(title: "Pair this phone…", action: #selector(showPairing), keyEquivalent: "p")
        pair.target = self
        menu.addItem(pair)
        let forget = NSMenuItem(title: "Forget phone", action: #selector(forgetPhone), keyEquivalent: "f")
        forget.target = self
        menu.addItem(forget)
        if let error {
            menu.addItem(.separator())
            let item = NSMenuItem(title: error, action: nil, keyEquivalent: "")
            item.isEnabled = false
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let quit = NSMenuItem(title: "Quit iDroid", action: #selector(quit), keyEquivalent: "q")
        quit.target = self
        menu.addItem(quit)
        statusItem.menu = menu
    }

    @objc private func showPairing() {
        controller?.beginPairing()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.presentPairWindow()
        }
    }

    @objc private func forgetPhone() {
        controller?.forgetPeer()
    }

    @objc private func quit() {
        NSApp.terminate(nil)
    }

    private func presentPairWindow() {
        guard let controller, let payload = controller.currentPairingPayload() else { return }
        if pairWindow == nil {
            pairWindow = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 460, height: 560),
                styleMask: [.titled, .closable],
                backing: .buffered,
                defer: false
            )
            pairWindow?.title = "Pair iDroid phone"
            pairWindow?.center()
        }
        pairWindow?.contentView = PairingView(payload: payload, controller: controller)
        pairWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func refreshPairWindow() {
        guard pairWindow?.isVisible == true,
              let payload = controller?.currentPairingPayload(),
              let controller else { return }
        pairWindow?.contentView = PairingView(payload: payload, controller: controller)
    }
}

@main
struct IdroidMain {
    static func main() {
        let application = NSApplication.shared
        let applicationDelegate = AppDelegate()
        application.delegate = applicationDelegate
        application.run()
    }
}

private final class PairingView: NSView {
    private let payload: PairingPayload
    private weak var controller: MacController?

    init(payload: PairingPayload, controller: MacController) {
        self.payload = payload
        self.controller = controller
        super.init(frame: NSRect(x: 0, y: 0, width: 460, height: 560))
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        fatalError("not supported")
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        NSColor.windowBackgroundColor.setFill()
        dirtyRect.fill()
        let title = "Scan this QR code on Android"
        (title as NSString).draw(
            at: NSPoint(x: 118, y: 510),
            withAttributes: [.font: NSFont.systemFont(ofSize: 18, weight: .semibold)]
        )
        if let image = qrImage(payload) {
            image.draw(in: NSRect(x: 70, y: 120, width: 320, height: 320))
        }
        let fingerprint = "Mac fingerprint: \(payload.macCertFingerprint.prefix(24))…"
        (fingerprint as NSString).draw(
            at: NSPoint(x: 68, y: 78),
            withAttributes: [.font: NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)]
        )
        let hint = "Listener: \(payload.serviceHint)   Expires: \(payload.expiresAt)"
        (hint as NSString).draw(
            at: NSPoint(x: 68, y: 52),
            withAttributes: [.font: NSFont.systemFont(ofSize: 11)]
        )
    }

    private func qrImage(_ payload: PairingPayload) -> NSImage? {
        guard let data = try? JSONEncoder().encode(payload),
              let filter = CIFilter(name: "CIQRCodeGenerator") else { return nil }
        filter.setValue(data, forKey: "inputMessage")
        filter.setValue("M", forKey: "inputCorrectionLevel")
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 12, y: 12))
        let context = CIContext()
        guard let cgImage = context.createCGImage(scaled, from: scaled.extent) else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: 320, height: 320))
    }
}
