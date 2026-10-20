#pragma once

#include <cmath>
#include <cstddef>
#include <stdexcept>
#include <string>
#include <string_view>

// Classification only: counters are observations, not proof of a physical device,
// displayed-frame identity, negotiated codec, or glass-to-glass latency.
namespace receiver_diagnostics {

inline constexpr double kDefaultAcceptTimeoutSeconds = 10.0;
inline constexpr double kOfferReadTimeoutSeconds = 10.0;
inline constexpr double kMaxAcceptTimeoutSeconds = 3600.0;

inline double parseTimeoutSeconds(std::string_view value, std::string_view option) {
    const std::string text(value);
    std::size_t consumed = 0;
    double parsed = 0.0;
    try {
        parsed = std::stod(text, &consumed);
    } catch (const std::exception &) {
        throw std::runtime_error(std::string(option) + " must be in (0, 3600]");
    }
    if (consumed != text.size() || !std::isfinite(parsed) ||
        !(parsed > 0.0 && parsed <= kMaxAcceptTimeoutSeconds)) {
        throw std::runtime_error(std::string(option) + " must be in (0, 3600]");
    }
    return parsed;
}

struct Outcome {
    const char *status;
    const char *error;
};

inline Outcome completedTrial(int received, int decoded, int enqueued, int decodeFailures) {
    if (decoded == 0 && decodeFailures > 0) {
        return {"FAIL", "decoder failed before first frame"};
    }
    if (received == 0) return {"TIMEOUT", "first-frame timeout: no H264 frames received"};
    if (decoded == 0) return {"TIMEOUT", "first-frame timeout: H264 received but not decoded"};
    if (enqueued == 0) return {"TIMEOUT", "first-frame timeout: decoded but not enqueued"};
    return {"COMPLETED", ""};
}

struct Classification {
    const char *stage;
    const char *code;
    const char *error;
};

// Whitelist report text as well as classification. Unknown third-party error
// text must never become a persisted diagnostic, even if a caller passes it.
inline Classification classify(std::string_view status, std::string_view error) {
    struct Entry { const char *error, *stage, *code; };
    static constexpr Entry entries[] = {
        {"could not create signaling socket", "tcp", "socket_failed"},
        {"could not parse signaling bind address", "tcp", "bind_address_failed"},
        {"could not bind signaling port", "tcp", "bind_failed"},
        {"signaling listener poll failed", "tcp", "poll_failed"},
        {"signaling accept failed", "tcp", "accept_failed"},
        {"signaling connection timed out", "tcp", "accept_timeout"},
        {"peer disconnected before signaling offer", "tcp", "offer_disconnected"},
        {"could not send signaling answer", "tcp", "answer_send_failed"},
        {"signaling offer read timed out", "signaling", "offer_timeout"},
        {"signaling message exceeded bound", "signaling", "offer_oversized"},
        {"signaling offer ended before newline", "signaling", "offer_incomplete"},
        {"malformed signaling offer", "signaling", "offer_malformed"},
        {"signaling authentication failed", "signaling", "authentication_failed"},
        {"invalid signaling session ID", "signaling", "invalid_session"},
        {"invalid base64 signaling payload", "signaling", "invalid_base64"},
        {"signaling answer exceeded bound", "signaling", "answer_oversized"},
        {"SDP negotiation failed", "sdp", "remote_offer_failed"},
        {"answer was not generated", "sdp", "answer_timeout"},
        {"could not read local SDP answer", "sdp", "local_answer_failed"},
        {"ICE gathering did not complete", "ice_gathering", "gather_timeout"},
        {"ICE connection failed", "ice", "ice_failed"},
        {"ICE connection timed out before trial start", "ice", "connection_timeout"},
        {"ICE/DTLS connection failed", "peer", "peer_failed"},
        {"peer disconnected; remote cause not observable", "peer", "disconnected"},
        {"receiver track error", "media", "track_failed"},
        {"decoder failed before first frame", "decoder", "first_decode_failed"},
        {"first-frame timeout: no H264 frames received", "first_frame", "no_h264"},
        {"first-frame timeout: H264 received but not decoded", "first_frame", "no_decoded_frame"},
        {"first-frame timeout: decoded but not enqueued", "first_frame", "no_enqueued_frame"},
        {"receiver window closed", "user", "shutdown"},
        {"application terminated", "user", "shutdown"},
        {"receiver destroyed", "user", "shutdown"},
    };
    if (status == "COMPLETED" && error.empty()) return {"session", "completed", ""};
    for (const auto &entry : entries) {
        if (error == entry.error) return {entry.stage, entry.code, entry.error};
    }
    return {"internal", "unclassified", "receiver failed"};
}

} // namespace receiver_diagnostics
