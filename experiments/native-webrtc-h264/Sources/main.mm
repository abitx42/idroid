#import <AVFoundation/AVFoundation.h>
#import <AppKit/AppKit.h>
#import <CoreMedia/CoreMedia.h>
#import <CoreVideo/CoreVideo.h>
#import <QuartzCore/QuartzCore.h>
#import <VideoToolbox/VideoToolbox.h>

#include "rtc/rtc.hpp"

#include <mach/mach.h>
#include <mach/mach_time.h>

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <filesystem>
#include <iomanip>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

namespace {

constexpr int kVideoWidth = 640;
constexpr int kVideoHeight = 360;
constexpr int kTargetFrameRate = 30;
constexpr double kInputTimeoutMs = 1000.0;
constexpr uint8_t kH264PayloadType = 96;
constexpr rtc::SSRC kVideoSsrc = 42;

struct Options {
    std::string output = "results/native-webrtc-h264.json";
    std::string environment = ".build/environment.txt";
    double durationSeconds = 30.0;

    static Options parse(int argc, char **argv) {
        Options options;
        for (int index = 1; index < argc; ++index) {
            const std::string argument = argv[index];
            auto requireValue = [&](const char *name) -> std::string {
                if (index + 1 >= argc) {
                    throw std::runtime_error(std::string("missing value for ") + name);
                }
                return argv[++index];
            };

            if (argument == "--output") {
                options.output = requireValue("--output");
            } else if (argument == "--environment") {
                options.environment = requireValue("--environment");
            } else if (argument == "--duration-seconds") {
                const auto value = requireValue("--duration-seconds");
                options.durationSeconds = std::stod(value);
                if (!(options.durationSeconds > 0.0 && options.durationSeconds <= 3600.0)) {
                    throw std::runtime_error("--duration-seconds must be in (0, 3600]");
                }
            } else if (argument == "--help" || argument == "-h") {
                std::puts("Usage: native-webrtc-h264 [--output PATH] [--environment PATH] [--duration-seconds N]");
                std::exit(0);
            } else {
                throw std::runtime_error("invalid argument: " + argument);
            }
        }
        return options;
    }
};

uint64_t monotonicNow() {
    return mach_absolute_time();
}

double elapsedMs(uint64_t start, uint64_t now) {
    static mach_timebase_info_data_t timebase = [] {
        mach_timebase_info_data_t value{};
        mach_timebase_info(&value);
        return value;
    }();
    if (now < start) return 0.0;
    const long double nanos = static_cast<long double>(now - start) * timebase.numer / timebase.denom;
    return static_cast<double>(nanos / 1'000'000.0L);
}

std::string readText(const std::string &path) {
    std::ifstream input(path);
    if (!input) return "NOT_AVAILABLE";
    std::ostringstream contents;
    contents << input.rdbuf();
    return contents.str();
}

std::string jsonEscape(const std::string &value) {
    std::ostringstream escaped;
    for (const unsigned char character : value) {
        switch (character) {
        case '"': escaped << "\\\""; break;
        case '\\': escaped << "\\\\"; break;
        case '\b': escaped << "\\b"; break;
        case '\f': escaped << "\\f"; break;
        case '\n': escaped << "\\n"; break;
        case '\r': escaped << "\\r"; break;
        case '\t': escaped << "\\t"; break;
        default:
            if (character < 0x20) {
                escaped << "\\u" << std::hex << std::setw(4) << std::setfill('0')
                        << static_cast<int>(character) << std::dec;
            } else {
                escaped << character;
            }
        }
    }
    return escaped.str();
}

struct TaskUsage {
    double cpuSeconds = 0.0;
    uint64_t residentBytes = 0;
};

std::optional<TaskUsage> currentTaskUsage() {
    mach_task_basic_info info{};
    mach_msg_type_number_t count = mach_msg_type_number_t(sizeof(mach_task_basic_info) / sizeof(natural_t));
    const kern_return_t result = task_info(
        mach_task_self_,
        MACH_TASK_BASIC_INFO,
        reinterpret_cast<task_info_t>(&info),
        &count
    );
    if (result != KERN_SUCCESS) return std::nullopt;
    const double user = info.user_time.seconds + info.user_time.microseconds / 1'000'000.0;
    const double system = info.system_time.seconds + info.system_time.microseconds / 1'000'000.0;
    return TaskUsage{user + system, static_cast<uint64_t>(info.resident_size)};
}

struct InputSample {
    int sequence = 0;
    std::string kind;
    bool trusted = false;
    uint64_t handlerNanos = 0;
    bool acknowledged = false;
    bool missingAcknowledgement = false;
    bool timedOut = false;
    std::optional<double> frameBoundaryMs;
};

struct ResourceSample {
    double elapsedSeconds = 0.0;
    double cpuSeconds = 0.0;
    uint64_t residentBytes = 0;
};

} // namespace

@interface InputTargetView : NSView
@property(nonatomic, copy) void (^inputHandler)(NSString *kind, BOOL trusted);
@property(nonatomic, assign) CGFloat phase;
@property(nonatomic, assign) NSUInteger inputSequence;
@end

@implementation InputTargetView

- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)canBecomeKeyView { return YES; }

- (void)mouseDown:(NSEvent *)event {
    (void)event;
    [self.window makeFirstResponder:self];
    if (self.inputHandler) self.inputHandler(@"manual-click", YES);
}

- (void)keyDown:(NSEvent *)event {
    (void)event;
    if (self.inputHandler) self.inputHandler(@"manual-key", YES);
}

- (void)drawRect:(NSRect)dirtyRect {
    (void)dirtyRect;
    [[NSColor colorWithCalibratedWhite:0.10 alpha:1.0] setFill];
    NSRectFill(self.bounds);

    const BOOL highlighted = self.inputSequence % 2 == 0 && self.inputSequence != 0;
    if (highlighted) {
        [[NSColor colorWithCalibratedRed:0.12 green:0.31 blue:0.49 alpha:1.0] setFill];
        NSRectFill(self.bounds);
    }

    NSBezierPath *border = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 1, 1) xRadius:10 yRadius:10];
    [[NSColor colorWithCalibratedRed:0.43 green:0.62 blue:1 alpha:1] setStroke];
    border.lineWidth = 2;
    [border stroke];

    const CGFloat sweepWidth = self.bounds.size.width * 0.24;
    const CGFloat sweepX = (self.bounds.size.width + sweepWidth) * self.phase - sweepWidth;
    NSRect sweep = NSMakeRect(sweepX, 0, sweepWidth, self.bounds.size.height);
    [[NSColor colorWithCalibratedRed:0.43 green:0.62 blue:1 alpha:0.22] setFill];
    NSRectFillUsingOperation(sweep, NSCompositingOperationSourceOver);

    NSDictionary *attributes = @{
        NSFontAttributeName: [NSFont systemFontOfSize:18 weight:NSFontWeightMedium],
        NSForegroundColorAttributeName: NSColor.whiteColor,
    };
    NSString *text = @"Input target — click at least ten times or focus and press keys";
    NSSize size = [text sizeWithAttributes:attributes];
    [text drawAtPoint:NSMakePoint(MAX(16, (self.bounds.size.width - size.width) / 2),
                                  (self.bounds.size.height - size.height) / 2)
       withAttributes:attributes];
}

@end

@interface ProbeRootView : NSView
@property(nonatomic, strong) AVSampleBufferDisplayLayer *displayLayer;
@property(nonatomic, strong) InputTargetView *inputTarget;
@end

@implementation ProbeRootView

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (self) {
        self.wantsLayer = YES;
        self.layer = [CALayer layer];
        self.layer.backgroundColor = [NSColor colorWithCalibratedWhite:0.055 alpha:1].CGColor;
        self.displayLayer = [AVSampleBufferDisplayLayer layer];
        self.displayLayer.videoGravity = AVLayerVideoGravityResizeAspect;
        self.displayLayer.backgroundColor = NSColor.blackColor.CGColor;
        [self.layer addSublayer:self.displayLayer];
        self.inputTarget = [[InputTargetView alloc] initWithFrame:NSZeroRect];
        [self addSubview:self.inputTarget];
    }
    return self;
}

- (void)layout {
    [super layout];
    const CGFloat margin = 40;
    const CGFloat inputHeight = 100;
    const CGFloat inputY = 28;
    self.inputTarget.frame = NSMakeRect(margin, inputY, self.bounds.size.width - margin * 2, inputHeight);
    const CGFloat videoY = inputY + inputHeight + 28;
    self.displayLayer.frame = NSMakeRect(margin, videoY, self.bounds.size.width - margin * 2,
                                          self.bounds.size.height - videoY - 28);
}

@end

namespace {

class NativeProbe {
public:
    explicit NativeProbe(Options options) : options_(std::move(options)) {}
    ~NativeProbe();

    void start();
    void requestAbort(const std::string &reason);
    NSWindow *window() const { return window_; }

    void handleEncodedSample(OSStatus status, CMSampleBufferRef sampleBuffer);
    void handleReceivedFrame(rtc::binary frame, rtc::FrameInfo info);
    void handleDecodedFrame(OSStatus status, CVImageBufferRef imageBuffer, CMTime presentationTimeStamp);
    void recordDisplayTick(uint64_t tickNanos);

private:
    void configureWindow();
    void configurePeers();
    void configureEncoder();
    void configureDisplayLink();
    void beginMediaRun();
    void encodeNextFrame();
    void finish(const std::string &status, const std::string &error = {});
    void stopResources();

    void recordInput(const std::string &kind, bool trusted);
    void resolveInputSamples(uint64_t displayTickNanos);
    void markInputTimeouts();
    void scheduleScriptedInputs();
    void sampleResources();
    void observeReadyForDisplay();
    void writeReport(const std::string &status, const std::string &error);

    bool configureDecoderForFormat(const std::vector<uint8_t> &sps, const std::vector<uint8_t> &pps);
    static CVPixelBufferRef makePixelBuffer(int frameIndex);
    static std::vector<std::vector<uint8_t>> splitAnnexB(const uint8_t *data, size_t size);
    static std::vector<uint8_t> sampleBufferToAnnexB(CMSampleBufferRef sampleBuffer, bool *isKeyFrame);
    static std::string stateName(rtc::PeerConnection::State state);
    static double percentile(const std::vector<double> &values, double fraction);

    Options options_;
    uint64_t startNanos_ = 0;
    uint64_t setupReadyNanos_ = 0;
    uint64_t mediaStartNanos_ = 0;
    uint64_t connectionReadyNanos_ = 0;
    NSWindow *window_ = nil;
    ProbeRootView *rootView_ = nil;
    NSTimer *frameTimer_ = nil;
    NSTimer *resourceTimer_ = nil;
    NSTimer *inputTimeoutTimer_ = nil;
    NSTimer *phaseTimer_ = nil;
    dispatch_block_t timeoutBlock_ = nil;
    CVDisplayLinkRef displayLink_ = nullptr;
    VTCompressionSessionRef encoder_ = nullptr;
    VTDecompressionSessionRef decoder_ = nullptr;
    CMVideoFormatDescriptionRef decoderFormat_ = nullptr;

    std::shared_ptr<rtc::PeerConnection> senderPeer_;
    std::shared_ptr<rtc::PeerConnection> receiverPeer_;
    std::shared_ptr<rtc::Track> senderTrack_;
    std::shared_ptr<rtc::Track> receiverTrack_;

    std::atomic<bool> finished_{false};
    std::atomic<bool> mediaStarted_{false};
    std::atomic<bool> senderTrackOpen_{false};
    std::atomic<int> sourceFrames_{0};
    std::atomic<int> encodedFrames_{0};
    std::atomic<int> sentFrames_{0};
    std::atomic<int> receivedFrames_{0};
    std::atomic<int> decodedFrames_{0};
    std::atomic<int> displayEnqueueCount_{0};
    std::atomic<int> encodeFailureCount_{0};
    std::atomic<int> decodeFailureCount_{0};
    std::atomic<int> decodeDropCount_{0};
    std::atomic<uint64_t> firstEncodedNanos_{0};
    std::atomic<uint64_t> firstReceivedNanos_{0};
    std::atomic<uint64_t> firstDecodedNanos_{0};
    std::mutex decoderMutex_;
    std::mutex stateMutex_;
    std::string senderState_ = "new";
    std::string receiverState_ = "new";
    std::string offerSdp_;
    std::string answerSdp_;
    std::string failureReason_;

    int sourceFrameIndex_ = 0;
    int nextInputSequence_ = 1;
    int scriptedCount_ = 0;
    int manualCount_ = 0;
    int displayTickCount_ = 0;
    std::optional<double> firstReadyForDisplayMs_;
    uint64_t lastDisplayTickNanos_ = 0;
    std::vector<double> displayTickIntervalsMs_;
    std::vector<InputSample> inputSamples_;
    std::vector<InputSample> pendingInputs_;
    std::vector<ResourceSample> resourceSamples_;
    std::vector<uint8_t> latestSps_;
    std::vector<uint8_t> latestPps_;
    std::string negotiatedCodec_ = "H264/90000 payload=96 profile-level-id=42e01f packetization-mode=1";
    std::string negotiatedProfile_ = "42e01f;packetization-mode=1;level-asymmetry-allowed=1";
};

} // namespace

static Options gOptions;

@interface NativeWebRTCH264AppDelegate : NSObject <NSApplicationDelegate, NSWindowDelegate> {
    NativeProbe *_probe;
}
@end

@implementation NativeWebRTCH264AppDelegate

- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    (void)notification;
    try {
        _probe = new NativeProbe(gOptions);
        _probe->start();
        _probe->window().delegate = self;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "ERROR: %s\n", error.what());
        delete _probe;
        _probe = nullptr;
        [NSApp terminate:nil];
    }
}

- (void)applicationWillTerminate:(NSNotification *)notification {
    (void)notification;
    if (_probe != nullptr) _probe->requestAbort("application terminated before the probe completed");
}

- (BOOL)windowShouldClose:(NSWindow *)sender {
    (void)sender;
    if (_probe != nullptr) _probe->requestAbort("window closed before the probe completed");
    return YES;
}

@end

int main(int argc, char **argv) {
    try {
        gOptions = Options::parse(argc, argv);
        @autoreleasepool {
            NSApplication *application = [NSApplication sharedApplication];
            [application setActivationPolicy:NSApplicationActivationPolicyRegular];
            NativeWebRTCH264AppDelegate *delegate = [[NativeWebRTCH264AppDelegate alloc] init];
            application.delegate = delegate;
            [application run];
        }
        return 0;
    } catch (const std::exception &error) {
        std::fprintf(stderr, "ERROR: %s\n", error.what());
        return 2;
    }
}

namespace {

std::string jsonString(const std::string &value) {
    return "\"" + jsonEscape(value) + "\"";
}

std::string jsonBool(bool value) { return value ? "true" : "false"; }
std::string jsonNull() { return "null"; }

std::string jsonInt(int64_t value) { return std::to_string(value); }
std::string jsonUInt(uint64_t value) { return std::to_string(value); }

std::string jsonDouble(double value) {
    if (!std::isfinite(value)) return "null";
    std::ostringstream output;
    output << std::setprecision(9) << value;
    return output.str();
}

std::string jsonNullable(const std::optional<double> &value) {
    return value ? jsonDouble(*value) : jsonNull();
}

std::string jsonArray(const std::vector<std::string> &values) {
    std::ostringstream output;
    output << "[";
    for (size_t index = 0; index < values.size(); ++index) {
        if (index != 0) output << ",";
        output << values[index];
    }
    output << "]";
    return output.str();
}

std::string jsonObject(const std::vector<std::pair<std::string, std::string>> &fields) {
    std::ostringstream output;
    output << "{";
    for (size_t index = 0; index < fields.size(); ++index) {
        if (index != 0) output << ",";
        output << jsonString(fields[index].first) << ":" << fields[index].second;
    }
    output << "}";
    return output.str();
}

static void compressionOutputCallback(void *outputCallbackRefCon,
                                      void *sourceFrameRefCon,
                                      OSStatus status,
                                      VTEncodeInfoFlags infoFlags,
                                      CMSampleBufferRef sampleBuffer) {
    (void)sourceFrameRefCon;
    (void)infoFlags;
    auto *probe = static_cast<NativeProbe *>(outputCallbackRefCon);
    if (probe != nullptr) probe->handleEncodedSample(status, sampleBuffer);
}

static void decompressionOutputCallback(void *decompressionOutputRefCon,
                                        void *sourceFrameRefCon,
                                        OSStatus status,
                                        VTDecodeInfoFlags infoFlags,
                                        CVImageBufferRef imageBuffer,
                                        CMTime presentationTimeStamp,
                                        CMTime presentationDuration) {
    (void)sourceFrameRefCon;
    (void)infoFlags;
    (void)presentationDuration;
    auto *probe = static_cast<NativeProbe *>(decompressionOutputRefCon);
    if (probe != nullptr) probe->handleDecodedFrame(status, imageBuffer, presentationTimeStamp);
}

static CVReturn displayLinkCallback(CVDisplayLinkRef displayLink,
                                    const CVTimeStamp *inNow,
                                    const CVTimeStamp *inOutputTime,
                                    CVOptionFlags flagsIn,
                                    CVOptionFlags *flagsOut,
                                    void *displayLinkContext) {
    (void)displayLink;
    (void)inNow;
    (void)inOutputTime;
    (void)flagsIn;
    (void)flagsOut;
    auto *probe = static_cast<NativeProbe *>(displayLinkContext);
    if (probe != nullptr) {
        const uint64_t tickNanos = monotonicNow();
        dispatch_async(dispatch_get_main_queue(), ^{
            probe->recordDisplayTick(tickNanos);
        });
    }
    return kCVReturnSuccess;
}

NativeProbe::~NativeProbe() {
    stopResources();
}

void NativeProbe::start() {
    startNanos_ = monotonicNow();
    configureWindow();
    configureDisplayLink();
    configurePeers();
    setupReadyNanos_ = monotonicNow();

    sampleResources();
    resourceTimer_ = [NSTimer scheduledTimerWithTimeInterval:1.0 repeats:YES block:^(NSTimer *timer) {
        (void)timer;
        this->sampleResources();
    }];
    inputTimeoutTimer_ = [NSTimer scheduledTimerWithTimeInterval:0.05 repeats:YES block:^(NSTimer *timer) {
        (void)timer;
        this->markInputTimeouts();
    }];
    phaseTimer_ = [NSTimer scheduledTimerWithTimeInterval:1.0 / 60.0 repeats:YES block:^(NSTimer *timer) {
        (void)timer;
        if (this->rootView_ == nil) return;
        this->rootView_.inputTarget.phase = fmod(this->rootView_.inputTarget.phase + 0.017, 1.0);
        this->rootView_.inputTarget.needsDisplay = YES;
    }];
    scheduleScriptedInputs();

    timeoutBlock_ = dispatch_block_create(DISPATCH_BLOCK_INHERIT_QOS_CLASS, ^{
        if (!this->finished_.load()) {
            this->finish("TIMEOUT", "native WebRTC/H.264 probe exceeded its bounded timeout");
        }
    });
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                 static_cast<int64_t>((options_.durationSeconds + 15.0) * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), timeoutBlock_);
}

void NativeProbe::configureWindow() {
    rootView_ = [[ProbeRootView alloc] initWithFrame:NSMakeRect(0, 0, 1120, 760)];
    rootView_.inputTarget.inputHandler = ^(NSString *kind, BOOL trusted) {
        this->recordInput(kind.UTF8String == nullptr ? "manual" : kind.UTF8String, trusted);
    };

    CMTimebaseRef timebase = nullptr;
    if (CMTimebaseCreateWithSourceClock(kCFAllocatorDefault, CMClockGetHostTimeClock(), &timebase) == noErr) {
        CMTimebaseSetTime(timebase, kCMTimeZero);
        CMTimebaseSetRate(timebase, 1.0);
        rootView_.displayLayer.controlTimebase = timebase;
        CFRelease(timebase);
    }
    rootView_.displayLayer.preventsDisplaySleepDuringVideoPlayback = YES;

    window_ = [[NSWindow alloc]
        initWithContentRect:rootView_.frame
                  styleMask:(NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                             NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable)
                    backing:NSBackingStoreBuffered
                      defer:NO];
    window_.title = @"iDroid Native WebRTC/H.264 Receiver";
    window_.contentView = rootView_;
    [window_ center];
    [window_ makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
}

void NativeProbe::configureDisplayLink() {
    CVDisplayLinkRef link = nullptr;
    if (CVDisplayLinkCreateWithActiveCGDisplays(&link) != kCVReturnSuccess || link == nullptr) {
        throw std::runtime_error("CVDisplayLink is unavailable");
    }
    if (CVDisplayLinkSetOutputCallback(link, displayLinkCallback, this) != kCVReturnSuccess) {
        CVDisplayLinkRelease(link);
        throw std::runtime_error("CVDisplayLink callback could not be installed");
    }
    displayLink_ = link;
    if (CVDisplayLinkStart(displayLink_) != kCVReturnSuccess) {
        CVDisplayLinkRelease(displayLink_);
        displayLink_ = nullptr;
        throw std::runtime_error("CVDisplayLink could not start");
    }
}

void NativeProbe::configurePeers() {
    rtc::InitLogger(rtc::LogLevel::Warning);
    rtc::Configuration configuration;
    configuration.iceServers.clear();
    senderPeer_ = std::make_shared<rtc::PeerConnection>(configuration);
    receiverPeer_ = std::make_shared<rtc::PeerConnection>(configuration);

    senderPeer_->onStateChange([this](rtc::PeerConnection::State state) {
        {
            std::lock_guard<std::mutex> lock(stateMutex_);
            senderState_ = stateName(state);
        }
        if (state == rtc::PeerConnection::State::Connected) {
            dispatch_async(dispatch_get_main_queue(), ^{
                this->beginMediaRun();
            });
        } else if (state == rtc::PeerConnection::State::Failed) {
            dispatch_async(dispatch_get_main_queue(), ^{
                this->finish("FAIL", "sender PeerConnection entered failed state");
            });
        }
    });
    receiverPeer_->onStateChange([this](rtc::PeerConnection::State state) {
        {
            std::lock_guard<std::mutex> lock(stateMutex_);
            receiverState_ = stateName(state);
        }
        if (state == rtc::PeerConnection::State::Connected) {
            dispatch_async(dispatch_get_main_queue(), ^{
                this->beginMediaRun();
            });
        } else if (state == rtc::PeerConnection::State::Failed) {
            dispatch_async(dispatch_get_main_queue(), ^{
                this->finish("FAIL", "receiver PeerConnection entered failed state");
            });
        }
    });

    senderPeer_->onLocalCandidate([this](rtc::Candidate candidate) {
        if (this->receiverPeer_) this->receiverPeer_->addRemoteCandidate(std::move(candidate));
    });
    receiverPeer_->onLocalCandidate([this](rtc::Candidate candidate) {
        if (this->senderPeer_) this->senderPeer_->addRemoteCandidate(std::move(candidate));
    });
    senderPeer_->onLocalDescription([this](rtc::Description description) {
        {
            std::lock_guard<std::mutex> lock(stateMutex_);
            offerSdp_ = std::string(description);
        }
        if (this->receiverPeer_) {
            this->receiverPeer_->setRemoteDescription(description);
        }
    });
    receiverPeer_->onLocalDescription([this](rtc::Description description) {
        {
            std::lock_guard<std::mutex> lock(stateMutex_);
            answerSdp_ = std::string(description);
        }
        if (this->senderPeer_) this->senderPeer_->setRemoteDescription(description);
    });

    rtc::Description::Video senderMedia("video", rtc::Description::Direction::SendOnly);
    senderMedia.addH264Codec(kH264PayloadType);
    senderMedia.setBitrate(3000);
    senderMedia.addSSRC(kVideoSsrc, "native-video-send");
    senderTrack_ = senderPeer_->addTrack(senderMedia);
    auto packetization = std::make_shared<rtc::RtpPacketizationConfig>(
        kVideoSsrc, "idroid-native", kH264PayloadType, rtc::H264RtpPacketizer::ClockRate);
    senderTrack_->setMediaHandler(std::make_shared<rtc::H264RtpPacketizer>(
        rtc::H264RtpPacketizer::Separator::StartSequence, packetization));
    senderTrack_->onOpen([this]() {
        senderTrackOpen_.store(true);
        dispatch_async(dispatch_get_main_queue(), ^{
            this->beginMediaRun();
        });
    });
    senderTrack_->onError([this](std::string error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            this->finish("FAIL", "sender track error: " + error);
        });
    });

    rtc::Description::Video receiverMedia("video", rtc::Description::Direction::RecvOnly);
    receiverMedia.addH264Codec(kH264PayloadType);
    receiverMedia.setBitrate(3000);
    receiverTrack_ = receiverPeer_->addTrack(receiverMedia);
    receiverTrack_->setMediaHandler(std::make_shared<rtc::H264RtpDepacketizer>());
    receiverTrack_->chainMediaHandler(std::make_shared<rtc::RtcpReceivingSession>());
    receiverTrack_->onFrame([this](rtc::binary frame, rtc::FrameInfo info) {
        this->handleReceivedFrame(std::move(frame), info);
    });
    receiverTrack_->onError([this](std::string error) {
        dispatch_async(dispatch_get_main_queue(), ^{
            this->finish("FAIL", "receiver track error: " + error);
        });
    });

    senderPeer_->setLocalDescription(rtc::Description::Type::Offer);
}

void NativeProbe::beginMediaRun() {
    if (finished_.load() || mediaStarted_.load() || !senderTrackOpen_.load()) return;
    {
        std::lock_guard<std::mutex> lock(stateMutex_);
        if (senderState_ != "connected" || receiverState_ != "connected") return;
    }
    if (connectionReadyNanos_ == 0) connectionReadyNanos_ = monotonicNow();
    if (connectionReadyNanos_ == 0) return;
    mediaStarted_.store(true);
    mediaStartNanos_ = monotonicNow();
    try {
        configureEncoder();
    } catch (const std::exception &error) {
        finish("FAIL", error.what());
        return;
    }
    encodeNextFrame();
    frameTimer_ = [NSTimer scheduledTimerWithTimeInterval:1.0 / kTargetFrameRate repeats:YES block:^(NSTimer *timer) {
        (void)timer;
        this->encodeNextFrame();
    }];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,
                                 static_cast<int64_t>(options_.durationSeconds * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
        if (!this->finished_.load()) this->finish("COMPLETED");
    });
}

void NativeProbe::configureEncoder() {
    if (encoder_ != nullptr) return;
    const OSStatus status = VTCompressionSessionCreate(
        kCFAllocatorDefault,
        kVideoWidth,
        kVideoHeight,
        kCMVideoCodecType_H264,
        nullptr,
        nullptr,
        nullptr,
        compressionOutputCallback,
        this,
        &encoder_
    );
    if (status != noErr || encoder_ == nullptr) {
        throw std::runtime_error("VTCompressionSessionCreate failed: " + std::to_string(status));
    }

    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_RealTime, kCFBooleanTrue);
    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse);
    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_ProfileLevel,
                         kVTProfileLevel_H264_ConstrainedBaseline_AutoLevel);

    CFNumberRef frameRate = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &kTargetFrameRate);
    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_ExpectedFrameRate, frameRate);
    CFRelease(frameRate);

    int32_t bitrate = 2'000'000;
    CFNumberRef averageBitrate = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &bitrate);
    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_AverageBitRate, averageBitrate);
    CFRelease(averageBitrate);

    int32_t keyFrameInterval = kTargetFrameRate;
    CFNumberRef maxKeyFrameInterval = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &keyFrameInterval);
    VTSessionSetProperty(encoder_, kVTCompressionPropertyKey_MaxKeyFrameInterval, maxKeyFrameInterval);
    CFRelease(maxKeyFrameInterval);

    const OSStatus prepareStatus = VTCompressionSessionPrepareToEncodeFrames(encoder_);
    if (prepareStatus != noErr) {
        throw std::runtime_error("VTCompressionSessionPrepareToEncodeFrames failed: " +
                                 std::to_string(prepareStatus));
    }
}

void NativeProbe::encodeNextFrame() {
    if (finished_.load() || !mediaStarted_.load() || encoder_ == nullptr) return;
    CVPixelBufferRef pixelBuffer = makePixelBuffer(sourceFrameIndex_);
    if (pixelBuffer == nullptr) {
        encodeFailureCount_.fetch_add(1);
        return;
    }
    const CMTime presentationTime = CMTimeMake(sourceFrameIndex_, kTargetFrameRate);
    const CMTime duration = CMTimeMake(1, kTargetFrameRate);
    VTEncodeInfoFlags flags = 0;
    const OSStatus status = VTCompressionSessionEncodeFrame(
        encoder_, pixelBuffer, presentationTime, duration, nullptr, nullptr, &flags);
    CVPixelBufferRelease(pixelBuffer);
    sourceFrameIndex_ += 1;
    sourceFrames_.fetch_add(1);
    if (status != noErr) encodeFailureCount_.fetch_add(1);
}

CVPixelBufferRef NativeProbe::makePixelBuffer(int frameIndex) {
    NSDictionary *attributes = @{
        (id)kCVPixelBufferIOSurfacePropertiesKey: @{},
        (id)kCVPixelBufferMetalCompatibilityKey: @YES,
    };
    CVPixelBufferRef pixelBuffer = nullptr;
    const CVReturn createStatus = CVPixelBufferCreate(
        kCFAllocatorDefault,
        kVideoWidth,
        kVideoHeight,
        kCVPixelFormatType_32BGRA,
        (__bridge CFDictionaryRef)attributes,
        &pixelBuffer
    );
    if (createStatus != kCVReturnSuccess || pixelBuffer == nullptr) return nullptr;
    if (CVPixelBufferLockBaseAddress(pixelBuffer, 0) != kCVReturnSuccess) {
        CVPixelBufferRelease(pixelBuffer);
        return nullptr;
    }

    uint8_t *baseAddress = static_cast<uint8_t *>(CVPixelBufferGetBaseAddress(pixelBuffer));
    const size_t bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer);
    const int barX = (frameIndex * 7) % (kVideoWidth - 120);
    for (int y = 0; y < kVideoHeight; ++y) {
        uint8_t *row = baseAddress + y * bytesPerRow;
        for (int x = 0; x < kVideoWidth; ++x) {
            const int offset = x * 4;
            const bool inBar = x >= barX && x < barX + 120 && y >= 92 && y < 212;
            const bool stripe = ((x + frameIndex * 3) / 32) % 2 == 0;
            if (inBar) {
                row[offset] = 0xE0;
                row[offset + 1] = 0x9A;
                row[offset + 2] = 0x43;
                row[offset + 3] = 0xFF;
            } else if (stripe) {
                row[offset] = 0x24;
                row[offset + 1] = 0x1B;
                row[offset + 2] = 0x2A;
                row[offset + 3] = 0xFF;
            } else {
                row[offset] = 0x16;
                row[offset + 1] = 0x12;
                row[offset + 2] = 0x18;
                row[offset + 3] = 0xFF;
            }
        }
    }
    CVPixelBufferUnlockBaseAddress(pixelBuffer, 0);
    return pixelBuffer;
}

std::vector<uint8_t> NativeProbe::sampleBufferToAnnexB(CMSampleBufferRef sampleBuffer, bool *isKeyFrame) {
    if (isKeyFrame != nullptr) *isKeyFrame = false;
    if (sampleBuffer == nullptr) return {};

    bool keyFrame = true;
    CFArrayRef attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, false);
    if (attachments != nullptr && CFArrayGetCount(attachments) > 0) {
        CFDictionaryRef attachment = static_cast<CFDictionaryRef>(CFArrayGetValueAtIndex(attachments, 0));
        const auto notSync = static_cast<CFBooleanRef>(CFDictionaryGetValue(attachment, kCMSampleAttachmentKey_NotSync));
        keyFrame = notSync != kCFBooleanTrue;
    }
    if (isKeyFrame != nullptr) *isKeyFrame = keyFrame;

    std::vector<uint8_t> output;
    auto appendStartCode = [&output]() {
        output.insert(output.end(), {0, 0, 0, 1});
    };

    if (keyFrame) {
        CMVideoFormatDescriptionRef format = CMSampleBufferGetFormatDescription(sampleBuffer);
        for (size_t index = 0; index < 2; ++index) {
            const uint8_t *parameterSet = nullptr;
            size_t parameterSetSize = 0;
            size_t parameterSetCount = 0;
            const OSStatus status = CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                format, index, &parameterSet, &parameterSetSize, &parameterSetCount, nullptr);
            if (status == noErr && parameterSet != nullptr && parameterSetSize > 0) {
                appendStartCode();
                output.insert(output.end(), parameterSet, parameterSet + parameterSetSize);
            }
        }
    }

    CMBlockBufferRef block = CMSampleBufferGetDataBuffer(sampleBuffer);
    if (block == nullptr) return {};
    const size_t totalLength = CMBlockBufferGetDataLength(block);
    std::vector<uint8_t> avcc(totalLength);
    if (CMBlockBufferCopyDataBytes(block, 0, totalLength, avcc.data()) != kCMBlockBufferNoErr) return {};

    size_t offset = 0;
    while (offset + 4 <= avcc.size()) {
        const uint32_t nalLength = (static_cast<uint32_t>(avcc[offset]) << 24) |
                                   (static_cast<uint32_t>(avcc[offset + 1]) << 16) |
                                   (static_cast<uint32_t>(avcc[offset + 2]) << 8) |
                                   static_cast<uint32_t>(avcc[offset + 3]);
        offset += 4;
        if (nalLength == 0 || offset + nalLength > avcc.size()) return {};
        appendStartCode();
        output.insert(output.end(), avcc.begin() + static_cast<ptrdiff_t>(offset),
                      avcc.begin() + static_cast<ptrdiff_t>(offset + nalLength));
        offset += nalLength;
    }
    return offset == avcc.size() ? output : std::vector<uint8_t>{};
}

std::vector<std::vector<uint8_t>> NativeProbe::splitAnnexB(const uint8_t *data, size_t size) {
    std::vector<std::vector<uint8_t>> nals;
    if (data == nullptr || size == 0) return nals;

    auto startCodeLength = [&](size_t offset) -> size_t {
        if (offset + 3 <= size && data[offset] == 0 && data[offset + 1] == 0 && data[offset + 2] == 1) {
            return 3;
        }
        if (offset + 4 <= size && data[offset] == 0 && data[offset + 1] == 0 &&
            data[offset + 2] == 0 && data[offset + 3] == 1) {
            return 4;
        }
        return 0;
    };

    size_t first = 0;
    while (first < size && startCodeLength(first) == 0) ++first;
    if (first == size) {
        nals.emplace_back(data, data + size);
        return nals;
    }

    while (first < size) {
        const size_t prefix = startCodeLength(first);
        if (prefix == 0) break;
        const size_t nalStart = first + prefix;
        size_t next = nalStart;
        while (next < size && startCodeLength(next) == 0) ++next;
        if (nalStart < next) nals.emplace_back(data + nalStart, data + next);
        first = next;
    }
    return nals;
}

void NativeProbe::handleEncodedSample(OSStatus status, CMSampleBufferRef sampleBuffer) {
    if (finished_.load()) return;
    if (status != noErr || sampleBuffer == nullptr) {
        encodeFailureCount_.fetch_add(1);
        return;
    }
    uint64_t unset = 0;
    firstEncodedNanos_.compare_exchange_strong(unset, monotonicNow());
    bool isKeyFrame = false;
    std::vector<uint8_t> frame = sampleBufferToAnnexB(sampleBuffer, &isKeyFrame);
    if (frame.empty() || !senderTrack_ || !senderTrack_->isOpen()) {
        encodeFailureCount_.fetch_add(1);
        return;
    }

    CMTime pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer);
    double seconds = CMTIME_IS_VALID(pts) ? CMTimeGetSeconds(pts) : 0.0;
    if (!std::isfinite(seconds) || seconds < 0) seconds = encodedFrames_.load() / static_cast<double>(kTargetFrameRate);
    rtc::FrameInfo info(static_cast<uint32_t>(seconds * rtc::H264RtpPacketizer::ClockRate));
    rtc::binary payload;
    payload.reserve(frame.size());
    for (uint8_t byte : frame) payload.push_back(static_cast<std::byte>(byte));
    senderTrack_->sendFrame(std::move(payload), info);
    encodedFrames_.fetch_add(1);
    sentFrames_.fetch_add(1);
}

bool NativeProbe::configureDecoderForFormat(const std::vector<uint8_t> &sps,
                                            const std::vector<uint8_t> &pps) {
    if (sps.empty() || pps.empty()) return false;
    std::lock_guard<std::mutex> lock(decoderMutex_);
    if (decoder_ != nullptr && sps == latestSps_ && pps == latestPps_) return true;

    const uint8_t *parameterSets[] = {sps.data(), pps.data()};
    const size_t parameterSetSizes[] = {sps.size(), pps.size()};
    CMVideoFormatDescriptionRef format = nullptr;
    const OSStatus formatStatus = CMVideoFormatDescriptionCreateFromH264ParameterSets(
        kCFAllocatorDefault,
        2,
        parameterSets,
        parameterSetSizes,
        4,
        &format
    );
    if (formatStatus != noErr || format == nullptr) return false;

    NSDictionary *destinationAttributes = @{
        (id)kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_32BGRA),
        (id)kCVPixelBufferMetalCompatibilityKey: @YES,
    };
    VTDecompressionOutputCallbackRecord callbackRecord = {
        decompressionOutputCallback,
        this,
    };
    VTDecompressionSessionRef newDecoder = nullptr;
    const OSStatus decoderStatus = VTDecompressionSessionCreate(
        kCFAllocatorDefault,
        format,
        nullptr,
        (__bridge CFDictionaryRef)destinationAttributes,
        &callbackRecord,
        &newDecoder
    );
    if (decoderStatus != noErr || newDecoder == nullptr) {
        CFRelease(format);
        return false;
    }

    if (decoder_ != nullptr) {
        VTDecompressionSessionWaitForAsynchronousFrames(decoder_);
        VTDecompressionSessionInvalidate(decoder_);
        CFRelease(decoder_);
    }
    if (decoderFormat_ != nullptr) CFRelease(decoderFormat_);
    decoder_ = newDecoder;
    decoderFormat_ = format;
    latestSps_ = sps;
    latestPps_ = pps;
    return true;
}

void NativeProbe::handleReceivedFrame(rtc::binary frame, rtc::FrameInfo info) {
    if (finished_.load()) return;
    uint64_t unset = 0;
    firstReceivedNanos_.compare_exchange_strong(unset, monotonicNow());
    receivedFrames_.fetch_add(1);

    std::vector<uint8_t> annexB;
    annexB.reserve(frame.size());
    for (const std::byte byte : frame) annexB.push_back(std::to_integer<uint8_t>(byte));
    const auto nals = splitAnnexB(annexB.data(), annexB.size());
    std::vector<uint8_t> sps;
    std::vector<uint8_t> pps;
    for (const auto &nal : nals) {
        if (nal.empty()) continue;
        const uint8_t type = nal[0] & 0x1F;
        if (type == 7) sps = nal;
        if (type == 8) pps = nal;
    }
    if (!sps.empty() && !pps.empty()) configureDecoderForFormat(sps, pps);

    std::vector<uint8_t> avcc;
    for (const auto &nal : nals) {
        if (nal.empty()) continue;
        const uint8_t type = nal[0] & 0x1F;
        if (type == 7 || type == 8) continue;
        const uint32_t length = static_cast<uint32_t>(nal.size());
        avcc.push_back(static_cast<uint8_t>((length >> 24) & 0xFF));
        avcc.push_back(static_cast<uint8_t>((length >> 16) & 0xFF));
        avcc.push_back(static_cast<uint8_t>((length >> 8) & 0xFF));
        avcc.push_back(static_cast<uint8_t>(length & 0xFF));
        avcc.insert(avcc.end(), nal.begin(), nal.end());
    }
    if (avcc.empty()) {
        decodeDropCount_.fetch_add(1);
        return;
    }

    std::lock_guard<std::mutex> lock(decoderMutex_);
    if (decoder_ == nullptr || decoderFormat_ == nullptr) {
        decodeDropCount_.fetch_add(1);
        return;
    }

    CMBlockBufferRef block = nullptr;
    OSStatus status = CMBlockBufferCreateWithMemoryBlock(
        kCFAllocatorDefault,
        nullptr,
        avcc.size(),
        kCFAllocatorDefault,
        nullptr,
        0,
        avcc.size(),
        0,
        &block
    );
    if (status == noErr) status = CMBlockBufferReplaceDataBytes(avcc.data(), block, 0, avcc.size());
    if (status != noErr || block == nullptr) {
        if (block != nullptr) CFRelease(block);
        decodeFailureCount_.fetch_add(1);
        return;
    }

    const double sourceSeconds = info.timestampSeconds
        ? info.timestampSeconds->count()
        : (receivedFrames_.load() - 1) / static_cast<double>(kTargetFrameRate);
    CMSampleTimingInfo timing = {
        CMTimeMake(1, kTargetFrameRate),
        CMTimeMakeWithSeconds(sourceSeconds, 90'000),
        kCMTimeInvalid,
    };
    CMSampleBufferRef sample = nullptr;
    status = CMSampleBufferCreateReady(
        kCFAllocatorDefault,
        block,
        decoderFormat_,
        1,
        0,
        &timing,
        0,
        nullptr,
        &sample
    );
    CFRelease(block);
    if (status != noErr || sample == nullptr) {
        decodeFailureCount_.fetch_add(1);
        return;
    }

    VTDecodeInfoFlags flags = 0;
    status = VTDecompressionSessionDecodeFrame(decoder_, sample,
                                               kVTDecodeFrame_EnableAsynchronousDecompression,
                                               nullptr, &flags);
    CFRelease(sample);
    if (status != noErr) decodeFailureCount_.fetch_add(1);
}

void NativeProbe::handleDecodedFrame(OSStatus status,
                                     CVImageBufferRef imageBuffer,
                                     CMTime presentationTimeStamp) {
    if (finished_.load()) return;
    if (status != noErr || imageBuffer == nullptr) {
        decodeFailureCount_.fetch_add(1);
        return;
    }
    uint64_t unset = 0;
    firstDecodedNanos_.compare_exchange_strong(unset, monotonicNow());
    decodedFrames_.fetch_add(1);

    CMVideoFormatDescriptionRef format = nullptr;
    if (CMVideoFormatDescriptionCreateForImageBuffer(kCFAllocatorDefault, imageBuffer, &format) != noErr ||
        format == nullptr) {
        decodeFailureCount_.fetch_add(1);
        return;
    }
    CMSampleTimingInfo timing = {
        CMTimeMake(1, kTargetFrameRate),
        CMTIME_IS_VALID(presentationTimeStamp) ? presentationTimeStamp : kCMTimeZero,
        kCMTimeInvalid,
    };
    CMSampleBufferRef sample = nullptr;
    const OSStatus sampleStatus = CMSampleBufferCreateReadyWithImageBuffer(
        kCFAllocatorDefault, imageBuffer, format, &timing, &sample);
    CFRelease(format);
    if (sampleStatus != noErr || sample == nullptr) {
        decodeFailureCount_.fetch_add(1);
        return;
    }

    CFRetain(sample);
    CFRelease(sample);
    dispatch_async(dispatch_get_main_queue(), ^{
        if (!this->finished_.load() && this->rootView_ != nil) {
            [this->rootView_.displayLayer enqueueSampleBuffer:sample];
            this->displayEnqueueCount_.fetch_add(1);
            this->observeReadyForDisplay();
        }
        CFRelease(sample);
    });
}

void NativeProbe::recordInput(const std::string &kind, bool trusted) {
    if (finished_.load()) return;
    InputSample sample;
    sample.sequence = nextInputSequence_++;
    sample.kind = kind;
    sample.trusted = trusted;
    sample.handlerNanos = monotonicNow();
    inputSamples_.push_back(sample);
    pendingInputs_.push_back(sample);
    if (trusted) {
        manualCount_ += 1;
    } else {
        scriptedCount_ += 1;
    }
    if (rootView_ != nil) {
        rootView_.inputTarget.inputSequence = static_cast<NSUInteger>(sample.sequence);
        rootView_.inputTarget.needsDisplay = YES;
    }
}

void NativeProbe::recordDisplayTick(uint64_t tickNanos) {
    if (finished_.load()) return;
    displayTickCount_ += 1;
    if (lastDisplayTickNanos_ != 0) {
        displayTickIntervalsMs_.push_back(elapsedMs(lastDisplayTickNanos_, tickNanos));
    }
    lastDisplayTickNanos_ = tickNanos;
    resolveInputSamples(tickNanos);
}

void NativeProbe::resolveInputSamples(uint64_t displayTickNanos) {
    const double tickMs = elapsedMs(startNanos_, displayTickNanos);
    std::vector<InputSample> remaining;
    remaining.reserve(pendingInputs_.size());
    for (auto &sample : pendingInputs_) {
        if (sample.handlerNanos <= displayTickNanos) {
            sample.acknowledged = true;
            sample.frameBoundaryMs = tickMs - elapsedMs(startNanos_, sample.handlerNanos);
            for (auto &original : inputSamples_) {
                if (original.sequence == sample.sequence) {
                    original = sample;
                    break;
                }
            }
        } else {
            remaining.push_back(sample);
        }
    }
    pendingInputs_ = std::move(remaining);
}

void NativeProbe::markInputTimeouts() {
    if (finished_.load()) return;
    const uint64_t now = monotonicNow();
    for (auto &sample : pendingInputs_) {
        if (elapsedMs(sample.handlerNanos, now) > kInputTimeoutMs) {
            sample.timedOut = true;
            for (auto &original : inputSamples_) {
                if (original.sequence == sample.sequence) {
                    original.timedOut = true;
                    break;
                }
            }
        }
    }
}

void NativeProbe::scheduleScriptedInputs() {
    __block int scheduled = 0;
    [NSTimer scheduledTimerWithTimeInterval:0.020 repeats:YES block:^(NSTimer *timer) {
        if (this->finished_.load() || scheduled >= 100) {
            [timer invalidate];
            return;
        }
        scheduled += 1;
        this->recordInput("scripted", false);
    }];
}

void NativeProbe::sampleResources() {
    const auto usage = currentTaskUsage();
    if (!usage) return;
    const uint64_t base = mediaStartNanos_ == 0 ? startNanos_ : mediaStartNanos_;
    resourceSamples_.push_back(ResourceSample{
        elapsedMs(base, monotonicNow()) / 1000.0,
        usage->cpuSeconds,
        usage->residentBytes,
    });
}

void NativeProbe::observeReadyForDisplay() {
    if (firstReadyForDisplayMs_ || rootView_ == nil) return;
    if (rootView_.displayLayer.isReadyForDisplay) {
        firstReadyForDisplayMs_ = elapsedMs(startNanos_, monotonicNow());
    }
}

void NativeProbe::requestAbort(const std::string &reason) {
    if ([NSThread isMainThread]) {
        finish("ABORTED", reason);
    } else {
        const std::string reasonCopy = reason;
        dispatch_async(dispatch_get_main_queue(), ^{
            this->finish("ABORTED", reasonCopy);
        });
    }
}

void NativeProbe::stopResources() {
    if (frameTimer_ != nil) [frameTimer_ invalidate];
    if (resourceTimer_ != nil) [resourceTimer_ invalidate];
    if (inputTimeoutTimer_ != nil) [inputTimeoutTimer_ invalidate];
    if (phaseTimer_ != nil) [phaseTimer_ invalidate];
    frameTimer_ = nil;
    resourceTimer_ = nil;
    inputTimeoutTimer_ = nil;
    phaseTimer_ = nil;
    if (timeoutBlock_ != nil) {
        dispatch_block_cancel(timeoutBlock_);
        timeoutBlock_ = nil;
    }
    if (displayLink_ != nullptr) {
        CVDisplayLinkStop(displayLink_);
        CVDisplayLinkRelease(displayLink_);
        displayLink_ = nullptr;
    }
    if (encoder_ != nullptr) {
        VTCompressionSessionCompleteFrames(encoder_, kCMTimeInvalid);
        VTCompressionSessionInvalidate(encoder_);
        CFRelease(encoder_);
        encoder_ = nullptr;
    }
    {
        std::lock_guard<std::mutex> lock(decoderMutex_);
        if (decoder_ != nullptr) {
            VTDecompressionSessionWaitForAsynchronousFrames(decoder_);
            VTDecompressionSessionInvalidate(decoder_);
            CFRelease(decoder_);
            decoder_ = nullptr;
        }
        if (decoderFormat_ != nullptr) {
            CFRelease(decoderFormat_);
            decoderFormat_ = nullptr;
        }
    }
    if (senderPeer_) senderPeer_->close();
    if (receiverPeer_) receiverPeer_->close();
    senderTrack_.reset();
    receiverTrack_.reset();
    senderPeer_.reset();
    receiverPeer_.reset();
}

void NativeProbe::finish(const std::string &status, const std::string &error) {
    bool alreadyFinished = finished_.exchange(true);
    if (alreadyFinished) return;

    for (auto &sample : pendingInputs_) {
        sample.missingAcknowledgement = true;
        sample.timedOut = true;
        for (auto &original : inputSamples_) {
            if (original.sequence == sample.sequence) {
                original = sample;
                break;
            }
        }
    }
    pendingInputs_.clear();
    if (!error.empty()) failureReason_ = error;
    stopResources();
    sampleResources();
    observeReadyForDisplay();
    writeReport(status, error);
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 250 * NSEC_PER_MSEC),
                   dispatch_get_main_queue(), ^{
        [NSApp terminate:nil];
    });
}

std::string NativeProbe::stateName(rtc::PeerConnection::State state) {
    switch (state) {
    case rtc::PeerConnection::State::New: return "new";
    case rtc::PeerConnection::State::Connecting: return "connecting";
    case rtc::PeerConnection::State::Connected: return "connected";
    case rtc::PeerConnection::State::Disconnected: return "disconnected";
    case rtc::PeerConnection::State::Failed: return "failed";
    case rtc::PeerConnection::State::Closed: return "closed";
    }
    return "unknown";
}

double NativeProbe::percentile(const std::vector<double> &values, double fraction) {
    if (values.empty()) return 0.0;
    std::vector<double> sorted = values;
    std::sort(sorted.begin(), sorted.end());
    const size_t index = std::min(sorted.size() - 1,
                                  static_cast<size_t>((sorted.size() - 1) * fraction));
    return sorted[index];
}

void NativeProbe::writeReport(const std::string &status, const std::string &error) {
    std::vector<std::string> rawInputSamples;
    rawInputSamples.reserve(inputSamples_.size());
    for (const auto &sample : inputSamples_) {
        rawInputSamples.push_back(jsonObject({
            {"sequence", jsonInt(sample.sequence)},
            {"kind", jsonString(sample.kind)},
            {"trusted", jsonBool(sample.trusted)},
            {"acknowledged", jsonBool(sample.acknowledged)},
            {"missingAcknowledgement", jsonBool(sample.missingAcknowledgement)},
            {"timedOut", jsonBool(sample.timedOut)},
            {"frameBoundaryMs", jsonNullable(sample.frameBoundaryMs)},
        }));
    }

    auto summarizeInputs = [&](bool trusted) {
        std::vector<double> values;
        int attempted = 0;
        int acknowledged = 0;
        int missing = 0;
        int timeout = 0;
        for (const auto &sample : inputSamples_) {
            if (sample.trusted != trusted) continue;
            attempted += 1;
            if (sample.acknowledged && sample.frameBoundaryMs) {
                acknowledged += 1;
                values.push_back(*sample.frameBoundaryMs);
            }
            if (sample.missingAcknowledgement) missing += 1;
            if (sample.timedOut) timeout += 1;
        }
        return jsonObject({
            {"attemptedCount", jsonInt(attempted)},
            {"acknowledgedCount", jsonInt(acknowledged)},
            {"missingAcknowledgementCount", jsonInt(missing)},
            {"timeoutCount", jsonInt(timeout)},
            {"p50Ms", values.empty() ? jsonNull() : jsonDouble(percentile(values, 0.50))},
            {"p95Ms", values.empty() ? jsonNull() : jsonDouble(percentile(values, 0.95))},
            {"maxMs", values.empty() ? jsonNull() : jsonDouble(*std::max_element(values.begin(), values.end()))},
        });
    };

    std::vector<std::string> resourceValues;
    resourceValues.reserve(resourceSamples_.size());
    for (const auto &sample : resourceSamples_) {
        resourceValues.push_back(jsonObject({
            {"elapsedSeconds", jsonDouble(sample.elapsedSeconds)},
            {"cpuSeconds", jsonDouble(sample.cpuSeconds)},
            {"residentBytes", jsonUInt(sample.residentBytes)},
        }));
    }

    std::vector<double> intervalValues = displayTickIntervalsMs_;
    const std::string intervalSummary = jsonObject({
        {"count", jsonInt(static_cast<int64_t>(intervalValues.size()))},
        {"p50Ms", intervalValues.empty() ? jsonNull() : jsonDouble(percentile(intervalValues, 0.50))},
        {"p95Ms", intervalValues.empty() ? jsonNull() : jsonDouble(percentile(intervalValues, 0.95))},
        {"maxMs", intervalValues.empty() ? jsonNull() : jsonDouble(*std::max_element(intervalValues.begin(), intervalValues.end()))},
    });

    auto relativeNanos = [&](uint64_t value) -> std::string {
        return value == 0 ? jsonNull() : jsonDouble(elapsedMs(startNanos_, value));
    };
    const int decodedFrames = decodedFrames_.load();
    const int displayFrames = displayEnqueueCount_.load();
    const std::string presentationGate = firstReadyForDisplayMs_ && decodedFrames > 0 && displayFrames > 0
        ? "PASS"
        : status == "COMPLETED" ? "FAIL" : "NOT_TESTED";
    const std::string inputGate = inputSamples_.empty()
        ? "NOT_TESTED"
        : std::any_of(inputSamples_.begin(), inputSamples_.end(), [](const InputSample &sample) {
              return sample.timedOut || sample.missingAcknowledgement;
          })
            ? "FAIL"
            : "PASS";

    std::string senderState;
    std::string receiverState;
    std::string offerSdp;
    std::string answerSdp;
    {
        std::lock_guard<std::mutex> lock(stateMutex_);
        senderState = senderState_;
        receiverState = receiverState_;
        offerSdp = offerSdp_;
        answerSdp = answerSdp_;
    }

    const std::string connectionObject = jsonObject({
        {"setupReadyMs", relativeNanos(setupReadyNanos_)},
        {"connectionReadyMs", relativeNanos(connectionReadyNanos_)},
        {"connectionGate", connectionReadyNanos_ != 0 ? jsonString("PASS") : jsonString("NOT_TESTED")},
        {"senderStateAtReport", jsonString(senderState)},
        {"receiverStateAtReport", jsonString(receiverState)},
        {"offerSdp", jsonString(offerSdp)},
        {"answerSdp", jsonString(answerSdp)},
    });

    const double runSeconds = options_.durationSeconds;
    const std::string report = jsonObject({
        {"experiment", jsonString("native-webrtc-h264")},
        {"status", jsonString(status)},
        {"durationSeconds", jsonDouble(options_.durationSeconds)},
        {"environmentRecord", jsonString(readText(options_.environment))},
        {"workload", jsonObject({
            {"width", jsonInt(kVideoWidth)},
            {"height", jsonInt(kVideoHeight)},
            {"targetFrameRate", jsonInt(kTargetFrameRate)},
            {"source", jsonString("deterministic moving BGRA CVPixelBuffer")},
            {"codec", jsonString("H264")},
            {"codecProfile", jsonString(negotiatedProfile_)},
            {"rtpPayloadType", jsonInt(kH264PayloadType)},
            {"rtpClockRate", jsonInt(rtc::H264RtpPacketizer::ClockRate)},
            {"transport", jsonString("libdatachannel PeerConnection with local ICE/DTLS/SRTP")},
        })},
        {"connection", connectionObject},
        {"codec", jsonObject({
            {"negotiatedCodec", jsonString(negotiatedCodec_)},
            {"senderEncoder", jsonString("VideoToolbox VTCompressionSession")},
            {"receiverDecoder", jsonString("VideoToolbox VTDecompressionSession")},
            {"hardwareAcceleration", jsonString("NOT_OBSERVABLE: VideoToolbox does not expose the selected hardware/software implementation through this probe")},
        })},
        {"frames", jsonObject({
            {"sourceGenerated", jsonInt(sourceFrames_.load())},
            {"encoded", jsonInt(encodedFrames_.load())},
            {"sentToWebRTC", jsonInt(sentFrames_.load())},
            {"rtpFramesReceived", jsonInt(receivedFrames_.load())},
            {"h264FramesDecoded", jsonInt(decodedFrames)},
            {"displayEnqueueCount", jsonInt(displayFrames)},
            {"encodeFailureCount", jsonInt(encodeFailureCount_.load())},
            {"decodeFailureCount", jsonInt(decodeFailureCount_.load())},
            {"decodeDropCount", jsonInt(decodeDropCount_.load())},
            {"decodedFramesPerSecond", jsonDouble(decodedFrames / std::max(0.001, runSeconds))},
            {"presentationGate", jsonString(presentationGate)},
            {"perFrameDisplayedFreshness", jsonString("NOT_OBSERVABLE: AVSampleBufferDisplayLayer does not expose displayed sample-buffer identity")},
        })},
        {"timing", jsonObject({
            {"firstEncodedMs", relativeNanos(firstEncodedNanos_.load())},
            {"firstRtpFrameReceivedMs", relativeNanos(firstReceivedNanos_.load())},
            {"firstDecodedFrameMs", relativeNanos(firstDecodedNanos_.load())},
            {"firstReadyForDisplayMs", firstReadyForDisplayMs_ ? jsonDouble(*firstReadyForDisplayMs_) : jsonNull()},
        })},
        {"display", jsonObject({
            {"readyForDisplay", rootView_ != nil ? jsonBool(rootView_.displayLayer.isReadyForDisplay) : jsonBool(false)},
            {"displayTickCount", jsonInt(displayTickCount_)},
            {"displayTickIntervalsMs", intervalSummary},
        })},
        {"input", jsonObject({
            {"metricVersion", jsonInt(1)},
            {"ackTimeoutMs", jsonDouble(kInputTimeoutMs)},
            {"scriptedCount", jsonInt(scriptedCount_)},
            {"manualCount", jsonInt(manualCount_)},
            {"manualInputGate", manualCount_ > 0 ? jsonString("OBSERVED") : jsonString("NOT_TESTED")},
            {"frameBoundaryGate", jsonString(inputGate)},
            {"scripted", summarizeInputs(false)},
            {"manual", summarizeInputs(true)},
            {"samples", jsonArray(rawInputSamples)},
            {"definition", jsonString("Elapsed from the native event handler to the next CVDisplayLink tick after the target state update; a display-boundary proxy, not glass-to-glass latency.")},
        })},
        {"processSamples", jsonArray(resourceValues)},
        {"resourceMeasurementLimits", jsonString("processSamples cover this native probe. VideoToolbox, WindowServer, GPU-driver, and peer attribution are not a complete whole-application resource budget.")},
        {"processAttribution", jsonString("NOT_COMPLETE: only the native probe task is sampled; decoder and display service costs are not separately attributed")},
        {"statusError", error.empty() ? jsonNull() : jsonString(error)},
    });

    const std::filesystem::path outputPath(options_.output);
    std::error_code filesystemError;
    if (std::filesystem::exists(outputPath, filesystemError)) {
        std::fprintf(stderr, "ERROR: refusing to overwrite existing report: %s\n", options_.output.c_str());
        return;
    }
    std::filesystem::create_directories(outputPath.parent_path(), filesystemError);
    std::ofstream output(options_.output, std::ios::out | std::ios::trunc);
    if (!output) {
        std::fprintf(stderr, "ERROR: could not open report path: %s\n", options_.output.c_str());
        return;
    }
    output << report << "\n";
    output.close();
    std::printf("Probe report: %s\n", options_.output.c_str());
}

} // namespace
