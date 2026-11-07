import AppKit
import CoreImage
import CryptoKit
import Foundation
import Network
import Security
import Darwin

private let continuityServiceType = "_acontinuity._tcp."
private let continuityPort: UInt16 = 39231
private let maxPairingAttempts = 3
private let pairingLockoutSeconds: TimeInterval = 5
private let maxRejectedPairingTokens = 32

struct PairingPayload: Codable {
    let version: UInt16
    let macCertFingerprint: String
    let oneTimeToken: String
    let expiresAt: Int64
    let serviceHint: String
}

struct PairRequest: Codable {
    let type: String
    let token: String
    let phoneFingerprint: String
    let name: String
    let certificate: String
}

struct ReconnectRequest: Codable {
    let type: String
    let fingerprint: String
}

enum MacLinkError: LocalizedError {
    case invalidMessage
    case notPaired
    case tokenRejected
    case expired

    var errorDescription: String? {
        switch self {
        case .invalidMessage: return "peer message was invalid"
        case .notPaired: return "peer is not paired"
        case .tokenRejected: return "pairing token was rejected"
        case .expired: return "pairing token expired"
        }
    }
}

final class MacController: NSObject {
    let identity: MacIdentity
    private let trustStore = MacTrustStore()
    private let queue = DispatchQueue(label: "com.idroid.mac-native.control")
    private let logURL: URL
    private(set) var coreLink: CoreLink
    private let sessionID = UInt64.random(in: 1...UInt64.max)
    private var listener: NWListener?
    private var browser: NWBrowser?
    private var pathMonitor: NWPathMonitor?
    private var pairPayload: PairingPayload?
    private var rejectedPairingTokens: [String: (attempts: Int, retryAt: Date)] = [:]
    private var activeConnection: NWConnection?
    private var activePeerFingerprint: String?
    private var activeInitiator: String?
    var stateChanged: ((String) -> Void)?

    init(identity: MacIdentity? = nil) throws {
        self.identity = try identity ?? MacIdentity()
        self.coreLink = try CoreLink()
        let applicationSupport = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = applicationSupport.appendingPathComponent("iDroid", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        self.logURL = directory.appendingPathComponent("control-plane.ndjson")
        super.init()
    }

    var linkState: LinkStateText { coreLink.state }

    func start() {
        queue.async { [weak self] in
            self?.startListener()
            self?.startBrowser()
            self?.startPathMonitor()
        }
    }

    func beginPairing() {
        queue.async { [weak self] in
            guard let self else { return }
            let token = Data((0..<16).map { _ in UInt8.random(in: 0...255) })
            let payload = PairingPayload(
                version: 1,
                macCertFingerprint: self.identity.fingerprint,
                oneTimeToken: token.base64URLEncodedString,
                expiresAt: Int64(Date().timeIntervalSince1970 * 1_000) + 120_000,
                serviceHint: "\(Self.localIPv4Address()):\(continuityPort)"
            )
            self.pairPayload = payload
            self.record("PairingTokenIssued")
            self.stateChanged?("Pairing QR active until \(payload.expiresAt)")
        }
    }

    func currentPairingPayload() -> PairingPayload? {
        queue.sync { pairPayload }
    }

    func forgetPeer() {
        queue.async { [weak self] in
            guard let self else { return }
            try? self.trustStore.forget()
            self.activeConnection?.cancel()
            self.activeConnection = nil
            self.activePeerFingerprint = nil
            try? self.coreLink.networkChanged()
            self.record("PeerForgotten")
            self.stateChanged?("Searching")
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.listener?.cancel()
            self?.browser?.cancel()
            self?.pathMonitor?.cancel()
            self?.activeConnection?.cancel()
            self?.listener = nil
            self?.browser = nil
            self?.pathMonitor = nil
            self?.activeConnection = nil
        }
    }

    private func startListener() {
        guard listener == nil else { return }
        do {
            let parameters = NWParameters(tls: makeServerTLSOptions(), tcp: NWProtocolTCP.Options())
            parameters.includePeerToPeer = true
            let listener = try NWListener(using: parameters, on: NWEndpoint.Port(rawValue: continuityPort)!)
            listener.service = NWListener.Service(
                name: "iDroid Mac",
                type: continuityServiceType,
                domain: nil,
                txtRecord: NWTXTRecord(["v": "1", "role": "mac"])
            )
            listener.stateUpdateHandler = { [weak self] state in
                self?.queue.async {
                    switch state {
                    case .ready:
                        self?.record("ListenerReady")
                        self?.stateChanged?(self?.linkState.rawValue ?? "Searching")
                    case .failed:
                        self?.record("ListenerFailed")
                        self?.listener = nil
                    default: break
                    }
                }
            }
            listener.newConnectionHandler = { [weak self] connection in
                self?.acceptIncoming(connection)
            }
            self.listener = listener
            listener.start(queue: queue)
        } catch {
            record("ListenerFailed")
            stateChanged?("Listener failed")
        }
    }

    private func startBrowser() {
        guard browser == nil else { return }
        let browser = NWBrowser(for: .bonjour(type: continuityServiceType, domain: nil), using: .tcp)
        browser.stateUpdateHandler = { [weak self] state in
            if case .failed = state {
                self?.record("BonjourBrowseFailed")
            }
        }
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            guard let self else { return }
            for result in results {
                if case let .service(name, _, _, _) = result.endpoint, name == "iDroid Mac" {
                    continue
                }
                self.connectToDiscoveredPeer(result.endpoint)
            }
        }
        self.browser = browser
        browser.start(queue: queue)
    }

    private func startPathMonitor() {
        guard pathMonitor == nil else { return }
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] path in
            self?.queue.async {
                guard let self else { return }
                if path.status == .satisfied {
                    self.handleNetworkChanged()
                } else {
                    self.record("NetworkUnavailable")
                    self.handleNetworkChanged()
                }
            }
        }
        pathMonitor = monitor
        monitor.start(queue: queue)
    }

    private func handleNetworkChanged() {
        activeConnection?.cancel()
        activeConnection = nil
        activePeerFingerprint = nil
        activeInitiator = nil
        try? coreLink.networkChanged()
        record("NetworkChanged")
        stateChanged?(coreLink.state.rawValue)
    }

    private func acceptIncoming(_ connection: NWConnection) {
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            guard let self else { return }
            if case .ready = state {
                self.readMessage(on: connection, initiatedBy: nil)
            } else if case .failed = state {
                self.record("IncomingConnectionFailed")
                self.handleNetworkChanged()
            }
        }
        connection.start(queue: queue)
    }

    private func connectToDiscoveredPeer(_ endpoint: NWEndpoint) {
        guard trustStore.load() != nil else { return }
        let connection = NWConnection(to: endpoint, using: makeClientParameters())
        connection.stateUpdateHandler = { [weak self, weak connection] state in
            guard let self else { return }
            if case .ready = state {
                self.sendReconnectHello(on: connection)
            } else if case .failed = state {
                self.record("OutgoingConnectionFailed")
                self.handleNetworkChanged()
            }
        }
        connection.start(queue: queue)
    }

    private func sendReconnectHello(on connection: NWConnection?) {
        guard let connection, trustStore.load() != nil else { return }
        do {
            try coreLink.beginConnecting()
            record("ConnectStarted")
        } catch {
            connection.cancel()
            return
        }
        let message = try? JSONEncoder().encode(
            ReconnectRequest(type: "hello", fingerprint: identity.fingerprint)
        )
        send(message, on: connection)
        readMessage(on: connection, initiatedBy: identity.fingerprint)
    }

    private func readMessage(on connection: NWConnection?, initiatedBy: String?) {
        guard let connection else { return }
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) {
            [weak self, weak connection] data, _, isComplete, error in
            guard let self, let connection else { return }
            if let data, !data.isEmpty {
                self.handleMessage(data, on: connection, initiatedBy: initiatedBy)
            } else if isComplete || error != nil {
                self.record("ConnectionClosed")
                if self.activeConnection === connection {
                    self.handleNetworkChanged()
                }
            } else {
                self.readMessage(on: connection, initiatedBy: initiatedBy)
            }
        }
    }

    private func handleMessage(_ data: Data, on connection: NWConnection, initiatedBy: String?) {
        guard let line = String(data: data, encoding: .utf8),
              let first = line.split(separator: "\n", maxSplits: 1).first,
              let object = first.data(using: .utf8) else {
            send("ERROR invalid_message\n", on: connection)
            connection.cancel()
            return
        }
        if first == "OK ready" {
            handleOutgoingReady(on: connection)
            return
        }
        if let request = try? JSONDecoder().decode(PairRequest.self, from: object), request.type == "pair" {
            handlePair(request, on: connection)
        } else if let request = try? JSONDecoder().decode(ReconnectRequest.self, from: object), request.type == "hello" {
            handleReconnect(request, on: connection, initiatedBy: initiatedBy)
        } else {
            send("ERROR invalid_message\n", on: connection)
            connection.cancel()
        }
    }

    private func handlePair(_ request: PairRequest, on connection: NWConnection) {
        guard let payload = pairPayload else {
            send("ERROR listener_closed\n", on: connection)
            connection.cancel()
            return
        }
        let now = Int64(Date().timeIntervalSince1970 * 1_000)
        guard now < payload.expiresAt else {
            pairPayload = nil
            record("PairingTokenExpired")
            send("ERROR expired\n", on: connection)
            connection.cancel()
            return
        }
        guard request.token == payload.oneTimeToken else {
            let now = Date()
            if let rejected = rejectedPairingTokens[request.token], now < rejected.retryAt {
                record("PairingTokenLockedOut")
                send("ERROR locked_out\n", on: connection)
                connection.cancel()
                return
            }
            if rejectedPairingTokens[request.token] == nil,
               rejectedPairingTokens.count >= maxRejectedPairingTokens,
               let oldest = rejectedPairingTokens.keys.sorted().first {
                rejectedPairingTokens.removeValue(forKey: oldest)
            }
            var rejected = rejectedPairingTokens[request.token] ?? (attempts: 0, retryAt: now)
            rejected.attempts += 1
            if rejected.attempts >= maxPairingAttempts {
                rejected.retryAt = now.addingTimeInterval(pairingLockoutSeconds)
                rejectedPairingTokens[request.token] = rejected
                record("PairingTokenAttemptsExceeded")
                send("ERROR attempts_exceeded\n", on: connection)
            } else {
                rejectedPairingTokens[request.token] = rejected
                record("PairingTokenRejected")
                send("ERROR token_invalid\n", on: connection)
            }
            connection.cancel()
            return
        }
        guard let certificate = Data(base64Encoded: request.certificate),
              !request.phoneFingerprint.isEmpty else {
            send("ERROR invalid_peer\n", on: connection)
            connection.cancel()
            return
        }
        let peer = PairedMacPeer(
            fingerprint: request.phoneFingerprint,
            name: request.name,
            certificateBase64: certificate.base64EncodedString(),
            pairedAt: now
        )
        do {
            try trustStore.save(peer)
            rejectedPairingTokens.removeValue(forKey: request.token)
            pairPayload = nil
            record("PairingTokenConsumed")
            send("OK paired\n", on: connection)
            admit(connection, peerFingerprint: peer.fingerprint, initiator: peer.fingerprint)
            listener?.cancel()
            listener = nil
            startListener()
        } catch {
            send("ERROR store_failed\n", on: connection)
            connection.cancel()
        }
    }

    private func handleReconnect(
        _ request: ReconnectRequest,
        on connection: NWConnection,
        initiatedBy: String?
    ) {
        guard let peer = trustStore.load(), peer.fingerprint == request.fingerprint else {
            record("ConnectFailed:NotPaired")
            send("ERROR not_paired\n", on: connection)
            connection.cancel()
            return
        }
        guard admit(connection, peerFingerprint: peer.fingerprint, initiator: initiatedBy ?? peer.fingerprint) else {
            return
        }
        do {
            if coreLink.state == .ready {
                try coreLink.networkChanged()
            }
            try coreLink.beginConnecting()
            try coreLink.transportConnected()
            record("HandshakeOk")
            try coreLink.handshakeReady(sessionID: sessionID)
            send("OK ready\n", on: connection)
            stateChanged?("\(coreLink.state.rawValue) reconnect=\(coreLink.reconnectMilliseconds())ms")
            readMessage(on: connection, initiatedBy: initiatedBy)
        } catch {
            try? coreLink.connectionFailed()
            connection.cancel()
        }
    }

    private func handleOutgoingReady(on connection: NWConnection) {
        guard let peer = trustStore.load(),
              admit(connection, peerFingerprint: peer.fingerprint, initiator: identity.fingerprint) else {
            try? coreLink.connectionFailed()
            return
        }
        do {
            try coreLink.transportConnected()
            record("HandshakeOk")
            try coreLink.handshakeReady(sessionID: sessionID)
            stateChanged?("\(coreLink.state.rawValue) reconnect=\(coreLink.reconnectMilliseconds())ms")
        } catch {
            try? coreLink.connectionFailed()
            connection.cancel()
        }
    }

    @discardableResult
    private func admit(_ connection: NWConnection, peerFingerprint: String, initiator: String) -> Bool {
        if let active = activeConnection {
            if let currentInitiator = activeInitiator, initiator >= currentInitiator {
                record("ConnectionSuperseded")
                connection.cancel()
                return false
            }
            record("ConnectionSuperseded")
            active.cancel()
        }
        activeConnection = connection
        activePeerFingerprint = peerFingerprint
        activeInitiator = initiator
        return true
    }

    private func send(_ text: String, on connection: NWConnection) {
        send(Data(text.utf8), on: connection)
    }

    private func send(_ data: Data?, on connection: NWConnection?) {
        guard let data, let connection else { return }
        connection.send(content: data, completion: .contentProcessed { [weak self] error in
            if error != nil { self?.record("SendFailed") }
        })
    }

    private func makeServerTLSOptions() -> NWProtocolTLS.Options {
        let options = NWProtocolTLS.Options()
        let security = options.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(security, .TLSv13)
        sec_protocol_options_set_max_tls_protocol_version(security, .TLSv13)
        sec_protocol_options_set_peer_authentication_required(security, true)
        if let identity = sec_identity_create(identity.identity) {
            sec_protocol_options_set_local_identity(security, identity)
        }
        let pinned = trustStore.load().flatMap { Data(base64Encoded: $0.certificateBase64) }
        sec_protocol_options_set_verify_block(security, { _, trust, complete in
            complete(Self.evaluate(trust: trust, pinnedCertificate: pinned))
        }, queue)
        return options
    }

    private func makeClientParameters() -> NWParameters {
        let options = NWProtocolTLS.Options()
        let security = options.securityProtocolOptions
        sec_protocol_options_set_min_tls_protocol_version(security, .TLSv13)
        sec_protocol_options_set_max_tls_protocol_version(security, .TLSv13)
        if let identity = sec_identity_create(identity.identity) {
            sec_protocol_options_set_local_identity(security, identity)
        }
        let pinned = trustStore.load().flatMap { Data(base64Encoded: $0.certificateBase64) }
        sec_protocol_options_set_verify_block(security, { _, trust, complete in
            complete(Self.evaluate(trust: trust, pinnedCertificate: pinned))
        }, queue)
        return NWParameters(tls: options, tcp: NWProtocolTCP.Options())
    }

    private static func evaluate(trust: sec_trust_t, pinnedCertificate: Data?) -> Bool {
        guard let pinnedCertificate,
              let certificate = SecCertificateCreateWithData(nil, pinnedCertificate as CFData) else {
            // During first pairing the QR token authenticates the peer. The
            // listener is recreated with a pin immediately after pairing.
            return true
        }
        let secTrust = sec_trust_copy_ref(trust).takeRetainedValue()
        guard SecTrustSetPolicies(secTrust, SecPolicyCreateSSL(false, nil)) == errSecSuccess,
              SecTrustSetAnchorCertificates(secTrust, [certificate] as CFArray) == errSecSuccess,
              SecTrustSetAnchorCertificatesOnly(secTrust, true) == errSecSuccess else {
            return false
        }
        var error: CFError?
        return SecTrustEvaluateWithError(secTrust, &error)
    }

    private func record(_ event: String) {
        let line = "{\"at_ms\":\(CoreLink.monotonicMilliseconds()),\"event\":\"\(event)\"}\n"
        if let handle = try? FileHandle(forWritingTo: logURL) {
            handle.seekToEndOfFile()
            handle.write(Data(line.utf8))
            try? handle.close()
        } else {
            try? Data(line.utf8).write(to: logURL, options: .atomic)
        }
    }

    private static func localIPv4Address() -> String {
        var addresses: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&addresses) == 0, let first = addresses else { return "127.0.0.1" }
        defer { freeifaddrs(addresses) }
        var current: UnsafeMutablePointer<ifaddrs>? = first
        var fallback = "127.0.0.1"
        while let item = current {
            let interface = item.pointee
            if interface.ifa_addr.pointee.sa_family == UInt8(AF_INET) {
                var address = interface.ifa_addr.pointee
                var buffer = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
                let raw = withUnsafePointer(to: &address) {
                    $0.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { pointer in
                        var address = pointer.pointee.sin_addr
                        return inet_ntop(AF_INET, &address, &buffer, socklen_t(INET_ADDRSTRLEN))
                    }
                }
                if let raw {
                    let value = String(cString: raw)
                    if value != "127.0.0.1" {
                        if interface.ifa_name.flatMap({ String(cString: $0) }) == "en0" {
                            return value
                        }
                        fallback = value
                    }
                }
            }
            current = interface.ifa_next
        }
        return fallback
    }
}

private extension Data {
    var base64URLEncodedString: String {
        base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
