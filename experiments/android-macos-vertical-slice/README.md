# Android-to-macOS vertical-slice experiment

**Status:** acceptance plan frozen; a buildable Kotlin debug APK and a buildable
native macOS receiver exist. A physical diagnostic run reached Android consent,
foreground-service startup, screen capture, and local SDP creation, but no
successful Android-to-Mac media session has been established.

This experiment is deliberately smaller than the product. It targets one
physical Android device and one macOS host, one screen-capture session, one
H.264 video track, one visible macOS receiver, and one bounded session. It does
not add Finder integration, a virtual camera, audio calls, multi-device
support, public relay infrastructure, or production onboarding.

## Current hardware and tooling inventory

The inventory was measured on the current host before implementation:

- Mac: MacBook Air `Mac17,3`, Apple M5, 16 GB RAM, macOS `26.6.2`, build
  `25G83`.
- Apple development tools: Command Line Tools at
  `/Library/Developer/CommandLineTools`; no valid code-signing identities were
  returned by `security find-identity -p codesigning -v`.
- Android hardware at the initial inventory point: **no physical Android device attached**;
  `$HOME/Library/Android/sdk/platform-tools/adb devices -l` listed no devices.
- Wi-Fi: the host reported no active AirPort association. `en0` had a private
  `172.30.192.133` address and a default route, but a shared Android/macOS
  Wi-Fi network was not verified.
- Android tooling: Homebrew `sdkmanager` `22.0` is available. The SDK under
  `/opt/homebrew/share/android-commandlinetools` has platform-tools, Android
  platforms, build-tools, CMake, and NDK `28.2.13676358`. `adb` is not on
  `PATH`; Gradle and `kotlinc` are not on `PATH`; Java `21.0.12` is installed.
- Repository state: no Android Gradle project, APK, Android source, or Android
  signing configuration existed at the start of this experiment.

The initial missing device, unverified shared Wi-Fi, and lack of signing identity
were host-readiness limitations; the later physical run remains diagnostic-only.
The Android build
uses a project-local Gradle `8.7` bootstrap and keeps resolved dependencies in
`app/gradle.lockfile`; it still needs an Android SDK supplied through
`ANDROID_SDK_ROOT`/`ANDROID_HOME` or a standard per-user SDK path.

## Frozen acceptance criteria

The following gates are frozen before implementation. They are experiment
gates, not replacements for the product hard gates in
[`docs/DECISION-GATES.md`](../../docs/DECISION-GATES.md).

| Gate | PASS evidence | Current status |
| --- | --- | --- |
| Real Android-to-Mac connectivity | A physical Android device and this Mac establish one authenticated session over the same declared Wi-Fi, with session ID and endpoint logs. | **NOT_ESTABLISHED** — diagnostic run reached no TCP connection. |
| Android capture consent | User initiates MediaProjection consent; denial produces no capture and no session. | **PARTIAL** — consent denial and grant were observed; latest source changes need a fresh run. |
| Foreground-service lifecycle | Capture runs only with the declared foreground-service type and visible notification; stop/revoke tears down the projection. | **PARTIAL** — startup and capture events observed; full stop/revoke remains unverified. |
| H.264 media | Android negotiates H.264; Mac records negotiated profile/packetization, received frames, decoded frames, and decoder limitations. | Local native H.264 path passes; Android interoperability **NOT_ESTABLISHED**. |
| Frame rate/freshness | Three 30-second device trials record received/decoded/dropped frames and freshness indicators; no displayed-frame identity is inferred where unavailable. | **NOT_TESTED**. |
| Setup/first frame | Connection setup, first received, first decoded, and first-ready/display observations are recorded with unique reports. | **NOT_TESTED**. |
| Glass-to-glass | A calibrated 240-fps camera records Android input/capture marker and Mac display marker; p50/p95/p99/max, sample count, uncertainty, and failures are retained. | **NOT_TESTED** — no camera/device rig. |
| Mac-to-phone input | Only a supported, consented Android input mechanism is tested; unsupported generic injection remains `NOT_TESTED`. | **NOT_TESTED**. |
| Authenticated session | Pairing identity and session ID are bound to an HMAC-authenticated signaling envelope and WebRTC DTLS identity; no fixtures are used. | Prototype design only; **NOT_TESTED**. |
| Wi-Fi interruption/recovery | At least one controlled disconnect/reconnect preserves explicit failure state and bounded recovery behavior. | **NOT_TESTED**. |
| Screen lock/capture stop | Lock, projection revoke, app stop, and process death produce explicit capture-stop/session-stop reports. | **NOT_TESTED**. |
| Resources | Native process CPU/RSS plus Android CPU/memory/battery/thermal observations are recorded with attribution limits. | **NOT_TESTED**. |

Unavailable gates remain unavailable; they must not be converted into passes by
the existing local two-peer loopback.

## Minimal implementation boundary

### Android

- Kotlin test application only.
- User-visible host/port/pairing-code fields; each capture creates a random
  session ID.
- User-initiated `MediaProjectionManager` consent.
- A foreground service with a visible notification and the correct
  `mediaProjection` service type.
- `ScreenCapturerAndroid`/WebRTC H.264 video track when supported by the device.
- Explicit stop/revoke handling where the API permits; rotation remains
  untested until a device is available.
- No Accessibility or generic touch injection claim.

### macOS

- Extend the existing native receiver candidate only for a remote mode.
- Listen on a bounded local TCP signaling port.
- Validate an experimental user-entered pairing code using HMAC-SHA-256 over
  bounded signaling envelopes and an explicit session ID.
- Use libdatachannel for the remote H.264 WebRTC receive track and VideoToolbox
  for decode.
- Keep WebRTC DTLS/SRTP as the media confidentiality layer.
- Show the decoded track in the existing AppKit presentation surface.
- Report disconnect, malformed message, stale session, capture-stop, and
  timeout states explicitly.

The pairing-code signaling design is an experimental boundary, not production
authentication. It must not be exposed to an untrusted network or described as
the finished security model. Production pairing still requires platform key
storage, identity lifecycle, authorization, revocation, replay protection, and
negative tests. Binding to one private Wi-Fi address does not isolate the port
from other devices on that same network; use a trusted, isolated test network.

## Buildable implementation

For the first phone session use the saved package
[`FIRST-DEVICE-TEST.md`](FIRST-DEVICE-TEST.md): one authorized physical-device
guard, private Wi-Fi selection, fresh run directories, redacted diagnostics,
three trials, and cleanup. Trial 1 is diagnostic-only. Build before preparing
consent; the prebuilt receiver's accept window is only 10 seconds. No
physical-device execution is claimed here.

From the repository root, provide an Android SDK path when it is not already in
`$HOME/Library/Android/sdk` or `$HOME/Android/Sdk`:

```sh
ANDROID_SDK_ROOT=/path/to/android-sdk \
  bash experiments/android-macos-vertical-slice/build-android.sh
```

The script downloads Gradle `8.7` only into the experiment's ignored `.build/`
directory, writes the ignored `local.properties`, and builds:

```text
experiments/android-macos-vertical-slice/app/build/outputs/apk/debug/app-debug.apk
```

After the Gradle distribution, SDK, and locked dependencies are cached, the
same build can be checked without network access:

```sh
IDROID_ANDROID_OFFLINE=1 \
  ANDROID_SDK_ROOT=/path/to/android-sdk \
  bash experiments/android-macos-vertical-slice/build-android.sh
```

The native receiver is built together with the existing native H.264 probe:

```sh
bash experiments/native-webrtc-h264/build.sh
experiments/native-webrtc-h264/.build/native/android-webrtc-receiver \
  --help
```

When a physical Android device and shared Wi-Fi are available, follow
`FIRST-DEVICE-TEST.md` exactly. Start the redacted Android collector first,
then use the direct receiver command below in the original terminal and the
bounded readiness watcher in a second terminal:

```sh
experiments/native-webrtc-h264/wait-for-receiver.sh \
  "$RECEIVER" "$MAC_WIFI_IP" 18765 \
  "$RUN_DIR/trial-$TRIAL.json" 45 10

experiments/native-webrtc-h264/.build/native/android-webrtc-receiver \
  --bind-address "$MAC_WIFI_IP" \
  --pairing-code-stdin \
  --port 18765 \
  --accept-timeout-seconds 45 \
  --duration-seconds 30 \
  --environment "$RUN_DIR/device-build.json" \
  --output "$RUN_DIR/trial-$TRIAL.json"
```

Set the non-secret address variable as described in `FIRST-DEVICE-TEST.md`.
The receiver reads the pairing code silently before binding and displays no
prompt; enter it only after the shell instruction, wait for
`receiver_ready`, then grant MediaProjection consent. Never put the code in a
command, environment variable or file. The receiver report records the bounded
signaling result, WebRTC connection state, H.264 decode/presentation counters,
and explicit `NOT_TESTED` limits for Android-only measurements. Native receiver
stdout/stderr is not retained; the structured report is the receiver-side
evidence. The physical run completed as a diagnostic attempt but did not
establish a successful media session.

## Evidence rules

- Every run uses a unique report path.
- Failed, timed-out, denied-consent, disconnected, and capture-stopped runs are
  preserved.
- `NOT_TESTED` is used for absent hardware, absent camera, absent permissions,
  or unavailable API evidence.
- Network RTT, encode time, decode time, WebRTC callback time, and JavaScript
  timing are not substitutes for 240-fps glass-to-glass latency.
- The existing local native loopback and WKWebView results remain separate
  evidence classes and cannot satisfy the Android/macOS vertical-slice gate.
