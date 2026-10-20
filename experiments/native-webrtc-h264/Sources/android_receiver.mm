#import <AVFoundation/AVFoundation.h>
#import <AppKit/AppKit.h>
#import <CommonCrypto/CommonHMAC.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <QuartzCore/QuartzCore.h>
#import <VideoToolbox/VideoToolbox.h>

#include "rtc/rtc.hpp"
#include "receiver_diagnostics.hpp"

#include <arpa/inet.h>
#include <ifaddrs.h>
#include <mach/mach_time.h>
#include <netinet/in.h>
#include <poll.h>
#include <readpassphrase.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <termios.h>
#include <unistd.h>

#include <algorithm>
#include <array>
#include <atomic>
#include <cerrno>
#include <chrono>
#include <condition_variable>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>

namespace {

constexpr int kWidth = 640;
constexpr int kHeight = 360;
constexpr int kFrameRate = 30;
constexpr size_t kMaxSignalLine = 262'144;
constexpr size_t kMaxStdinPairingCode = 256;
constexpr uint8_t kPayloadType = 96;
constexpr char kLibDataChannelRevision[] = "6b1e2e620f1e37f0eafeee702eaea0043cb305fd";

std::string readPairingCodeFromStdin() {
    // On macOS RPP_STDIN skips echo control, even for a terminal. Use the
    // controlling terminal only after proving stdin is that terminal; otherwise
    // fail before reading. readpassphrase restores echo across interrupts.
    int flags = RPP_STDIN;
    if (isatty(STDIN_FILENO)) {
        if (tcgetsid(STDIN_FILENO) != getsid(0)) {
            throw std::runtime_error("--pairing-code-stdin requires a controlling terminal or a pipe");
        }
        flags = RPP_REQUIRE_TTY;
    }
    // The extra byte detects truncation; nonzero fill exposes embedded NULs.
    std::array<char, kMaxStdinPairingCode + 2> buffer;
    buffer.fill('\x7f');
    if (!readpassphrase("", buffer.data(), buffer.size(), flags | RPP_ECHO_OFF)) {
        throw std::runtime_error("could not read --pairing-code-stdin");
    }
    const auto terminator = std::find(buffer.rbegin(), buffer.rend(), '\0');
    if (terminator == buffer.rend()) throw std::runtime_error("invalid --pairing-code-stdin input");
    const size_t size = static_cast<size_t>(terminator.base() - buffer.begin() - 1);
    if (size < 8 || size > kMaxStdinPairingCode ||
        std::find(buffer.begin(), buffer.begin() + size, '\0') != buffer.begin() + size) {
        throw std::runtime_error("--pairing-code-stdin requires 8 to 256 bytes");
    }
    return std::string(buffer.data(), size);
}

std::optional<std::string> localInterfaceForAddress(const in_addr &address) {
    ifaddrs *interfaces = nullptr;
    if (getifaddrs(&interfaces) != 0) return std::nullopt;

    std::optional<std::string> result;
    for (ifaddrs *current = interfaces; current != nullptr; current = current->ifa_next) {
        if (!current->ifa_addr || current->ifa_addr->sa_family != AF_INET) continue;
        const auto *interfaceAddress = reinterpret_cast<const sockaddr_in *>(current->ifa_addr);
        if (interfaceAddress->sin_addr.s_addr == address.s_addr && current->ifa_name) {
            result = current->ifa_name;
            break;
        }
    }
    freeifaddrs(interfaces);
    return result;
}

struct Options {
    std::string output = "results/android-remote.json";
    std::string environmentPath;
    std::string bindAddress;
    std::string bindInterface;
    std::string pairingCode;
    int port = 8765;
    double durationSeconds = 30.0;
    double acceptTimeoutSeconds = receiver_diagnostics::kDefaultAcceptTimeoutSeconds;

    static Options parse(int argc, char **argv) {
        Options result;
        bool pairingCodeInArgv = false;
        bool pairingCodeInStdin = false;
        for (int i = 1; i < argc; ++i) {
            const std::string arg = argv[i];
            auto value = [&](const char *name) {
                if (++i >= argc) throw std::runtime_error(std::string("missing value for ") + name);
                return std::string(argv[i]);
            };
            if (arg == "--output") result.output = value("--output");
            else if (arg == "--environment") result.environmentPath = value("--environment");
            else if (arg == "--bind-address") result.bindAddress = value("--bind-address");
            else if (arg == "--pairing-code") {
                pairingCodeInArgv = true;
                result.pairingCode = value("--pairing-code");
            } else if (arg == "--pairing-code-stdin") {
                if (pairingCodeInStdin) throw std::runtime_error("--pairing-code-stdin may only be specified once");
                pairingCodeInStdin = true;
            }
            else if (arg == "--port") result.port = std::stoi(value("--port"));
            else if (arg == "--duration-seconds") result.durationSeconds = std::stod(value("--duration-seconds"));
            else if (arg == "--accept-timeout-seconds") {
                result.acceptTimeoutSeconds = receiver_diagnostics::parseTimeoutSeconds(
                    value("--accept-timeout-seconds"), "--accept-timeout-seconds");
            }
            else if (arg == "--help" || arg == "-h") {
                std::puts("Usage: android-webrtc-receiver --bind-address PRIVATE_IPV4 (--pairing-code-stdin | --pairing-code CODE) [--port N] [--output PATH] [--environment PATH] [--duration-seconds N] [--accept-timeout-seconds N]\n--pairing-code-stdin reads one line (8 to 256 bytes) without terminal echo.\n--accept-timeout-seconds controls the TCP accept deadline; default 10, range (0, 3600]. Offer reads remain limited to 10 seconds after accept.");
                std::exit(0);
            } else throw std::runtime_error("invalid argument");
        }
        if (pairingCodeInArgv && pairingCodeInStdin) throw std::runtime_error("choose only one pairing-code input option");
        if (result.bindAddress.empty()) throw std::runtime_error("--bind-address is required and must be an RFC1918 private IPv4 address");
        in_addr parsedAddress{};
        if (inet_pton(AF_INET, result.bindAddress.c_str(), &parsedAddress) != 1) {
            throw std::runtime_error("--bind-address must be an IPv4 address");
        }
        const uint32_t address = ntohl(parsedAddress.s_addr);
        const bool isTen = (address >> 24) == 10;
        const bool is172 = (address & 0xFFF00000u) == 0xAC100000u;
        const bool is192 = (address >> 16) == ((192 << 8) | 168);
        if (!isTen && !is172 && !is192) {
            throw std::runtime_error("--bind-address must be an RFC1918 private IPv4 address");
        }
        const auto interface = localInterfaceForAddress(parsedAddress);
        if (!interface) throw std::runtime_error("--bind-address is not assigned to a local interface");
        result.bindInterface = *interface;
        if (result.port < 1 || result.port > 65535) throw std::runtime_error("--port is out of range");
        if (!(result.durationSeconds > 0 && result.durationSeconds <= 3600)) throw std::runtime_error("invalid duration");
        if (pairingCodeInStdin) result.pairingCode = readPairingCodeFromStdin();
        if (result.pairingCode.size() < 8) throw std::runtime_error("--pairing-code requires at least 8 characters");
        return result;
    }
};

uint64_t nowNanos() {
    static mach_timebase_info_data_t timebase = [] { mach_timebase_info_data_t t{}; mach_timebase_info(&t); return t; }();
    const uint64_t raw = mach_absolute_time();
    return raw * timebase.numer / timebase.denom;
}

double elapsedMs(uint64_t start, uint64_t now) {
    return now < start ? 0 : static_cast<double>(now - start) / 1'000'000.0;
}

std::string quote(const std::string &value) {
    std::ostringstream out;
    out << '"';
    for (unsigned char c : value) {
        if (c == '"') out << "\\\"";
        else if (c == '\\') out << "\\\\";
        else if (c == '\n') out << "\\n";
        else if (c == '\r') out << "\\r";
        else if (c == '\t') out << "\\t";
        else out << c;
    }
    out << '"';
    return out.str();
}

std::string object(const std::vector<std::pair<std::string, std::string>> &fields) {
    std::ostringstream out;
    out << '{';
    for (size_t i = 0; i < fields.size(); ++i) {
        if (i) out << ',';
        out << quote(fields[i].first) << ':' << fields[i].second;
    }
    out << '}';
    return out.str();
}

std::string base64UrlEncode(const std::string &value) {
    static const char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    std::string output;
    uint32_t buffer = 0;
    int bits = 0;
    for (unsigned char c : value) {
        buffer = (buffer << 8) | c;
        bits += 8;
        while (bits >= 6) { bits -= 6; output.push_back(alphabet[(buffer >> bits) & 0x3F]); }
    }
    if (bits) output.push_back(alphabet[(buffer << (6 - bits)) & 0x3F]);
    return output;
}

std::string base64UrlDecode(const std::string &value) {
    auto digit = [](char c) -> int {
        if (c >= 'A' && c <= 'Z') return c - 'A';
        if (c >= 'a' && c <= 'z') return c - 'a' + 26;
        if (c >= '0' && c <= '9') return c - '0' + 52;
        if (c == '-') return 62;
        if (c == '_') return 63;
        return -1;
    };
    std::string output;
    uint32_t buffer = 0;
    int bits = 0;
    for (char c : value) {
        const int d = digit(c);
        if (d < 0) throw std::runtime_error("invalid base64 signaling payload");
        buffer = (buffer << 6) | static_cast<uint32_t>(d);
        bits += 6;
        if (bits >= 8) { bits -= 8; output.push_back(static_cast<char>((buffer >> bits) & 0xFF)); }
    }
    return output;
}

std::string hmacHex(const std::string &secret, const std::string &value) {
    unsigned char digest[CC_SHA256_DIGEST_LENGTH]{};
    CCHmac(kCCHmacAlgSHA256, secret.data(), secret.size(), value.data(), value.size(), digest);
    std::ostringstream out;
    out << std::hex << std::setfill('0');
    for (unsigned char c : digest) out << std::setw(2) << static_cast<int>(c);
    return out.str();
}

bool constantTimeEqual(const std::string &left, const std::string &right) {
    if (left.size() != right.size()) return false;
    unsigned char difference = 0;
    for (size_t index = 0; index < left.size(); ++index) {
        difference |= static_cast<unsigned char>(left[index]) ^ static_cast<unsigned char>(right[index]);
    }
    return difference == 0;
}

bool validSessionId(const std::string &sessionId) {
    if (sessionId.empty() || sessionId.size() > 128) return false;
    for (unsigned char character : sessionId) {
        if (character < 0x21 || character > 0x7E || character == '|') return false;
    }
    return true;
}

bool sendAll(int descriptor, const char *data, size_t size) {
    size_t sent = 0;
    while (sent < size) {
        const ssize_t count = send(descriptor, data + sent, size - sent, 0);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) return false;
        sent += static_cast<size_t>(count);
    }
    return true;
}

} // namespace

@interface AndroidReceiverView : NSView
@property(nonatomic, strong) AVSampleBufferDisplayLayer *displayLayer;
@end

@implementation AndroidReceiverView
- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer = [CALayer layer];
        self.layer.backgroundColor = NSColor.blackColor.CGColor;
        self.displayLayer = [AVSampleBufferDisplayLayer layer];
        self.displayLayer.videoGravity = AVLayerVideoGravityResizeAspect;
        [self.layer addSublayer:self.displayLayer];
    }
    return self;
}
- (void)layout { [super layout]; self.displayLayer.frame = self.bounds; }
@end

namespace {

class AndroidReceiver;

static void decodeCallback(void *, void *, OSStatus, VTDecodeInfoFlags, CVImageBufferRef, CMTime, CMTime);

class AndroidReceiver {
public:
    explicit AndroidReceiver(Options options) : options_(std::move(options)) {}
    ~AndroidReceiver();
    void start();
    void abort(const std::string &reason);
    void handleFrame(rtc::binary frame, rtc::FrameInfo info);
    void handleDecoded(OSStatus status, CVImageBufferRef image, CMTime pts);
    void handleAnswer(rtc::Description description);
    NSWindow *window() const { return window_; }

private:
    void setupWindow();
    void setupPeer();
    void signalLoop();
    void finish(const std::string &status, const std::string &error = {});
    void writeReport(const std::string &status, const std::string &error);
    double preConnectionTimeoutSeconds() const {
        return std::max(options_.durationSeconds + 15.0, options_.acceptTimeoutSeconds + 1.0);
    }
    bool setupDecoder(const std::vector<uint8_t> &sps, const std::vector<uint8_t> &pps);
    static std::vector<std::vector<uint8_t>> splitAnnexB(const uint8_t *, size_t);
    static std::string readText(const std::string &);

    Options options_;
    uint64_t startNanos_ = 0;
    uint64_t connectionNanos_ = 0;
    std::atomic<uint64_t> firstReceivedNanos_{0};
    std::atomic<uint64_t> firstDecodedNanos_{0};
    uint64_t firstReadyNanos_ = 0;
    std::atomic<bool> finished_{false};
    std::atomic<int> receivedFrames_{0};
    std::atomic<int> decodedFrames_{0};
    std::atomic<int> displayFrames_{0};
    std::atomic<int> decodeFailures_{0};
    std::atomic<int> decoderSetupFailures_{0};
    std::atomic<int> decodeDrops_{0};
    std::atomic<bool> tcpAccepted_{false};
    std::atomic<bool> authenticated_{false};
    std::atomic<bool> remoteDescriptionAccepted_{false};
    std::atomic<bool> answerSent_{false};
    std::atomic<int> signalSocket_{-1};
    std::thread signalThread_;
    std::mutex stateMutex_;
    std::condition_variable answerReady_;
    std::condition_variable gatheringReady_;
    std::string answerSdp_;
    std::string sessionId_;
    std::string senderState_ = "new";
    bool gatheringComplete_ = false;
    std::string error_;
    std::mutex decoderMutex_;
    std::vector<uint8_t> sps_, pps_;
    CMVideoFormatDescriptionRef decoderFormat_ = nullptr;
    VTDecompressionSessionRef decoder_ = nullptr;
    NSWindow *window_ = nil;
    AndroidReceiverView *view_ = nil;
    std::shared_ptr<rtc::PeerConnection> peer_;
    std::shared_ptr<rtc::Track> track_;
};

} // namespace

namespace {

static void decodeCallback(void *ref,
                           void *,
                           OSStatus status,
                           VTDecodeInfoFlags,
                           CVImageBufferRef image,
                           CMTime pts,
                           CMTime duration) {
    (void)duration;
    auto *receiver = static_cast<AndroidReceiver *>(ref);
    if (receiver) receiver->handleDecoded(status, image, pts);
}

AndroidReceiver::~AndroidReceiver() {
    abort("receiver destroyed");
    if (signalThread_.joinable()) signalThread_.join();
}

void AndroidReceiver::setupWindow() {
    view_ = [[AndroidReceiverView alloc] initWithFrame:NSMakeRect(0, 0, 960, 640)];
    window_ = [[NSWindow alloc] initWithContentRect:view_.frame
                                          styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                                                     NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable)
                                            backing:NSBackingStoreBuffered
                                              defer:NO];
    window_.title = @"iDroid Android-to-Mac receiver (experimental)";
    window_.contentView = view_;
    [window_ center];
    [window_ makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
}

void AndroidReceiver::setupPeer() {
    // Library warnings may embed remote SDP/credentials. Emit fixed receiver
    // diagnostics instead of forwarding untrusted parser or callback text.
    rtc::InitLogger(rtc::LogLevel::None);
    rtc::Configuration configuration;
    configuration.iceServers.clear();
    configuration.bindAddress = options_.bindAddress;
    peer_ = std::make_shared<rtc::PeerConnection>(configuration);
    peer_->onStateChange([this](rtc::PeerConnection::State state) {
        if (finished_.load()) return;
        {
            std::lock_guard<std::mutex> lock(stateMutex_);
            switch (state) {
            case rtc::PeerConnection::State::New: senderState_ = "new"; break;
            case rtc::PeerConnection::State::Connecting: senderState_ = "connecting"; break;
            case rtc::PeerConnection::State::Connected: senderState_ = "connected"; break;
            case rtc::PeerConnection::State::Disconnected: senderState_ = "disconnected"; break;
            case rtc::PeerConnection::State::Failed: senderState_ = "failed"; break;
            case rtc::PeerConnection::State::Closed: senderState_ = "closed"; break;
            }
        }
        if (state == rtc::PeerConnection::State::Connected) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!this->finished_.load() && this->connectionNanos_ == 0) {
                    this->connectionNanos_ = nowNanos();
                    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                                 static_cast<int64_t>(this->options_.durationSeconds * NSEC_PER_SEC)),
                                   dispatch_get_main_queue(), ^{
                        if (!this->finished_.load()) {
                            const auto outcome = receiver_diagnostics::completedTrial(
                                this->receivedFrames_.load(), this->decodedFrames_.load(),
                                this->displayFrames_.load(), this->decodeFailures_.load());
                            this->finish(outcome.status, outcome.error);
                        }
                    });
                }
            });
        }
        if (state == rtc::PeerConnection::State::Failed) {
            dispatch_async(dispatch_get_main_queue(), ^{
                this->finish("FAIL", this->peer_->iceState() == rtc::PeerConnection::IceState::Failed
                                         ? "ICE connection failed" : "ICE/DTLS connection failed");
            });
        }
        if (state == rtc::PeerConnection::State::Disconnected || state == rtc::PeerConnection::State::Closed) {
            dispatch_async(dispatch_get_main_queue(), ^{
                if (!this->finished_.load()) this->finish("DISCONNECTED", "peer disconnected; remote cause not observable");
            });
        }
    });
    peer_->onIceStateChange([this](rtc::PeerConnection::IceState state) {
        if (state == rtc::PeerConnection::IceState::Failed && !finished_.load()) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "ICE connection failed"); });
        }
    });
    peer_->onLocalDescription([this](rtc::Description description) {
        this->handleAnswer(std::move(description));
    });
    peer_->onGatheringStateChange([this](rtc::PeerConnection::GatheringState state) {
        if (state == rtc::PeerConnection::GatheringState::Complete) {
            std::lock_guard<std::mutex> lock(stateMutex_);
            gatheringComplete_ = true;
            gatheringReady_.notify_all();
        }
    });

    rtc::Description::Video media("video", rtc::Description::Direction::RecvOnly);
    media.addH264Codec(kPayloadType);
    media.setBitrate(3000);
    track_ = peer_->addTrack(media);
    track_->setMediaHandler(std::make_shared<rtc::H264RtpDepacketizer>());
    track_->chainMediaHandler(std::make_shared<rtc::RtcpReceivingSession>());
    track_->onFrame([this](rtc::binary frame, rtc::FrameInfo info) {
        this->handleFrame(std::move(frame), info);
    });
    track_->onError([this](std::string) {
        dispatch_async(dispatch_get_main_queue(), ^{
            this->finish("FAIL", "receiver track error");
        });
    });
}

void AndroidReceiver::start() {
    startNanos_ = nowNanos();
    setupWindow();
    setupPeer();
    signalThread_ = std::thread([this] { signalLoop(); });
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                 static_cast<int64_t>(preConnectionTimeoutSeconds() * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
        // A late connection still gets the full requested trial duration.
        if (!this->finished_.load() && this->connectionNanos_ == 0) {
            const char *error = "ICE connection timed out before trial start";
            {
                std::lock_guard<std::mutex> lock(this->stateMutex_);
                if (!this->tcpAccepted_.load()) error = "signaling connection timed out";
                else if (!this->authenticated_.load()) error = "signaling offer read timed out";
                else if (!this->remoteDescriptionAccepted_.load()) error = "SDP negotiation failed";
                else if (this->answerSdp_.empty()) error = "answer was not generated";
                else if (!this->gatheringComplete_) error = "ICE gathering did not complete";
                else if (!this->answerSent_.load()) error = "could not send signaling answer";
            }
            this->finish("TIMEOUT", error);
        }
    });
}

void AndroidReceiver::abort(const std::string &reason) {
    if ([NSThread isMainThread]) finish(reason == "receiver window closed" ? "USER_STOPPED" : "ABORTED", reason);
    else {
        const std::string copy = reason;
        dispatch_async(dispatch_get_main_queue(), ^{
            this->finish("ABORTED", copy);
        });
    }
}

void AndroidReceiver::handleAnswer(rtc::Description description) {
    {
        std::lock_guard<std::mutex> lock(stateMutex_);
        answerSdp_ = std::string(description);
    }
    answerReady_.notify_all();
}

void AndroidReceiver::signalLoop() {
    auto releaseSocket = [this](int descriptor) {
        int expected = descriptor;
        if (signalSocket_.compare_exchange_strong(expected, -1)) {
            shutdown(descriptor, SHUT_RDWR);
            close(descriptor);
        }
    };
    const int server = socket(AF_INET, SOCK_STREAM, 0);
    if (server < 0) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "could not create signaling socket"); });
        return;
    }
    int reuse = 1;
    setsockopt(server, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse));
    sockaddr_in address{};
    address.sin_family = AF_INET;
    if (inet_pton(AF_INET, options_.bindAddress.c_str(), &address.sin_addr) != 1) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "could not parse signaling bind address"); });
        close(server);
        return;
    }
    address.sin_port = htons(static_cast<uint16_t>(options_.port));
    if (bind(server, reinterpret_cast<sockaddr *>(&address), sizeof(address)) < 0 || listen(server, 1) < 0) {
        close(server);
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "could not bind signaling port"); });
        return;
    }
    signalSocket_.store(server);
    int client = -1;
    const uint64_t acceptDeadline = nowNanos() + static_cast<uint64_t>(
        options_.acceptTimeoutSeconds * 1'000'000'000.0);
    while (!finished_.load()) {
        if (nowNanos() >= acceptDeadline) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("TIMEOUT", "signaling connection timed out"); });
            int expected = server;
            if (signalSocket_.compare_exchange_strong(expected, -1)) {
                shutdown(server, SHUT_RDWR);
                close(server);
            }
            return;
        }
        pollfd descriptor{server, POLLIN, 0};
        const int ready = poll(&descriptor, 1, 250);
        if (ready < 0 && errno == EINTR) continue;
        if (ready < 0) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "signaling listener poll failed"); });
            releaseSocket(server);
            return;
        }
        if (ready == 0) continue;
        client = accept(server, nullptr, nullptr);
        if (client < 0 && errno == EINTR) continue;
        if (client < 0) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "signaling accept failed"); });
            releaseSocket(server);
            return;
        }
        break;
    }
    int expectedServer = server;
    signalSocket_.compare_exchange_strong(expectedServer, -1);
    shutdown(server, SHUT_RDWR);
    close(server);
    if (client < 0 || finished_.load()) {
        if (client >= 0) close(client);
        return;
    }
    int expected = -1;
    if (!signalSocket_.compare_exchange_strong(expected, client)) {
        close(client);
        return;
    }
    tcpAccepted_.store(true);
    int noSigPipe = 1;
    setsockopt(client, SOL_SOCKET, SO_NOSIGPIPE, &noSigPipe, sizeof(noSigPipe));
    timeval sendTimeout{10, 0};
    setsockopt(client, SOL_SOCKET, SO_SNDTIMEO, &sendTimeout, sizeof(sendTimeout));

    std::string line;
    char buffer[4096];
    bool peerClosed = false;
    const uint64_t offerDeadline = nowNanos() + static_cast<uint64_t>(
        receiver_diagnostics::kOfferReadTimeoutSeconds * 1'000'000'000.0);
    while (line.size() <= kMaxSignalLine && !finished_.load()) {
        if (nowNanos() >= offerDeadline) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("TIMEOUT", "signaling offer read timed out"); });
            releaseSocket(client);
            return;
        }
        pollfd descriptor{client, POLLIN, 0};
        const int ready = poll(&descriptor, 1, 250);
        if (ready < 0 && errno == EINTR) continue;
        if (ready < 0 || (ready > 0 && (descriptor.revents & (POLLERR | POLLNVAL)))) {
            peerClosed = true;
            break;
        }
        if (ready > 0 && (descriptor.revents & POLLHUP) && !(descriptor.revents & POLLIN)) {
            peerClosed = true;
            break;
        }
        if (ready == 0) continue;
        const ssize_t count = recv(client, buffer, sizeof(buffer), 0);
        if (count <= 0) {
            peerClosed = true;
            break;
        }
        line.append(buffer, static_cast<size_t>(count));
        if (line.find('\n') != std::string::npos) break;
    }
    if (finished_.load()) { releaseSocket(client); return; }
    if (line.size() > kMaxSignalLine) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "signaling message exceeded bound"); });
        releaseSocket(client);
        return;
    }
    if (peerClosed && line.find('\n') == std::string::npos) {
        const char *error = line.empty()
                                ? "peer disconnected before signaling offer"
                                : "signaling offer ended before newline";
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", error); });
        releaseSocket(client);
        return;
    }
    const size_t newline = line.find('\n');
    if (newline != std::string::npos) line.resize(newline);
    if (!line.empty() && line.back() == '\r') line.pop_back();
    std::vector<std::string> fields;
    size_t start = 0;
    while (fields.size() < 4) {
        const size_t tab = line.find('\t', start);
        if (tab == std::string::npos) { fields.push_back(line.substr(start)); break; }
        fields.push_back(line.substr(start, tab - start));
        start = tab + 1;
    }
    if (fields.size() != 4 || fields[0] != "OFFER") {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "malformed signaling offer"); });
        releaseSocket(client);
        return;
    }
    if (!constantTimeEqual(fields[3], hmacHex(options_.pairingCode, "OFFER|" + fields[1] + "|" + fields[2]))) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "signaling authentication failed"); });
        releaseSocket(client);
        return;
    }
    authenticated_.store(true);
    if (!validSessionId(fields[1])) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "invalid signaling session ID"); });
        releaseSocket(client);
        return;
    }
    std::string offerSdp;
    try {
        offerSdp = base64UrlDecode(fields[2]);
    } catch (const std::exception &) {
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "invalid base64 signaling payload"); });
        releaseSocket(client);
        return;
    }
    {
        std::lock_guard<std::mutex> lock(stateMutex_);
        sessionId_ = fields[1];
    }
    try {
        peer_->setRemoteDescription(rtc::Description(offerSdp, "offer"));
        remoteDescriptionAccepted_.store(true);
    } catch (const std::exception &) {
        // Third-party parser exceptions can include untrusted SDP/credentials.
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "SDP negotiation failed"); });
        releaseSocket(client);
        return;
    }

    std::unique_lock<std::mutex> lock(stateMutex_);
    if (!answerReady_.wait_for(lock, std::chrono::seconds(10), [this] { return !answerSdp_.empty() || finished_.load(); })) {
        lock.unlock();
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("TIMEOUT", "answer was not generated"); });
        releaseSocket(client);
        return;
    }
    if (finished_.load()) { lock.unlock(); releaseSocket(client); return; }
    if (!gatheringReady_.wait_for(lock, std::chrono::seconds(10), [this] { return gatheringComplete_ || finished_.load(); })) {
        lock.unlock();
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("TIMEOUT", "ICE gathering did not complete"); });
        releaseSocket(client);
        return;
    }
    if (finished_.load()) { lock.unlock(); releaseSocket(client); return; }
    std::string answer;
    try {
        const auto local = peer_->localDescription();
        answer = local ? std::string(*local) : answerSdp_;
    } catch (...) {
        lock.unlock();
        dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "could not read local SDP answer"); });
        releaseSocket(client);
        return;
    }
    lock.unlock();
    if (!answer.empty() && !finished_.load()) {
        const std::string payload = base64UrlEncode(answer);
        const std::string auth = hmacHex(options_.pairingCode, "ANSWER|" + sessionId_ + "|" + payload);
        const std::string response = "ANSWER\t" + sessionId_ + "\t" + payload + "\t" + auth + "\n";
        if (response.size() > kMaxSignalLine) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "signaling answer exceeded bound"); });
            releaseSocket(client);
            return;
        }
        if (!sendAll(client, response.data(), response.size())) {
            dispatch_async(dispatch_get_main_queue(), ^{ this->finish("FAIL", "could not send signaling answer"); });
        } else {
            answerSent_.store(true);
        }
    }
    releaseSocket(client);
}

std::vector<std::vector<uint8_t>> AndroidReceiver::splitAnnexB(const uint8_t *data, size_t size) {
    std::vector<std::vector<uint8_t>> result;
    if (!data || !size) return result;
    auto prefix = [&](size_t i) -> size_t {
        if (i + 3 <= size && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1) return 3;
        if (i + 4 <= size && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 && data[i + 3] == 1) return 4;
        return 0;
    };
    size_t first = 0;
    while (first < size && prefix(first) == 0) ++first;
    if (first == size) { result.emplace_back(data, data + size); return result; }
    while (first < size) {
        const size_t length = prefix(first);
        const size_t begin = first + length;
        size_t next = begin;
        while (next < size && prefix(next) == 0) ++next;
        if (begin < next) result.emplace_back(data + begin, data + next);
        first = next;
    }
    return result;
}

bool AndroidReceiver::setupDecoder(const std::vector<uint8_t> &sps, const std::vector<uint8_t> &pps) {
    if (sps.empty() || pps.empty()) return false;
    std::lock_guard<std::mutex> lock(decoderMutex_);
    if (decoder_ && sps == sps_ && pps == pps_) return true;
    const uint8_t *sets[] = {sps.data(), pps.data()};
    const size_t sizes[] = {sps.size(), pps.size()};
    CMVideoFormatDescriptionRef format = nullptr;
    if (CMVideoFormatDescriptionCreateFromH264ParameterSets(kCFAllocatorDefault, 2, sets, sizes, 4, &format) != noErr || !format) {
        if (format) CFRelease(format);
        return false;
    }
    NSDictionary *attrs = @{
        (id)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
        (id)kCVPixelBufferMetalCompatibilityKey: @YES,
    };
    VTDecompressionOutputCallbackRecord callback = {decodeCallback, this};
    VTDecompressionSessionRef session = nullptr;
    const OSStatus status = VTDecompressionSessionCreate(kCFAllocatorDefault, format, nullptr,
                                                          (__bridge CFDictionaryRef)attrs, &callback, &session);
    if (status != noErr || !session) { CFRelease(format); return false; }
    if (decoder_) { VTDecompressionSessionInvalidate(decoder_); CFRelease(decoder_); }
    if (decoderFormat_) CFRelease(decoderFormat_);
    decoder_ = session;
    decoderFormat_ = format;
    sps_ = sps;
    pps_ = pps;
    return true;
}

void AndroidReceiver::handleFrame(rtc::binary frame, rtc::FrameInfo info) {
    if (finished_.load()) return;
    uint64_t unset = 0;
    firstReceivedNanos_.compare_exchange_strong(unset, nowNanos());
    receivedFrames_.fetch_add(1);
    std::vector<uint8_t> bytes;
    bytes.reserve(frame.size());
    for (const std::byte byte : frame) bytes.push_back(std::to_integer<uint8_t>(byte));
    const auto nals = splitAnnexB(bytes.data(), bytes.size());
    std::vector<uint8_t> sps, pps, avcc;
    for (const auto &nal : nals) {
        if (nal.empty()) continue;
        const uint8_t type = nal[0] & 0x1F;
        if (type == 7) sps = nal;
        else if (type == 8) pps = nal;
    }
    if (!sps.empty() && !pps.empty() && !setupDecoder(sps, pps)) {
        decoderSetupFailures_.fetch_add(1);
        decodeFailures_.fetch_add(1);
    }
    for (const auto &nal : nals) {
        if (nal.empty() || (nal[0] & 0x1F) == 7 || (nal[0] & 0x1F) == 8) continue;
        const uint32_t length = static_cast<uint32_t>(nal.size());
        avcc.push_back(static_cast<uint8_t>(length >> 24));
        avcc.push_back(static_cast<uint8_t>(length >> 16));
        avcc.push_back(static_cast<uint8_t>(length >> 8));
        avcc.push_back(static_cast<uint8_t>(length));
        avcc.insert(avcc.end(), nal.begin(), nal.end());
    }
    if (avcc.empty()) { decodeDrops_.fetch_add(1); return; }
    std::lock_guard<std::mutex> lock(decoderMutex_);
    if (!decoder_ || !decoderFormat_) { decodeDrops_.fetch_add(1); return; }
    CMBlockBufferRef block = nullptr;
    OSStatus status = CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault, nullptr, avcc.size(),
                                                          kCFAllocatorDefault, nullptr, 0, avcc.size(), 0, &block);
    if (status == noErr) status = CMBlockBufferReplaceDataBytes(avcc.data(), block, 0, avcc.size());
    if (status != noErr || !block) { if (block) CFRelease(block); decodeFailures_.fetch_add(1); return; }
    const double seconds = info.timestampSeconds ? info.timestampSeconds->count()
                                                 : (receivedFrames_.load() - 1) / static_cast<double>(kFrameRate);
    CMSampleTimingInfo timing = {CMTimeMake(1, kFrameRate), CMTimeMakeWithSeconds(seconds, 90'000), kCMTimeInvalid};
    CMSampleBufferRef sample = nullptr;
    status = CMSampleBufferCreateReady(kCFAllocatorDefault, block, decoderFormat_, 1, 0, &timing, 0, nullptr, &sample);
    CFRelease(block);
    if (status != noErr || !sample) { decodeFailures_.fetch_add(1); return; }
    VTDecodeInfoFlags flags = 0;
    status = VTDecompressionSessionDecodeFrame(decoder_, sample, kVTDecodeFrame_EnableAsynchronousDecompression, nullptr, &flags);
    CFRelease(sample);
    if (status != noErr) decodeFailures_.fetch_add(1);
}

void AndroidReceiver::handleDecoded(OSStatus status, CVImageBufferRef image, CMTime pts) {
    if (finished_.load()) return;
    if (status != noErr || !image) { decodeFailures_.fetch_add(1); return; }
    uint64_t unset = 0;
    firstDecodedNanos_.compare_exchange_strong(unset, nowNanos());
    decodedFrames_.fetch_add(1);
    CMVideoFormatDescriptionRef format = nullptr;
    if (CMVideoFormatDescriptionCreateForImageBuffer(kCFAllocatorDefault, image, &format) != noErr) {
        decodeFailures_.fetch_add(1); return;
    }
    CMSampleTimingInfo timing = {CMTimeMake(1, kFrameRate), pts, kCMTimeInvalid};
    CMSampleBufferRef sample = nullptr;
    const OSStatus sampleStatus = CMSampleBufferCreateReadyWithImageBuffer(kCFAllocatorDefault, image, format, &timing, &sample);
    CFRelease(format);
    if (sampleStatus != noErr || !sample) { decodeFailures_.fetch_add(1); return; }
    CFRetain(sample);
    CFRelease(sample);
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!this->finished_.load()) {
            [this->view_.displayLayer enqueueSampleBuffer:sample];
            this->displayFrames_.fetch_add(1);
            if (this->firstReadyNanos_ == 0 && this->view_.displayLayer.isReadyForDisplay) {
                this->firstReadyNanos_ = nowNanos();
            }
        }
        CFRelease(sample);
    });
}

std::string AndroidReceiver::readText(const std::string &path) {
    std::ifstream input(path);
    std::ostringstream out;
    out << input.rdbuf();
    return out.str();
}

void AndroidReceiver::finish(const std::string &status, const std::string &error) {
    if (finished_.exchange(true)) return;
    if (!error.empty()) error_ = error;
    answerReady_.notify_all();
    gatheringReady_.notify_all();
    const int socket = signalSocket_.exchange(-1);
    if (socket >= 0) {
        shutdown(socket, SHUT_RDWR);
        close(socket);
    }
    {
        std::lock_guard<std::mutex> lock(decoderMutex_);
        if (decoder_) { VTDecompressionSessionInvalidate(decoder_); CFRelease(decoder_); decoder_ = nullptr; }
        if (decoderFormat_) { CFRelease(decoderFormat_); decoderFormat_ = nullptr; }
    }
    writeReport(status, error);
    // Preserve the observed failure/connection state before our own close changes it.
    if (peer_) peer_->close();
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 250 * NSEC_PER_MSEC), dispatch_get_main_queue(), ^{
        [NSApp terminate:nil];
    });
}

void AndroidReceiver::writeReport(const std::string &status, const std::string &error) {
    const auto outcome = receiver_diagnostics::classify(status, error);
    std::string senderState;
    std::string sessionId;
    bool gatheringComplete;
    {
        std::lock_guard<std::mutex> lock(stateMutex_);
        senderState = senderState_;
        sessionId = sessionId_;
        gatheringComplete = gatheringComplete_;
    }
    auto stateText = [](auto state) { std::ostringstream out; out << state; return out.str(); };
    // writeReport runs before our own close(); getters also avoid callback lag.
    if (peer_) senderState = stateText(peer_->state());
    const auto ms = [&](uint64_t value) -> std::string { return value ? std::to_string(elapsedMs(startNanos_, value)) : "null"; };
    const bool presentationPass = decodedFrames_.load() > 0 && displayFrames_.load() > 0 && view_ && view_.displayLayer.isReadyForDisplay;
    const std::string report = object({
        {"experiment", quote("android-macos-vertical-slice")},
        {"status", quote(status)},
        {"outcome", object({{"stage", quote(outcome.stage)}, {"code", quote(outcome.code)}})},
        {"sessionId", quote(sessionId)},
        {"bindAddress", quote(options_.bindAddress)},
        {"bindInterface", quote(options_.bindInterface)},
        {"signalingPort", std::to_string(options_.port)},
        {"durationSeconds", std::to_string(options_.durationSeconds)},
        {"diagnostics", object({
            {"tcpAccepted", tcpAccepted_.load() ? "true" : "false"},
            {"offerAuthenticated", authenticated_.load() ? "true" : "false"},
            {"remoteDescriptionAccepted", remoteDescriptionAccepted_.load() ? "true" : "false"},
            {"iceGatheringComplete", gatheringComplete ? "true" : "false"},
            {"answerSent", answerSent_.load() ? "true" : "false"},
            {"acceptTimeoutSeconds", std::to_string(options_.acceptTimeoutSeconds)},
            {"offerReadTimeoutSeconds", std::to_string(receiver_diagnostics::kOfferReadTimeoutSeconds)},
            {"firstFrameDeadline", quote("trial end measured from PeerConnection connected")},
            {"firstFrameTimeoutSeconds", std::to_string(options_.durationSeconds)},
            {"preConnectionTimeoutSeconds", std::to_string(preConnectionTimeoutSeconds())},
        })},
        {"workload", object({
            {"width", std::to_string(kWidth)},
            {"height", std::to_string(kHeight)},
            {"targetFrameRate", std::to_string(kFrameRate)},
        })},
        {"security", object({
            {"signaling", quote("experimental HMAC-SHA256 pairing code over bounded TCP signaling")},
            {"media", quote("WebRTC DTLS/SRTP via libdatachannel")},
            {"productionStatus", quote("NOT_PRODUCTION_AUTHENTICATION")},
        })},
        {"connection", object({
            {"connectionReadyMs", ms(connectionNanos_)},
            {"peerStateAtReport", quote(senderState)},
            {"iceStateAtReport", quote(peer_ ? stateText(peer_->iceState()) : "not_created")},
            {"iceGatheringStateAtReport", quote(peer_ ? stateText(peer_->gatheringState()) : "not_created")},
            {"connectionGate", connectionNanos_ ? quote("PASS") : quote("NOT_TESTED")},
        })},
        {"capture", object({
            {"source", quote("Android MediaProjection screen capture; Android status is not observable from this Mac report")},
            {"permissionGate", quote("NOT_TESTED: no physical Android execution in this environment")},
            {"captureStopGate", quote("NOT_TESTED")},
        })},
        {"codec", object({
            {"negotiated", quote("H264/90000 payload=96 profile-level-id=42e01f packetization-mode=1")},
            {"decoder", quote("VideoToolbox VTDecompressionSession")},
            {"hardwareAcceleration", quote("NOT_OBSERVABLE")},
        })},
        {"frames", object({
            {"rtpFramesReceived", std::to_string(receivedFrames_.load())},
            {"h264FramesDecoded", std::to_string(decodedFrames_.load())},
            {"displayEnqueueCount", std::to_string(displayFrames_.load())},
            {"decodeFailureCount", std::to_string(decodeFailures_.load())},
            {"decoderSetupFailureCount", std::to_string(decoderSetupFailures_.load())},
            {"decodeDropCount", std::to_string(decodeDrops_.load())},
            {"presentationGate", quote(presentationPass ? "PASS" : "NOT_TESTED")},
            {"perFrameDisplayedFreshness", quote("NOT_OBSERVABLE: AVSampleBufferDisplayLayer does not expose displayed sample identity")},
        })},
        {"timing", object({
            {"firstRtpFrameReceivedMs", ms(firstReceivedNanos_.load())},
            {"firstDecodedFrameMs", ms(firstDecodedNanos_.load())},
            {"firstReadyForDisplayMs", ms(firstReadyNanos_)},
        })},
        {"environmentRecord", options_.environmentPath.empty() ? "null" : quote(readText(options_.environmentPath))},
        {"libdatachannelRevision", quote(kLibDataChannelRevision)},
        {"input", object({
            {"gate", quote("NOT_TESTED: remote Android input is not implemented in this slice")},
        })},
        {"resourceMeasurementLimits", quote("Android CPU/memory/battery/thermal and separate WindowServer/GPU attribution require device-side instrumentation and are NOT_TESTED")},
        {"statusError", outcome.error[0] == '\0' ? "null" : quote(outcome.error)},
    });
    const std::filesystem::path path(options_.output);
    std::error_code ec;
    if (std::filesystem::exists(path, ec)) {
        std::fprintf(stderr, "ERROR: refusing to overwrite report\n");
        return;
    }
    if (!path.parent_path().empty()) std::filesystem::create_directories(path.parent_path(), ec);
    std::ofstream output(options_.output);
    if (!output) {
        std::fprintf(stderr, "ERROR: could not write report\n");
        return;
    }
    output << report << '\n';
}

} // namespace

static Options gOptions;

@interface AndroidReceiverAppDelegate : NSObject <NSApplicationDelegate, NSWindowDelegate> {
    AndroidReceiver *_receiver;
}
@end

@implementation AndroidReceiverAppDelegate
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    try {
        _receiver = new AndroidReceiver(gOptions);
        _receiver->start();
        _receiver->window().delegate = self;
    } catch (const std::exception &) {
        std::fprintf(stderr, "ERROR: receiver startup failed\n");
        delete _receiver;
        _receiver = nullptr;
        [NSApp terminate:nil];
    }
}
- (void)applicationWillTerminate:(NSNotification *)notification {
    (void)notification;
    if (_receiver) _receiver->abort("application terminated");
}
- (BOOL)windowShouldClose:(NSWindow *)sender {
    (void)sender;
    if (_receiver) _receiver->abort("receiver window closed");
    return YES;
}
@end

int main(int argc, char **argv) {
    try {
        gOptions = Options::parse(argc, argv);
        @autoreleasepool {
            NSApplication *application = [NSApplication sharedApplication];
            [application setActivationPolicy:NSApplicationActivationPolicyRegular];
            AndroidReceiverAppDelegate *delegate = [[AndroidReceiverAppDelegate alloc] init];
            application.delegate = delegate;
            [application run];
        }
        return 0;
    } catch (const std::exception &) {
        std::fprintf(stderr, "ERROR: receiver argument or startup validation failed\n");
        return 2;
    }
}
