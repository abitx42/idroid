# Native AppKit video-presentation comparison

**Status:** minimal native presentation baseline; not a WebRTC implementation.

This experiment compares a native macOS presentation layer with the existing
WKWebView/WebRTC loopback without claiming that it is a native WebRTC stack.
It creates deterministic **uncompressed BGRA CVPixelBuffers**, wraps them in
CMSampleBuffers, and presents them through `AVSampleBufferDisplayLayer` in a
visible AppKit window.

That is a real native video-presentation path, but it intentionally does not
decode H.264. The result isolates native sample-buffer presentation, scheduling,
window/input handling, and process resource behavior. It cannot establish that
native WebRTC, VideoToolbox H.264 decode, or a future Android-to-Mac transport
would outperform WKWebView.

## Workload and comparability

| Property | Native probe | WKWebView baseline |
| --- | --- | --- |
| Duration | 30 seconds | 30 seconds |
| Frame dimensions | 640×360 | 640×360 canvas source / remote video |
| Target rate | 30 fps | `canvas.captureStream(30)` |
| Source | Deterministic moving BGRA pixel buffers | Deterministic moving canvas frames sent through local WebRTC loopback |
| Presentation | `AVSampleBufferDisplayLayer` | HTML `<video>` in `WKWebView` |
| Decode | **None**: uncompressed pixel buffers | WebRTC H.264 decode observed in inbound RTP stats |
| First-frame metric | `AVSampleBufferDisplayLayer.isReadyForDisplay` notification/poll | `requestVideoFrameCallback` first callback |
| Frame freshness | Per-frame displayed identity is **NOT OBSERVABLE** through this API; enqueue/back-pressure is recorded | `presentedFrames` and inbound decoded frames |
| Input metric | Native event handler to next `CVDisplayLink` tick | Event handler to next `requestAnimationFrame` after target update |
| Resources | Native process plus best-effort baseline-delta process snapshots | Native process plus best-effort baseline-delta WebKit snapshots |

The first-frame signals are not identical: `isReadyForDisplay` means Apple’s
layer has made the first frame ready for display, while the WebView metric is a
browser video-frame callback. They are useful directional startup observations,
not interchangeable glass-to-glass measurements.

## Acceptance states

The JSON report uses these states:

- **PASS:** the requested observable condition was met for this probe;
- **FAIL:** the probe ran but an observable functional condition failed;
- **NOT_TESTED:** the API or environment could not provide the requested
  evidence; and
- **TIMEOUT:** the native process did not complete before its bounded timeout.

Native presentation gates:

| Gate | Evidence | Limitation |
| --- | --- | --- |
| Native window | AppKit window and `AVSampleBufferDisplayLayer` are visible. | Does not prove a product window/menu-bar shell. |
| First presentation | `isReadyForDisplay` becomes true after a frame is enqueued. | This is “ready for display”, not a camera/glass-to-glass timestamp. |
| Enqueue progress | Generated, enqueued, back-pressured, and failed frame counts are recorded. | The layer API does not expose the identity of every displayed sample buffer. |
| Frame freshness | **NOT_OBSERVABLE** per displayed frame; the report records display-link ticks, enqueue progress, and back-pressure. | Do not infer presented-frame freshness from generated frames. |
| Input | Native mouse/key handler to next display-link tick, with 1-second acknowledgement timeout. | This is a frame-boundary proxy, not physical input-to-pixels latency. |
| Resources | Native CPU/RSS at least once per second and best-effort process snapshots. | Not complete whole-application attribution. |
| Hardware decode | **NOT_TESTED / NOT_APPLICABLE** for this uncompressed baseline. | No H.264 decode is performed. |

## Build and run

From the repository root on the available macOS Command Line Tools SDK:

```sh
cd experiments/native-video-presentation
bash build.sh
bash run.sh --duration-seconds 30 --output results/native-trial-1.json
```

Use a new output path for every run. Do not overwrite an earlier report. For a
human input run, the same visible window can be used; click the **Input target**
AppKit mouse/key events are recorded as manual samples. If no person interacts,
`manualInputGate` remains **NOT_TESTED**.

The native probe has no Swift Package, npm, Rust, server, camera, microphone,
or network dependency. It links only system frameworks.

## Measurement limits

`AVSampleBufferDisplayLayer` documents that clients cannot determine which
sample buffers have been decoded/displayed. This probe therefore reports the
first `isReadyForDisplay` observation and enqueue/back-pressure progress, but
does not invent a per-frame presented count. A native follow-up that needs
per-frame output would need a different observable path, such as a native
player/video-output pipeline or an external high-speed camera.

The native probe uses uncompressed pixel buffers. Low CPU, a stable enqueue rate,
or a successful first presentation must not be described as hardware decoding.

## Recommended comparison use

Run at least three separate 30-second trials and compare:

- startup/setup and first-ready timing;
- generated/enqueued/back-pressured frames;
- display-link and input frame-boundary behavior;
- native CPU/RSS and process-snapshot limitations; and
- completion/failure/timeout rate.

If this native presentation baseline is stable while WKWebView/WebRTC is not,
that narrows the investigation to WebRTC/WebKit or its environment. It does not
select a native WebRTC architecture. A fair native WebRTC comparison remains a
separate experiment.
