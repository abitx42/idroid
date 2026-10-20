# Native WebRTC/H.264 feasibility experiment

**Status:** minimal native WebRTC/H.264 loopback implemented; three 30-second
trials completed; an Android-facing receiver extension builds and has negative
signaling validation only. This remains a feasibility experiment, not a product
route.

This is a bounded macOS feasibility experiment. It is not the iDroid
application, Android capture, production pairing, a network interoperability
test, or a final architecture decision.

## Hypothesis

The hypothesis is:

> A native macOS WebRTC peer connection carrying an H.264 video track can
> receive, decode, and present the same deterministic 640×360/30-fps class of
> workload without the frame-rate stalls observed in the WKWebView/WebRTC
> loopback.

The native route is supported only if the experiment demonstrates a real
WebRTC session, a negotiated H.264 video track, native decode, and observable
presentation for at least three separate 30-second trials. A successful native
uncompressed presentation probe is not evidence for this hypothesis.

## Library selection

Two candidates were considered before implementation:

| Candidate | Strengths | Costs / reason not selected for this probe |
| --- | --- | --- |
| Google WebRTC (`webrtc.googlesource.com/src`) | Canonical native `PeerConnection` stack; BSD-3-Clause; mature macOS/iOS support; integrated codecs and WebRTC metrics. | The official checkout uses Chromium's `depot_tools`, GN, Ninja, and a very large source/dependency tree. The upstream development documentation estimates about 5.6 GB for a Mac checkout with iOS support. This is disproportionate for a bounded comparison and none of the required depot tools are present here. |
| **libdatachannel v0.24.6** | Active C++ WebRTC media-transport implementation; MPL-2.0; advertises macOS/iOS support; implements SDP, ICE, DTLS/SRTP, RTP media transport, and H.264 RTP packetization/depacketization. It is small enough to build with the available Apple Clang toolchain. | It is not a codec implementation. H.264 encode/decode and native presentation must be supplied by VideoToolbox. Its MPL-2.0 and dependency licenses require notice/source-compliance review before product distribution. |

The selected library is **libdatachannel v0.24.6**, pinned to commit
`6b1e2e620f1e37f0eafeee702eaea0043cb305fd`. It is selected for the experiment
because it supplies the real WebRTC session and H.264 RTP path while keeping
the native codec boundary explicit: VideoToolbox will be used for H.264
encoding/decoding, not treated as a hidden mock or inferred from CPU usage.

Upstream references:

- <https://github.com/paullouisageneau/libdatachannel>
- <https://github.com/paullouisageneau/libdatachannel/releases/tag/v0.24.6>
- <https://webrtc.googlesource.com/src/+/refs/heads/main/README.chromium>
- <https://webrtc.googlesource.com/src/+/refs/heads/main/docs/native-code/development/README.md>

## Feasibility and dependencies

The available host is macOS `26.6.2` / build `25G83`, arm64, with Command
Line Tools SDK `27.0`, Apple Clang `21.0.0.21000334`, Swift `6.4`, and GNU Make.
The host has no global CMake, Ninja, pkg-config, or preinstalled libdatachannel.

The selected library was nevertheless configured and built successfully in a
project-local temporary environment using:

- CMake `3.31.10`, installed only into a temporary project-local tool folder;
- Unix Makefiles rather than the unavailable Ninja generator;
- Homebrew OpenSSL `3.6.4` at `/opt/homebrew/opt/openssl@3`;
- libdatachannel's pinned submodules for libjuice, libsrtp, usrsctp, plog, and
  nlohmann/json; and
- Apple Clang targeting arm64.

The experiment build keeps its dependency checkout, CMake bootstrap, build
outputs, environment record, and reports under this experiment's `.build/` and
`results/` directories. It does not install dependencies globally.

The native implementation must use:

- libdatachannel for the PeerConnection, ICE, DTLS/SRTP, SDP, and H.264 RTP
  packetization/depacketization;
- VideoToolbox `VTCompressionSession` for the deterministic sender's H.264
  encoding;
- VideoToolbox `VTDecompressionSession` for the receiver's H.264 decoding; and
- AppKit plus `AVSampleBufferDisplayLayer` for visible native presentation.

The library build result proved dependency feasibility. The receiver and full
measurement path were then exercised in a local native WebRTC loopback; the
trial results are recorded in [`RESULTS.md`](RESULTS.md).

## Smallest experiment

The experiment uses two native libdatachannel peers in one macOS process. The
sender generates deterministic 640×360 BGRA frames, encodes them through
VideoToolbox H.264, and sends the encoded frames through a real local
libdatachannel WebRTC session. The receiver depacketizes H.264, decodes it
through VideoToolbox, and presents decoded pixel buffers in a visible AppKit
window.

This keeps the session local and deterministic while still exercising ICE,
DTLS/SRTP, SDP negotiation, RTP H.264 packetization, H.264 encoding/decoding,
and native presentation. It deliberately does not implement Android capture,
signaling over a real network, pairing, or product lifecycle behavior.

## Build and run

The experiment is macOS-only and requires a logged-in WindowServer session.
The first build downloads the pinned libdatachannel source and submodules into
`.build/deps/`, bootstraps CMake `3.31.10` into `.build/tools/`, and uses the
local Homebrew OpenSSL 3 prefix. No dependency is installed globally by the
experiment.

From the repository root:

```sh
bash experiments/native-webrtc-h264/build.sh
bash experiments/native-webrtc-h264/run.sh \
  --duration-seconds 30 \
  --output experiments/native-webrtc-h264/results/native-trial-1.json
```

Use a new output path for every run. The probe refuses to overwrite an existing
report. The native window's input target records trusted AppKit events as
manual samples; without human interaction, `manualInputGate` remains
`NOT_TESTED`.

### Android remote receiver extension

The same native target also builds an experimental Android-facing receiver at
`.build/native/android-webrtc-receiver`. It listens for one bounded TCP
signaling connection on an explicitly selected RFC1918 private IPv4 address,
validates the HMAC pairing envelope, waits for complete ICE gathering, receives
H.264 through libdatachannel, decodes with VideoToolbox, and presents the result
through `AVSampleBufferDisplayLayer`.

This is intentionally not the product security model: signaling is plaintext
TCP protected only by the experimental pairing-code HMAC, with no platform key
storage, replay protection, authorization lifecycle, or DTLS-identity binding.
Binding to one private address prevents wildcard exposure but does not isolate
the listener from other devices on that same network. Do not expose it beyond a
controlled test network.

Build and inspect the command surface with:

```sh
bash build.sh
.build/native/android-webrtc-receiver --help
```

For a physical-device run, use `run-android-receiver.sh` and pass a unique
`--output` path, an explicitly selected Mac Wi-Fi address, and a test-network
port such as `18765`:

```sh
bash run-android-receiver.sh \
  --bind-address "$MAC_WIFI_IP" \
  --pairing-code-stdin \
  --port 18765 \
  --duration-seconds 30 \
  --output "$RUN_DIR/trial-$TRIAL.json"
```

The receiver refuses wildcard, loopback, non-IPv4, and non-RFC1918 bind
addresses and requires the address to be assigned to a local interface. Check
that the selected port is free before starting it; the script never terminates
an unrelated listener. Signaling has a 10-second TCP accept deadline by
default, a separate fixed 10-second offer-read deadline after accept, and a
262,144-byte line bound. The accept deadline can be increased for diagnostics
with `--accept-timeout-seconds 45` (the accepted range is `(0, 3600]`). The
receiver report includes the source
environment record, selected bind address/interface and port, libdatachannel
revision, signaling/connection state, H.264 decode counters, presentation
counters, and explicit Android-side `NOT_TESTED` limits. No physical Android
run has been completed in this checkout.

Use the complete [first-device procedure](../android-macos-vertical-slice/FIRST-DEVICE-TEST.md)
for the real phone session. Prefer `--pairing-code-stdin` to keep the local code
out of arguments; enter it directly without echo, never in an environment
variable, saved command or file. Never persist a key or full SDP. Native reports now include
fixed `outcome.stage/code` classifications, TCP/authentication/SDP/answer stage
flags, ICE state, and decoder-setup counts. Peer state is captured before our
own cleanup closes it. A connected trial with no decoded/enqueued frame reports
a first-frame timeout or decoder failure instead of `COMPLETED`. Window close
is routed through the delegate and reports `USER_STOPPED`; actual physical/GUI
shutdown behavior still needs manual execution.

The focused local negative test requires a local RFC1918 address and exercises
the help surface, required/private binding, port-preservation check, invalid
pairing code, malformed and oversized signaling, bad authentication, timeout
cleanup, and report/log pairing-material non-disclosure:

```sh
IDROID_RECEIVER_TEST_BIND_ADDRESS=<LOCAL_RFC1918_IPV4> \
  IDROID_RECEIVER_TEST_PORT=<PRIVATE_TEST_PORT> \
  bash test-android-receiver.sh
```

The focused test includes a 45-second accept-timeout diagnostic case after the
default timeout checks; use a free private test port so it cannot disturb an
unrelated listener.

## Directly comparable measurements

- 640×360 frame dimensions, target 30 fps, deterministic motion, and 30-second
  duration;
- native setup, connection, first encoded frame, first received frame, first
  decoded frame, and first-ready-for-display times;
- negotiated codec/profile/packetization parameters;
- encoded/sent, RTP-received, H.264-decoded, displayed/enqueued, and dropped
  frame counts where the respective API exposes them;
- observed decoded/presented rate and freshness proxies;
- scripted native event-handler-to-display-boundary samples, with trusted
  manual input kept separate; and
- native process CPU/RSS plus best-effort process attribution.

## Measurements that are not equivalent

- The WKWebView route uses the browser's WebRTC implementation, canvas capture,
  browser H.264 encoder/decoder selection, and HTML video callbacks. The native
  route uses libdatachannel and VideoToolbox. Codec implementation and RTP
  scheduling are not interchangeable.
- `requestVideoFrameCallback`, VideoToolbox decode callbacks, and
  `AVSampleBufferDisplayLayer.isReadyForDisplay` are different boundaries.
  They must not be reported as identical glass-to-glass timestamps.
- Per-frame displayed identity is not exposed by `AVSampleBufferDisplayLayer`.
  A decoded or enqueued count is not a displayed-frame freshness proof.
- Low CPU usage is not evidence of hardware decoding. Decoder implementation
  and hardware acceleration are reported as `OBSERVED` only when an API field
  explicitly exposes them; otherwise they remain `NOT_OBSERVABLE`.
- A local in-process WebRTC session does not establish Android transport,
  physical-network behavior, pairing security, recovery, or production
  suitability.

## Acceptance states

Every report uses `PASS`, `FAIL`, `NOT_TESTED`, or `TIMEOUT` for each observable
gate. Failed and timed-out runs are preserved at unique report paths; they are
never averaged away.

The experiment cannot change HG-1 through HG-5 in `docs/DECISION-GATES.md`.
In particular, a passing native WebRTC loopback does not satisfy the physical
Android/macOS vertical slice, 240-fps glass-to-glass measurement, production
security boundary, or lifecycle-recovery gates.
