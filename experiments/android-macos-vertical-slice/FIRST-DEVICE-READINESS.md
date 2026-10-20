# First-device package: host validation record

This records host-only checks of the first-device package, not a physical
Android session or architecture decision. No commit was created. All existing
work and prior evidence were retained; new negative-test reports use unique
randomized filenames. The manual procedure is [FIRST-DEVICE-TEST.md](FIRST-DEVICE-TEST.md).

## Changes confined to confirmed readiness gaps

- Native receiver: fixed outcome classifications, phase flags and ICE state;
  count decoder setup failures; classify zero-frame trials as timeout/decoder
  failure; preserve state before teardown; discard untrusted library error text.
- Native CLI: hidden `--pairing-code-stdin` entry avoids actual test codes in
  command arguments/history. No signaling wire-format change.
- Android: closed diagnostic categories for TCP/authentication/malformed
  signaling/SDP/ICE; explicit user Stop; suppress raw library errors; pairing
  field masked and excluded from saved/autofill state; UI default port 18765.
- Test package: one-device/Wi-Fi/port/install/three-trial runbook, exclusive
  redacted-log output, host-only diagnostic runners, and ignored private trial
  directories. No capture/input feature expansion.

## Commands actually run and results

All commands below run from the repository root. The negative receiver suite
uses a local assigned RFC1918 address on a private test port, not a public
listener. It is synthetic signaling, not an Android or Wi-Fi media test.

```sh
experiments/native-webrtc-h264/.build/tools/cmake-3.31.10/cmake/data/bin/cmake \
  --build experiments/native-webrtc-h264/.build/native --parallel 2
experiments/native-webrtc-h264/.build/native/receiver-diagnostics-test
experiments/native-webrtc-h264/.build/native/android-webrtc-receiver --help
```

PASS: native loopback and receiver targets build; completion policy has six
cases, outcome/sanitization policy has twelve cases. Help matches stdin option.

```sh
IDROID_RECEIVER_TEST_BIND_ADDRESS="$(ipconfig getifaddr en0)" \
  IDROID_RECEIVER_TEST_PORT=28765 \
  bash experiments/native-webrtc-h264/test-android-receiver.sh
```

PASS: 33 script checks plus the above 18 policy cases. Synthetic failures and
timeouts are expected assertions, not failed tests. Includes stdin boundaries,
no echo/interrupt restoration, SDP error redaction, existing signaling
regressions and timeout cleanup. The local test port was not left listening.
`en0` here records this host-only test command; the physical runbook requires
identifying the actual Wi-Fi hardware interface instead of assuming its name.

```sh
experiments/android-macos-vertical-slice/.build/gradle-8.7/bin/gradle \
  -p experiments/android-macos-vertical-slice --offline --no-daemon \
  :app:assembleDebug :app:lintDebug
bash experiments/android-macos-vertical-slice/test-diagnostics.sh
python3 experiments/android-macos-vertical-slice/test-log-redaction.py
```

PASS: APK build; Android lint **0 errors, 25 warnings**; **12 direct JVM
signaling/privacy tests**, including real local socket read timeout; **2 Python
redaction tests**. These JVM tests are an explicit entry point, not Gradle test
discovery. Build still warns that AGP 8.5.0 was tested only through compileSdk
34, whereas this experiment uses 35, and about SDK XML-tool version mismatch.
No on-device permission/input/capture/lifecycle test ran.

```sh
docker run --rm --network none \
  --mount "type=bind,source=$PWD,target=/workspace,readonly" \
  -e CARGO_TARGET_DIR=/tmp/idroid-validation-target -w /workspace \
  idroid-ci:local sh -c 'make test && make lint'
```

PASS: **17 Rust unit + 5 integration tests**, formatting and Clippy with
warnings denied. Fresh target output and disabled container networking; cached
image/toolchain used. This does not prove offline Docker image provisioning or
native/Android clean-machine reproducibility.

`git diff --check` and shell syntax checks passed. The nine runbook shell blocks
were parsed with `bash -n` only, without executing their ADB/install/receiver
steps. Python collector/test sources parsed successfully. Running the manual
procedure itself remains **NOT TESTED**.

## Gate status and blockers

| Gate | Evidence status |
| --- | --- |
| HG-1 | Portable Linux core tests/lint pass in cached pinned-toolchain Docker with network disabled; not a new clean-machine platform distribution claim. |
| HG-2 | **NOT TESTED**: no physical Android media session; input also absent from this slice. |
| HG-3 | **NOT TESTED**: no valid 240-fps end-to-end latency evidence. |
| HG-4 | **INCOMPLETE**: shared-code HMAC is experimental; no production identity lifecycle, DTLS identity binding policy, revocation/replay design implementation or security acceptance. |
| HG-5 | **NOT TESTED on real devices**: synthetic diagnostics are not lifecycle/recovery evidence. |

Remaining work: run the three trials on one authorized physical Android phone
on trusted shared Wi-Fi, capture observations and redacted evidence, then
investigate actual interoperability failures. Android/Samsung capture consent,
background survival, encoder behavior, rotation, lock and stop remain unverified.
The receiver's `codec.negotiated` is still a configuration string, not measured
negotiation, and no displayed-frame identity/hardware-decode claim is made.
Android SDP callbacks have no application deadline; the operator must Stop a
stalled capture. No automatic reconnect or production trust system is claimed.
