# Native WebRTC/H.264 feasibility results

## Scope and revision

This report covers the minimal native macOS loopback under
`experiments/native-webrtc-h264/`. It does not implement Android capture,
production pairing, real-network signaling, or the full iDroid application.

The repository was inspected at:

- `HEAD`: `9d1dec7c8ca384e2f6a642902eda1049b94ed487`
- native uncompressed presentation probe commit: `9d1dec7`
- the existing WKWebView changes remained uncommitted and were preserved:
  - `experiments/wkwebview-webrtc/README.md`
  - `experiments/wkwebview-webrtc/Sources/main.swift`
  - `experiments/wkwebview-webrtc/Web/index.html`

The native WebRTC/H.264 experiment itself was intentionally not committed, per
the task instruction.

## Hypothesis

A native macOS WebRTC peer connection carrying H.264 can receive, decode, and
present deterministic 640×360/30-fps video without the substantial frame-rate
variability observed in the WKWebView/WebRTC loopback.

## Dependency decision

The selected library is libdatachannel `v0.24.6`, pinned to
`6b1e2e620f1e37f0eafeee702eaea0043cb305fd`.

It was selected over the canonical Google WebRTC source because the latter
requires the Chromium/depot_tools/GN/Ninja build system and a multi-gigabyte
checkout for a bounded feasibility probe. libdatachannel supplies the real
PeerConnection, SDP, ICE, DTLS/SRTP, and H.264 RTP transport while leaving the
codec boundary explicit. VideoToolbox supplies H.264 encode/decode.

Observed licensing:

- libdatachannel: MPL-2.0;
- libjuice: MPL-2.0;
- libsrtp: BSD-style license;
- usrsctp: BSD-style license;
- plog: MIT;
- nlohmann/json: MIT.

This is not a product legal review. A product distribution would need to carry
the applicable notices and source-compliance material for libdatachannel and
all bundled dependencies, plus review VideoToolbox/Apple distribution rules.

## Environment and feasibility

The build was executed on:

- macOS `26.6.2`, build `25G83`, arm64;
- Command Line Tools SDK `27.0`;
- Apple Clang `21.0.0.21000334`;
- CMake `3.31.10`, bootstrapped into the experiment's `.build/tools/`;
- Homebrew OpenSSL `3.6.4` at `/opt/homebrew/opt/openssl@3`; and
- GNU Make using CMake's `Unix Makefiles` generator.

The libdatachannel static library and its pinned submodules built successfully
for arm64. The environment record is:

`experiments/native-webrtc-h264/.build/environment.txt`

## Commands executed

Dependency/build feasibility and probe build:

```sh
bash experiments/native-webrtc-h264/build.sh
```

Functional smoke run:

```sh
bash experiments/native-webrtc-h264/run.sh --duration-seconds 5 \
  --output experiments/native-webrtc-h264/results/smoke-5s-v3.json
```

Three required 30-second trials (final implementation revision):

```sh
bash experiments/native-webrtc-h264/run.sh --duration-seconds 30 \
  --output experiments/native-webrtc-h264/results/native-trial-final-1.json
bash experiments/native-webrtc-h264/run.sh --duration-seconds 30 \
  --output experiments/native-webrtc-h264/results/native-trial-final-2.json
bash experiments/native-webrtc-h264/run.sh --duration-seconds 30 \
  --output experiments/native-webrtc-h264/results/native-trial-final-3.json
```

The earlier failed timeout is preserved at
`experiments/native-webrtc-h264/results/smoke-5s.json`; it was caused by an
incorrect answer-negotiation call and was not discarded or overwritten. The
corrected smoke run is `smoke-5s-v3.json`.

## Android-facing receiver extension validation

The remote receiver target was added after the three local trials. Its current
validation is limited to build and negative signaling behavior:

```sh
bash experiments/native-webrtc-h264/build.sh
experiments/native-webrtc-h264/.build/native/android-webrtc-receiver --help
```

Both the existing `native-webrtc-h264` target and the new
`android-webrtc-receiver` target built successfully. A malformed signaling
connection was also exercised and preserved at:

`experiments/native-webrtc-h264/results/android-remote-malformed-validation-v2.json`

That report has `status: FAIL`, `statusError: malformed signaling offer`, and
`connection.connectionGate: NOT_TESTED`, as required for a negative test. A
five-second native loopback regression after adding the remote target is
preserved at:

`experiments/native-webrtc-h264/results/native-regression-after-remote.json`

After the bind, framing, and timeout hardening, a further five-second regression
is preserved at:

`experiments/native-webrtc-h264/results/native-regression-after-security-audit.json`

No physical Android device was connected, so the remote receiver has no Android
offer, real Wi-Fi session, H.264 interoperability, lifecycle, or resource
evidence yet.

## Binding and negative-signaling validation

The receiver no longer accepts a wildcard bind. It requires an explicit RFC1918
private IPv4 `--bind-address`, rejects loopback and non-private addresses, and
records only the selected address and port in the report. The pairing code is
not included in reports or receiver output.

The focused validation command is:

```sh
IDROID_RECEIVER_TEST_BIND_ADDRESS=<LOCAL_RFC1918_IPV4> \
  bash experiments/native-webrtc-h264/test-android-receiver.sh
```

The latest run passed the following cases without terminating the unrelated
listener on the default test port `18765`:

- required/private bind address and invalid pairing-code rejection;
- help output documenting `--bind-address PRIVATE_IPV4`;
- malformed signaling and partial TCP reads;
- maximum-size boundary and oversized signaling;
- malformed SDP, invalid session fields, and invalid authentication;
- peer disconnect during signaling and second-client rejection;
- idle-client and no-client timeout cleanup; and
- pairing-material absence from reports and captured receiver output.

The negative reports are unique, ignored artifacts under
`experiments/native-webrtc-h264/results/`. This validates the local receiver
boundary only; it is not Android or Wi-Fi interoperability evidence.

## Native WebRTC/H.264 trial results

All three required trials completed with a real local libdatachannel
PeerConnection, negotiated H.264, VideoToolbox encoding, H.264 RTP transport,
VideoToolbox decoding, and native display-layer enqueue.

| Trial | Connection ready | First encoded | First RTP received | First decoded | First ready | Decoded FPS | Frames source/encoded/received/decoded | Encode/decode/drop failures | Scripted input p50/p95 | Manual gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- | --- | --- |
| `native-trial-final-1.json` | 89.7 ms | 147.9 ms | 148.2 ms | 203.8 ms | 237.3 ms | 30.03 | 901 / 901 / 901 / 901 | 0 / 0 / 0 | 9.58 / 15.99 ms | `NOT_TESTED` |
| `native-trial-final-2.json` | 62.3 ms | 119.6 ms | 119.9 ms | 171.1 ms | 175.3 ms | 30.03 | 901 / 901 / 901 / 901 | 0 / 0 / 0 | 9.79 / 16.58 ms | `OBSERVED` |
| `native-trial-final-3.json` | 61.5 ms | 120.8 ms | 121.3 ms | 253.7 ms | 276.2 ms | 30.03 | 901 / 901 / 901 / 901 | 0 / 0 / 0 | 8.05 / 14.90 ms | `NOT_TESTED` |

All three reports had:

- `status: COMPLETED`;
- `connectionGate: PASS`;
- `frames.presentationGate: PASS`;
- `input.frameBoundaryGate: PASS`;
- `h264FramesDecoded == displayEnqueueCount == 901`; and
- no encode failures, decode failures, or decode drops.

Trial 2 contains trusted AppKit input samples and is reported exactly as
observed. No separate deliberate human-only run was performed for this phase.

## Comparison with the existing WKWebView evidence

The native H.264 route is materially stronger evidence than the earlier native
uncompressed probe because it exercises real H.264 encoding, libdatachannel
WebRTC negotiation/transport, H.264 RTP depacketization, and native decode.

The comparison is still directional rather than equivalent:

- Native uses libdatachannel + VideoToolbox and reports decoded frames;
- WKWebView uses the browser WebRTC stack, canvas capture, browser H.264
  negotiation, and `requestVideoFrameCallback`;
- native reports H.264 profile `42e01f`, while the controlled WKWebView reports
  an inbound H.264 codec profile represented by its browser codec ID;
- native display freshness remains `NOT_OBSERVABLE` because
  `AVSampleBufferDisplayLayer` does not expose displayed sample identity; and
- both input metrics are display/frame-boundary proxies, not glass-to-glass
  latency.

The controlled WKWebView reports remain available locally at:

- `experiments/wkwebview-webrtc/results/controlled-v2-1.json`
- `experiments/wkwebview-webrtc/results/controlled-v2-2.json`
- `experiments/wkwebview-webrtc/results/controlled-v2-3.json`

Those reports recorded observed rates of approximately 2.75, 28.45, and 6.51
FPS, respectively. The native 30.03 FPS result must not be treated as proof
that a production native WebRTC route will always be faster: the stacks,
encoders, presentation callbacks, and process attribution differ.

WKWebView hardware-decoder fields remained `NOT_OBSERVABLE` in the inspected
controlled reports. The native probe likewise reports VideoToolbox hardware
selection as `NOT_OBSERVABLE`; low CPU usage is not used as a substitute.

## Decision-gate status

This experiment provides useful evidence for the maintenance/native-route
category, but it does not satisfy the product hard gates:

- HG-1: existing Linux core build gate remains separate and unchanged;
- HG-2: **RED / not tested** — no physical Android/macOS vertical slice;
- HG-3: **RED / not tested** — no 240-fps camera or glass-to-glass measurement;
- HG-4: **RED / not tested** — no production pairing, authorization, or
  cryptographic transport boundary;
- HG-5: **RED / not tested** — no 100-cycle real transport/lifecycle recovery
  run.

The native WebRTC/H.264 feasibility gate for this exact local experiment is
`PASS` across three trials. The product decision remains **conditional at most**
and cannot be a general-support or release decision under
`docs/DECISION-GATES.md`.

## Recommendation

Do not build the full application yet. Keep the native WebRTC/H.264 route as a
credible candidate for the next experiment, but the next decision should be a
matched sender/receiver or physical Android-to-macOS slice with:

1. frozen codec/profile and display assumptions;
2. real signaling and transport boundaries;
3. 240-fps glass-to-glass capture and frame-freshness evidence;
4. CPU, memory, thermal, and battery attribution; and
5. security and lifecycle recovery tests.

Before any product adoption, audit MPL-2.0/source-notice obligations and the
maintenance cost of carrying libdatachannel plus VideoToolbox glue. The current
experiment establishes technical viability of a native H.264/WebRTC path, not
production readiness or final architecture selection.
