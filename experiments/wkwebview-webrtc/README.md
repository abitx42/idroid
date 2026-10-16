# Kill Switch 1: WKWebView WebRTC feasibility

**Status:** reproducible experiment harness; no result is implied until the
macOS run completes and its JSON artifact is reviewed.

This is the smallest native macOS route test for the current product question:
can a `WKWebView` host a real WebRTC video path and expose enough observability
to decide whether the route is worth taking further? It is deliberately not an
application shell, Android capture pipeline, Finder extension, virtual camera,
or network interoperability test.

## Question and route under test

The harness builds a native AppKit process containing a real `WKWebView`. The
page creates a moving canvas video source, sends it through a local
`RTCPeerConnection` loopback, attaches the inbound remote `MediaStream` to an
HTML `<video>` element, and measures the video callbacks and WebRTC inbound
stats. The canvas is only the deterministic source; the observed playback is a
real WebRTC media track and not a timer or a mock video element.

The loopback isolates the WKWebView route. It does **not** prove Android
capture, a real network, macOS screen capture, synthetic input injection,
production codecs, or cross-device latency.

## Acceptance criteria — define before comparing results

The following criteria are frozen for this experiment. They are functional
evidence gates, not a claim that the product’s final performance target has
passed.

| Gate | Required evidence | Result interpretation |
| --- | --- | --- |
| Native route | The AppKit process creates a `WKWebView`, loads the local page, and writes a report with OS, architecture, SDK, Swift compiler, and source revision. | Missing report or failed load is **FAIL** for the harness. A successful compile alone is not a route result. |
| Real video playback | `RTCPeerConnection` negotiation succeeds, a remote video track arrives, `HTMLVideoElement.readyState` reaches at least 3, `play()` resolves, and at least 100 remote video-frame callbacks are observed. | **PASS** only for this local loopback path. No remote track or insufficient frames is **FAIL**. |
| Frame freshness | `requestVideoFrameCallback` is available; `presentedFrames` is monotonic; at least 99% of callbacks advance to a new presented frame; inbound `framesDecoded` increases. | **PASS** for observed callback freshness. This does not establish glass-to-glass latency. |
| Hardware decoding | Record `decoderImplementation`, `powerEfficient`, codec, decode time, and any other fields exposed by current WKWebView WebRTC stats. | **OBSERVED** only when a browser/API field explicitly reports it. Otherwise **NOT OBSERVABLE**; CPU behavior must not be used as proof. |
| Input response | The page records scripted pointer response samples and accepts manual pointer/keyboard samples while the window is visible. Every recorded event must produce a render response. | **PASS** means the route records finite responses and no event is lost. Report p50/p95; do not call an arbitrary latency target passed. |
| CPU and memory | The native process samples resident memory and task CPU at least once per second during the run, with platform/toolchain metadata. | **PASS** means complete, finite samples exist. Suitability requires a later decision against a declared budget. |
| macOS-specific gates | Physical WebRTC playback, hardware behavior, frame freshness, input, CPU, and memory are run on macOS with the native process. | If the process cannot execute, every macOS behavior gate is **NOT TESTED**, regardless of Linux or compile results. |

The experiment intentionally does not freeze a product p95 target. The current
decision gates require the target to be frozen at G0 after baseline rehearsal;
this harness records the distributions needed for that decision. No benchmark
number is considered a pass until the report, environment, workload, and raw
run are retained together.

## Files and dependencies

```text
experiments/wkwebview-webrtc/
├── README.md
├── .gitignore
├── build.sh
├── run.sh
├── record-environment.sh
├── Sources/main.swift
└── Web/index.html
```

The harness uses only macOS system frameworks: AppKit, WebKit, Foundation, and
the Mach task API. It has no Swift Package dependencies, npm dependencies,
server, camera permission, microphone permission, or network service. The
WebRTC loopback uses `canvas.captureStream(30)` so the first run is repeatable
without an external media file or peer.

## Run on macOS

From the repository root:

```sh
cd experiments/wkwebview-webrtc
bash build.sh
bash run.sh --duration-seconds 30 --output results/wkwebview-loopback.json
```

`build.sh` uses `xcrun` to locate the active macOS SDK and `swiftc`, compiles the
native shell, and records the environment in `.build/environment.txt`.
`run.sh` builds first, opens a visible window, runs the WebRTC loopback for the
requested duration, samples the native process, and exits after writing the
JSON report. Click the **Input target** during the run if manual input samples
are desired; scripted samples are generated automatically.

The run requires a logged-in macOS user session with a WindowServer. The
Command Line Tools SDK is sufficient to compile the harness, but a compile is
not a substitute for executing the windowed WebKit route. A machine without a
usable macOS GUI must mark the macOS-specific gates **NOT TESTED**.

## Report contents

The JSON artifact contains:

- exact run status and errors, duration, source revision, and captured
  platform/toolchain text;
- WebKit user agent and feature availability;
- WebRTC negotiation and remote-track status;
- first-frame time, callback count, presented-frame repeats/non-monotonic
  observations, source frame count, and inbound RTP stats;
- codec, decoder implementation, power-efficiency, decode-time, and other
  hardware-related fields when WebKit exposes them;
- scripted and manual input samples with p50/p95/max summaries; and
- native resident-memory and CPU samples.

The report is evidence for this exact loopback workload. It must not be
generalized to Android, a real network, a production video source, a full
screen mirror, or a final iDroid performance score.

## Interpretation and known limits

- `WKWebView` does not provide a universal public “hardware decoder used” flag
  for WebRTC. A missing `decoderImplementation`/`powerEfficient` field is an
  observation gap, not evidence of software decoding or hardware decoding.
- Canvas capture exercises a real WebRTC encode/decode and HTML video path, but
  it is not camera capture and does not validate H.264/VP8 behavior for every
  production source or OS build.
- Scripted DOM input is not macOS Accessibility or `CGEvent` input injection.
  Manual samples only prove visible WebView event response in this harness.
- Native CPU and memory samples cover this process. They do not cover the OS,
  GPU driver, browser GPU process, Android peer, or a future transport.
- No 240-fps camera, glass-to-glass measurement, battery/thermal soak, or
  physical Android/macOS vertical slice is performed here.

## Evidence status

Do not fill in results by inference. A result is complete only when the command
was actually run and the JSON artifact exists. If macOS execution is
unavailable, record:

```text
WKWebView WebRTC macOS gates: NOT TESTED
Reason: <exact command/tool/environment error>
```

The recommended next action after a successful loopback is to compare the
observed codec/frame freshness/resource data with the frozen G0 criteria, then
run the same measurement against the surviving native architecture before any
full application work is started.
