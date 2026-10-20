#include "receiver_diagnostics.hpp"

#include <cstdio>
#include <cstring>
#include <stdexcept>
#include <string>

int main() {
    if (receiver_diagnostics::kDefaultAcceptTimeoutSeconds != 10.0 ||
        receiver_diagnostics::kOfferReadTimeoutSeconds != 10.0) {
        throw std::runtime_error("receiver timeout defaults changed");
    }
    if (receiver_diagnostics::parseTimeoutSeconds("10", "--accept-timeout-seconds") != 10.0 ||
        receiver_diagnostics::parseTimeoutSeconds("45", "--accept-timeout-seconds") != 45.0 ||
        receiver_diagnostics::parseTimeoutSeconds("0.25", "--accept-timeout-seconds") != 0.25) {
        throw std::runtime_error("receiver timeout parsing mismatch");
    }
    for (const char *value : {"", "0", "-1", "3600.1", "nan", "inf", "45junk"}) {
        bool rejected = false;
        try {
            (void)receiver_diagnostics::parseTimeoutSeconds(value, "--accept-timeout-seconds");
        } catch (const std::runtime_error &error) {
            rejected = true;
            if (std::string(error.what()).find("--accept-timeout-seconds") == std::string::npos) {
                throw std::runtime_error("invalid receiver timeout error was not attributed");
            }
        }
        if (!rejected) throw std::runtime_error("invalid receiver timeout was accepted");
    }
    std::puts("PASS timeout-diagnostics: default=10 configured=45 offer-read=10 invalid-values-rejected");

    using receiver_diagnostics::completedTrial;
    struct Case {
        int received, decoded, enqueued, failures;
        const char *status;
        const char *error;
    };
    const Case cases[] = {
        {0, 0, 0, 0, "TIMEOUT", "first-frame timeout: no H264 frames received"},
        {5, 0, 0, 0, "TIMEOUT", "first-frame timeout: H264 received but not decoded"},
        {5, 0, 0, 3, "FAIL", "decoder failed before first frame"},
        {5, 5, 0, 0, "TIMEOUT", "first-frame timeout: decoded but not enqueued"},
        {5, 5, 5, 0, "COMPLETED", ""},
        {5, 4, 4, 1, "COMPLETED", ""}, // Errors remain in counters; completion is not a gate pass.
    };
    for (const auto &test : cases) {
        const auto result = completedTrial(test.received, test.decoded, test.enqueued, test.failures);
        if (std::strcmp(result.status, test.status) || std::strcmp(result.error, test.error)) {
            throw std::runtime_error("receiver completion diagnostic mismatch");
        }
    }
    std::puts("PASS completion-diagnostics: 6 cases (classification only; no device)");

    struct ClassificationCase { const char *error, *stage, *code; };
    const ClassificationCase classifications[] = {
        {"signaling connection timed out", "tcp", "accept_timeout"},
        {"malformed signaling offer", "signaling", "offer_malformed"},
        {"signaling authentication failed", "signaling", "authentication_failed"},
        {"ICE gathering did not complete", "ice_gathering", "gather_timeout"},
        {"ICE connection failed", "ice", "ice_failed"},
        {"ICE/DTLS connection failed", "peer", "peer_failed"},
        {"SDP negotiation failed", "sdp", "remote_offer_failed"},
        {"decoder failed before first frame", "decoder", "first_decode_failed"},
        {"first-frame timeout: no H264 frames received", "first_frame", "no_h264"},
        {"receiver window closed", "user", "shutdown"},
    };
    for (const auto &test : classifications) {
        const auto result = receiver_diagnostics::classify("FAIL", test.error);
        if (std::strcmp(result.stage, test.stage) || std::strcmp(result.code, test.code) ||
            std::strcmp(result.error, test.error)) {
            throw std::runtime_error("receiver outcome classification mismatch");
        }
    }
    const auto untrusted = receiver_diagnostics::classify("FAIL", "untrusted SDP sentinel");
    if (std::strcmp(untrusted.stage, "internal") || std::strcmp(untrusted.code, "unclassified") ||
        std::strcmp(untrusted.error, "receiver failed")) {
        throw std::runtime_error("untrusted diagnostic was not sanitized");
    }
    const auto completed = receiver_diagnostics::classify("COMPLETED", "");
    if (std::strcmp(completed.code, "completed") || completed.error[0] != '\0') {
        throw std::runtime_error("completion diagnostic mismatch");
    }
    std::puts("PASS outcome-diagnostics: 12 cases (fixed text; no device)");
}
