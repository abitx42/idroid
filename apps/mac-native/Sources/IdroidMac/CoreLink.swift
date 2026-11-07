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
