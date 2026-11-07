# Thin real pairing/reconnect trial

**Status:** procedure ready; physical pairing and reconnect evidence is not yet
recorded. This trial targets one Xiaomi `22101320I` (Android 14) and this Mac.
It exercises identity, pairing, Bonjour/NSD, TLS 1.3, the Rust link state
machine, and automatic reconnect. It does not claim screen mirroring, input, or
product support.

## Build

Use a trusted isolated Wi-Fi network with client isolation disabled. Do not use
ADB forwarding or reverse forwarding; USB is only for installation and
diagnostics.

```sh
export ANDROID_SDK_ROOT="$HOME/Library/Android/sdk"
bash experiments/android-macos-vertical-slice/build-android-core.sh
bash apps/mac-native/build-mac.sh
```

The Android build is arm64-only for the selected Xiaomi and includes the native
Rust JNI bridge. The script bootstraps Gradle through the normal Android build
script when necessary.

Artifacts:

- `experiments/android-macos-vertical-slice/app/build/outputs/apk/debug/app-debug.apk`
- `apps/mac-native/.build/release/idroid-mac-native`

## Pair once

1. Install the APK manually on the authorized phone:

   ```sh
   "$ANDROID_SDK_ROOT/platform-tools/adb" install -r \
     experiments/android-macos-vertical-slice/app/build/outputs/apk/debug/app-debug.apk
   ```

2. Launch the Mac binary. Allow the macOS local-network/firewall prompt if it
   appears. The menu-bar item should show `Searching`.
3. Choose **Pair this phone…**. The Mac generates its Keychain identity on the
   first launch and displays a QR containing the Mac certificate fingerprint,
   one-time token, expiry metadata, and the TLS listener hint.
4. Open the Android app, grant Nearby-device and camera permission, choose
   **Scan Mac pairing QR**, and scan the QR. Android pins the Mac certificate
   fingerprint before sending its certificate and token.
5. The Mac consumes the token and stores the Android certificate in Keychain.
   Android stores the Mac certificate and endpoint in its private preferences.
   The QR listener closes after success or expiry.
6. Leave both continuity services running for at least 30 seconds. Record the
   Mac log path and the displayed link state. Do not copy certificates, tokens,
   or private key material into the report.

The Mac control-plane log is:

```text
~/Library/Application Support/iDroid/control-plane.ndjson
```

## Reconnect scenarios

For each scenario, start with both peers paired and idle. Preserve the log
before and after the scenario, then summarize only event names, source, state,
and elapsed milliseconds.

| Scenario | Action | Expected observation |
| --- | --- | --- |
| Clean link | Wait for the first automatic connection | TLS succeeds; link reaches `Ready` with one session ID |
| Wi-Fi interruption | Disable Wi-Fi for 10 seconds, then restore it | `NetworkChanged`, `Searching`, a new authenticated handshake, then `Ready` |
| Mac sleep/wake | Start the Mac sleep probe manually, sleep/wake, then wait | Reconnect uses Mac `CLOCK_MONOTONIC`; physical wake interval is recorded separately |
| Phone lock | Lock the phone for 30 seconds, unlock it | Foreground service remains alive or reports an explicit loss and reconnects |
| Hotspot switch | Move both peers to the phone hotspot and restore the original LAN | Cached/gateway/Bonjour candidates race; winner source is logged |
| Duplicate paths | Keep Wi-Fi and any permitted second local path available | One authenticated connection survives; the loser is closed without a spurious failure |

Run each interruption at least three times before calculating p50/p95. A
single successful pair is evidence of a real path, not a reliability pass.

## Evidence record

For every run retain a redacted record with:

- source revision and APK SHA-256;
- Mac/Android OS and device model;
- network scenario and start/end wall-clock labels;
- event names and relative monotonic milliseconds;
- authenticated handshake count and winning discovery source;
- reconnect success/failure and timeout reason; and
- whether the sleep probe was physically run.

Keep raw logs, certificates, pairing QR payloads, tokens, and build outputs out
of Git. The sleep-inclusive wake number remains **UNMEASURED** until a human
runs `experiments/mac-clock-sleep/run.sh` on the physical Mac.
