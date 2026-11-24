import Foundation
import IdroidCoreFFI

enum LinkStateText: String {
    case searching = "Searching"
    case connecting = "Connecting"
    case handshaking = "Handshaking"
    case ready = "Ready"
    case degraded = "Degraded"
    case lost = "Lost"
}

final class CoreLink {
    private let handle: OpaquePointer

    init() throws {
        guard idroid_core_ffi_version() == 1,
              let handle = idroid_link_new(Self.monotonicMilliseconds()) else {
            throw CoreLinkError.createFailed
        }
        self.handle = handle
    }

    deinit {
        idroid_link_free(handle)
    }

    var state: LinkStateText {
        switch idroid_link_state(handle) {
        case IDROID_LINK_CONNECTING: return .connecting
        case IDROID_LINK_HANDSHAKING: return .handshaking
        case IDROID_LINK_READY: return .ready
        case IDROID_LINK_DEGRADED: return .degraded
        case IDROID_LINK_LOST: return .lost
        default: return .searching
        }
    }

    func beginConnecting() throws {
        try check(idroid_link_begin_connecting(handle, Self.monotonicMilliseconds()))
    }

    func transportConnected() throws {
        try check(idroid_link_transport_connected(handle, Self.monotonicMilliseconds()))
    }

    func connectionFailed() throws {
        try check(idroid_link_connection_failed(handle, Self.monotonicMilliseconds()))
    }

    func networkChanged() throws {
        try check(idroid_link_network_changed(handle, Self.monotonicMilliseconds()))
    }

    func handshakeReady(sessionID: UInt64?) throws {
        try check(idroid_link_handshake_ready(
            handle,
            Self.monotonicMilliseconds(),
            sessionID != nil,
            sessionID ?? 0
        ))
    }

    func reconnectMilliseconds() -> UInt64 {
        var value: UInt64 = 0
        _ = idroid_link_reconnect_time_ms(handle, Self.monotonicMilliseconds(), &value)
        return value
    }

    static func monotonicMilliseconds() -> UInt64 {
        var value = timespec()
        clock_gettime(CLOCK_MONOTONIC, &value)
        return UInt64(value.tv_sec) * 1_000 + UInt64(value.tv_nsec) / 1_000_000
    }

    private func check(_ result: IdroidLinkResult) throws {
        guard result == IDROID_LINK_OK else {
            throw CoreLinkError.result(result)
        }
    }
}

enum CoreLinkError: LocalizedError {
    case createFailed
    case result(IdroidLinkResult)

    var errorDescription: String? {
        switch self {
        case .createFailed: return "core link could not be created"
        case .result(let result): return "core link operation failed (\(result.rawValue))"
        }
    }
}

enum CoreCodec {
    static func encodeHello(
        minVersion: UInt16 = 2,
        maxVersion: UInt16 = 2,
        fingerprint: Data,
        role: UInt8 = 1, // 1 = mac, 2 = android
        appVersion: String = "1.0",
        lastSessionID: UInt64? = nil
    ) -> Data? {
        guard fingerprint.count == 32 else { return nil }
        var buffer = [UInt8](repeating: 0, count: 512)
        var length: Int = 0
        let status = fingerprint.withUnsafeBytes { fpPtr in
            appVersion.utf8CString.withUnsafeBytes { appVerPtr in
                idroid_codec_encode_hello(
                    minVersion,
                    maxVersion,
                    fpPtr.baseAddress?.assumingMemoryBound(to: UInt8.self),
                    role,
                    appVerPtr.baseAddress?.assumingMemoryBound(to: UInt8.self),
                    appVersion.utf8.count,
                    lastSessionID != nil,
                    lastSessionID ?? 0,
                    &buffer,
                    buffer.count,
                    &length
                )
            }
        }
        guard status == IDROID_CODEC_OK else { return nil }
        return Data(buffer.prefix(length))
    }

    static func decodeHello(frame: Data) -> (minVersion: UInt16, maxVersion: UInt16, fingerprintHex: String, role: UInt8, lastSessionID: UInt64?)? {
        var view = IdroidHelloView()
        let status = frame.withUnsafeBytes { framePtr in
            idroid_codec_decode_hello(
                framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self),
                frame.count,
                &view
            )
        }
        guard status == IDROID_CODEC_OK else { return nil }
        let fpBytes = withUnsafeBytes(of: view.fingerprint) { Data($0) }
        let fpHex = fpBytes.map { String(format: "%02x", $0) }.joined()
        let session = view.has_last_session != 0 ? view.last_session_id : nil
        return (view.version_min, view.version_max, fpHex, view.role, session)
    }

    static func encodeHelloAck(version: UInt16 = 2) -> Data? {
        var buffer = [UInt8](repeating: 0, count: 64)
        var length: Int = 0
        let status = idroid_codec_encode_hello_ack(version, &buffer, buffer.count, &length)
        guard status == IDROID_CODEC_OK else { return nil }
        return Data(buffer.prefix(length))
    }

    static func decodeHelloAck(frame: Data) -> UInt16? {
        var version: UInt16 = 0
        let status = frame.withUnsafeBytes { framePtr in
            idroid_codec_decode_hello_ack(
                framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self),
                frame.count,
                &version
            )
        }
        guard status == IDROID_CODEC_OK else { return nil }
        return version
    }

    static func encodeBye(code: UInt8? = nil, retryable: Bool = true, userAction: UInt8? = nil) -> Data? {
        var buffer = [UInt8](repeating: 0, count: 64)
        var length: Int = 0
        let status = idroid_codec_encode_bye(
            code != nil,
            code ?? 0,
            retryable,
            userAction != nil,
            userAction ?? 0,
            &buffer,
            buffer.count,
            &length
        )
        guard status == IDROID_CODEC_OK else { return nil }
        return Data(buffer.prefix(length))
    }

    static func decodeBye(frame: Data) -> (code: UInt8?, retryable: Bool, userAction: UInt8?)? {
        var view = IdroidByeView()
        let status = frame.withUnsafeBytes { framePtr in
            idroid_codec_decode_bye(
                framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self),
                frame.count,
                &view
            )
        }
        guard status == IDROID_CODEC_OK else { return nil }
        let code = view.has_code != 0 ? view.code : nil
        let retryable = view.retryable != 0
        let action = view.has_user_action != 0 ? view.user_action : nil
        return (code, retryable, action)
    }

    static func frameKind(frame: Data) -> Int32 {
        frame.withUnsafeBytes { framePtr in
            idroid_codec_frame_kind(framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self), frame.count)
        }
    }

    static func frameVersion(frame: Data) -> Int32 {
        frame.withUnsafeBytes { framePtr in
            idroid_codec_frame_version(framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self), frame.count)
        }
    }

    static func validate(frame: Data) -> Bool {
        let status = frame.withUnsafeBytes { framePtr in
            idroid_codec_validate(framePtr.baseAddress?.assumingMemoryBound(to: UInt8.self), frame.count)
        }
        return status == IDROID_CODEC_OK
    }
}
