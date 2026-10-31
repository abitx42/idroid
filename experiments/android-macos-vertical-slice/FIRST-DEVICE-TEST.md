# First physical Android → Mac video trial

**Status: procedure exercised for diagnostic trials; successful media validation NOT ESTABLISHED.** This is
one Android screen-capture/video receiver, not remote touch control, production
pairing, or the complete HG-2 vertical slice. Follow this from the repository
root in a normal interactive macOS Terminal with a logged-in desktop session.
Do not run the receiver from a remote/public server.

## 1. Safety and prerequisites

- Exactly **one authorized physical Android device** owned by, or explicitly
  authorized for testing by, the operator. Initial intended targets: Pixel or
  Samsung S-series, Android 14/15. No emulators or additional ADB transports.
  Stop if authorization is `unauthorized`, `offline`, or more than one entry is
  listed. Do not automatically authorize anything.
- Mac and phone on the **same trusted, isolated Wi-Fi LAN**, with client
  isolation disabled. Confirm in both devices' Wi-Fi settings. An RFC1918 IP
  alone does not establish trust. No public/guest/campus network, port
  forwarding, tunneling, TURN relay, or automatic firewall disablement.
- Use USB **only for ADB install/log collection**, not media forwarding. Never
  use `adb forward`/`reverse` here. Do not change a corporate VPN without
  permission; move to a permitted test network if local routing is blocked.
- Mac Command Line Tools, Python 3, Java 17+ (existing build uses Java 21),
  Android SDK/platform-tools and the cached Gradle 8.7/dependencies. Native
  dependencies are the existing pinned libdatachannel revision and project-local
  CMake 3.31.10; OpenSSL 3 is also required. See each experiment's README for
  initial dependency setup. Clean-machine/macOS 13–15 compatibility is not
  established by these instructions.
- No screen/terminal recording while entering pairing material. Never put it in
  a command argument, environment variable, `.env`, clipboard log, screenshot,
  report, bug report or saved shell history. Do not use `set -x`, `script`, raw
  logcat, bugreport or network/SDP capture. Choose a fresh, private, random ASCII
  test code for each trial (16+ characters recommended, maximum 256 bytes).
  Enter it manually on both devices. The receiver reads it without echo before
  opening the listener; the app masks it and disables saved/autofill state.
  This is not secure-storage/zeroization or protection from a compromised OS.

## 2. Build exactly the code under test (no cleanup)

Never reset, stash, run `clean`, uninstall, clear app data, or replace result
files. Existing ignored build outputs can be incrementally rebuilt.

For an already configured checkout (preserves `local.properties`):

```sh
export REPO="$PWD"
export ANDROID_EXPERIMENT="$REPO/experiments/android-macos-vertical-slice"
export NATIVE_EXPERIMENT="$REPO/experiments/native-webrtc-h264"
"$ANDROID_EXPERIMENT/.build/gradle-8.7/bin/gradle" \
  -p "$ANDROID_EXPERIMENT" --offline --no-daemon :app:assembleDebug :app:lintDebug
"$NATIVE_EXPERIMENT/.build/tools/cmake-3.31.10/cmake/data/bin/cmake" \
  --build "$NATIVE_EXPERIMENT/.build/native" --parallel 2
export APK="$ANDROID_EXPERIMENT/app/build/outputs/apk/debug/app-debug.apk"
export RECEIVER="$NATIVE_EXPERIMENT/.build/native/android-webrtc-receiver"
"$RECEIVER" --help
```

If dependencies are absent, stop for setup; do not silently fall back to an old
APK/binary. `build-android.sh` bootstraps Gradle and **rewrites** ignored
`local.properties`: use it only for a new configuration, not over existing work.
The native `build.sh` fetches/checks out dependencies and rewrites its environment
snapshot: use the cached CMake build above for an existing checkout. A native
build does not imply that a device session has run.

## 3. Identify one device and the actual Wi-Fi interface

Set `ADB` to the executable in **your** SDK; do not assume it is on PATH:

```sh
export ADB="${ANDROID_SDK_ROOT:?Set ANDROID_SDK_ROOT to your SDK}/platform-tools/adb"
export SERIAL="$(python3 - "$ADB" <<'PY'
import subprocess, sys
adb = sys.argv[1]
listing = subprocess.check_output([adb, "devices"], text=True)
entries = [line.split() for line in listing.splitlines()
           if line.strip() and not line.startswith("List of devices")]
if len(entries) != 1 or len(entries[0]) < 2 or entries[0][1] != "device":
    sys.exit("STOP: require exactly one authorized ADB device and no other transports")
serial = entries[0][0]
qemu = subprocess.check_output([adb, "-s", serial, "shell", "getprop", "ro.kernel.qemu"], text=True).strip()
if serial.startswith("emulator-") or qemu == "1":
    sys.exit("STOP: emulator is not a physical Android device")
print(serial)
PY
)"
test -n "$SERIAL" || { printf 'Device preflight failed; STOP.\n'; exit 1; }
networksetup -listallhardwareports
```

Select the `Device:` for **Hardware Port: Wi-Fi** from that output. Do not
assume `en0`, use the default route, or choose a VPN/Ethernet address:

```sh
export WIFI_IF=en0 # REPLACE if the Wi-Fi hardware port above uses another name
export MAC_WIFI_IP="$(ipconfig getifaddr "$WIFI_IF")"
python3 - "$MAC_WIFI_IP" <<'PY'
import ipaddress, sys
address = ipaddress.IPv4Address(sys.argv[1])
assert any(address in ipaddress.ip_network(net)
           for net in ("10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16")), "STOP: not RFC1918 IPv4"
print("Private Wi-Fi IPv4 selected:", address)
PY
networksetup -getairportnetwork "$WIFI_IF"
```

**Stop** if IP validation fails, the Wi-Fi interface is not associated, or both
devices are not on the confirmed trusted LAN. Recent macOS versions may hide
SSID information from Terminal: inspect System Settings and the phone settings
instead; do not infer association from a private IP. Recheck the IP after every
network change. Do not save the SSID or ADB serial in shared artifacts.

## 4. Reserve a unique evidence directory and install the current APK

The results parent is created if this is a clean checkout. `mktemp` creates a
new child directory; never reuse an earlier directory. Set a restrictive umask
first.

```sh
umask 077
mkdir -p "$NATIVE_EXPERIMENT/results"
export RUN_DIR="$(mktemp -d "$NATIVE_EXPERIMENT/results/first-device.XXXXXXXX")"
python3 - "$ADB" "$SERIAL" "$RUN_DIR" "$APK" <<'PY'
from pathlib import Path
import hashlib, json, subprocess, sys
import platform
adb, serial, directory, apk = sys.argv[1:]
def prop(name):
    return subprocess.check_output([adb, "-s", serial, "shell", "getprop", name], text=True).strip()
metadata = {"deviceModel": prop("ro.product.model"),
            "manufacturer": prop("ro.product.manufacturer"),
            "androidVersion": prop("ro.build.version.release"), "androidSdk": prop("ro.build.version.sdk"),
            "apkSha256": hashlib.sha256(Path(apk).read_bytes()).hexdigest(),
            "macOSVersion": subprocess.check_output(["sw_vers", "-productVersion"], text=True).strip(),
            "macOSBuild": subprocess.check_output(["sw_vers", "-buildVersion"], text=True).strip(),
            "macArchitecture": platform.machine(),
            "sourceCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
            "worktreeMayContainUncommittedChanges": True, "physicalSessionResult": "NOT_TESTED"}
with (Path(directory) / "device-build.json").open("x") as output:
    json.dump(metadata, output, indent=2)
PY
"$ADB" -s "$SERIAL" install -r "$APK"
"$ADB" -s "$SERIAL" shell am start -W -n com.idroid.verticalslice/.MainActivity
```

Require `Success` from install and a successful activity launch; otherwise stop.
If the installed app has a different signing identity, do **not** uninstall or
erase it automatically. Obtain the owner's decision first. The app requests
notification permission and Android screen-capture consent through its UI.
No pairing code is passed through ADB. Verify model/version metadata before
proceeding; targets are intended, not validated support claims.

## 5. Run three separate 30-second trials

Do this section **three times**, setting `TRIAL=1`, then `2`, then `3`. Do not
automate the consent taps. Repeat the one-device preflight if anything is
reconnected. Before each trial, stop any prior capture using the app's **Stop
capture** button. If it is unresponsive, `adb -s "$SERIAL" shell am force-stop
com.idroid.verticalslice` stops only this authorized test app, then relaunch it
with the command in section 4. Force-stop is recovery, not a normal-stop pass.

**Trial 1 is diagnostic-only.** Keep the APK, receiver, port, pairing flow,
timeouts, and transport behavior exactly as specified here. Do not inject input,
change the signaling/media implementation, or treat the result as a product
success. The purpose is to capture the sanitized Android event sequence and the
structured receiver report far enough to classify the observed branch. Preserve
and review trial 1 before proceeding to later trials; a single trial never
establishes the underlying cause or a baseline pass.

1. Prepare the phone fields: Mac's current `MAC_WIFI_IP`, port **18765** and a
   fresh private test code. Grant notification permission if requested. Press
   **Consent and start capture** and wait at the final Android capture-consent
   confirmation, **without granting capture yet**. Choose full-screen sharing
   for a comparable baseline. No secret-bearing screenshot/recording.
2. Inspect port 18765 **before starting** the receiver:

   ```sh
   lsof -nP -iTCP:18765 -sTCP:LISTEN
   ```

   No listener should be listed. If occupied on **any address**, stop and
   investigate the owner; never `killall`, `pkill`, use `fuser -k`, or terminate
   another listener. Do not use `nc -z` or any TCP probe after receiver startup:
   this receiver accepts exactly one client, so a probe consumes the trial.
3. In the same terminal/session with the variables above, select the trial and
   start only the redacted log collector. It never clears Android log buffers:

   ```sh
    export TRIAL=1 # then 2, then 3, one separate run each
     export COLLECTOR_READINESS="$ANDROID_EXPERIMENT/collector-readiness.py"
     test ! -e "$RUN_DIR/trial-$TRIAL.json" && \
       test ! -e "$RUN_DIR/trial-$TRIAL.android.log" && \
       test ! -e "$RUN_DIR/trial-$TRIAL.android.status"
   # STOP if any path exists. Preserve it; select a fresh trial/run directory.
    python3 "$ANDROID_EXPERIMENT/capture-diagnostics.py" \
      --adb "$ADB" --serial "$SERIAL" \
      --output "$RUN_DIR/trial-$TRIAL.android.log" \
      --status "$RUN_DIR/trial-$TRIAL.android.status" &
    LOG_PID=$!
    trap 'kill -TERM "$LOG_PID" 2>/dev/null || true; wait "$LOG_PID" 2>/dev/null || true' EXIT

    COLLECTOR_READY=0
    for _ in $(seq 1 50); do
      if python3 "$COLLECTOR_READINESS" \
          "$RUN_DIR/trial-$TRIAL.android.status" "$LOG_PID" >/dev/null; then
        COLLECTOR_READY=1
        break
      else
        READINESS_EXIT=$?
        if test "$READINESS_EXIT" -eq 1; then
          printf 'STOP: Android diagnostic collector failed, exited, or published an invalid status; preserve the status and log files.\n' >&2
          exit 1
        fi
      fi
      if ! kill -0 "$LOG_PID" 2>/dev/null; then
        printf 'STOP: Android diagnostic collector exited before readiness; preserve the status and log files.\n' >&2
        exit 1
      fi
      sleep 0.1
    done
    test "$COLLECTOR_READY" -eq 1 || {
      printf 'STOP: Android diagnostic collector did not report readiness within 5 seconds.\n' >&2
      exit 1
    }
    ```

   The collector stores only closed-vocabulary `iDroidCapture` diagnostic/state
    tokens. Other text (including SDP, exception strings and unknown fields) is
     discarded before writing. `-T 1` may include one previous diagnostic: use
    `CAPTURE_SERVICE_STARTED` to delimit this run. The adjacent status file uses
    only fixed values: `STARTING`, `COLLECTING_NO_EVENTS`,
    `COLLECTING_EVENTS`, `STARTUP_FAILED`, `COLLECTION_FAILED`,
    `INTERRUPTED_BEFORE_START`, `SHUTDOWN_FAILED`, and `STOPPED`.
    `COLLECTING_NO_EVENTS` means the ADB logcat subprocess started and remains
    alive, not that the app emitted no events. `COLLECTING_EVENTS` means at
    least one line passed the closed-vocabulary redactor. A startup, collection,
    interruption-before-start, or shutdown failure stops the procedure before
    Android consent. No raw-log fallback is permitted. An empty Android log with
    a collecting status means no accepted event has been observed yet; an empty
    log with a failed status is missing evidence.
    Timestamped stage events include button press, projection consent, service
    start, screen capture, first captured frame, SDP offer creation, ICE start
    and completion, TCP connection, peer connection, bounded timeout failures,
    and sanitized signaling failures.
 4. In a **second Terminal**, start the bounded readiness watcher below. It
    checks the listener without connecting to it, requires exactly one listener
    at the selected local address, and verifies that the listener belongs to the
    expected receiver command and this trial's report path. It exits nonzero
    after 10 seconds if verification fails; do not grant Android capture then.

    ```sh
    "$NATIVE_EXPERIMENT/wait-for-receiver.sh" \
      "$RECEIVER" "$MAC_WIFI_IP" 18765 \
      "$RUN_DIR/trial-$TRIAL.json" 45 10
    ```

    In the original Terminal, start the receiver directly, so no wrapper rebuild
    consumes the short setup window. `--port` is explicit because the receiver's
    legacy default remains 8765 while the Android UI defaults to 18765. Pairing
    input is read **silently before binding** and before the TCP accept timer
    begins; the receiver does not display a prompt. Enter the same fresh code
    after the shell instruction below, then wait for the readiness watcher to
    report `receiver_ready` before granting Android capture. Immediately before
    granting consent, rerun the readiness check below; both the fixed status and
    the collector PID must still validate:

    ```sh
    python3 "$COLLECTOR_READINESS" \
      "$RUN_DIR/trial-$TRIAL.android.status" "$LOG_PID" >/dev/null || {
        printf 'STOP: collector readiness was lost before Android consent; do not grant capture.\n' >&2
        exit 1
    }
    ```

    The check reduces, but cannot eliminate, the unavoidable race in which a
    process exits immediately after a successful liveness check. A later
    `COLLECTION_FAILED` or `SHUTDOWN_FAILED` status remains failure evidence.

   ```sh
    printf 'The receiver will read the next input without echo and without displaying a prompt. Enter the fresh private code, then wait for receiver_ready in the other terminal.\n'
   ( set -C
     "$RECEIVER" --bind-address "$MAC_WIFI_IP" --port 18765 \
        --pairing-code-stdin --accept-timeout-seconds 45 \
       --duration-seconds 30 \
       --environment "$RUN_DIR/device-build.json" \
       --output "$RUN_DIR/trial-$TRIAL.json" \
       > /dev/null 2>&1
   )
   ```

    After entering the code, wait for `receiver_ready` from the watcher, then
    immediately grant the pending phone capture confirmation. The code is read
    before binding, so readiness may appear only after this input. This
    diagnostic run allows 45 seconds to accept TCP while the Android service
    initializes capture and gathers ICE. The post-accept offer read remains
    independently limited to 10 seconds. If the accept deadline is reached,
    preserve the timeout, stop the app and retry in a **new** evidence directory.
    Never increase timeouts to conceal a failed attempt.
5. Keep the phone unlocked with benign changing content visible (for example,
   scroll an innocuous settings page manually). Confirm the **phone's** actual
   content appears, not a local test pattern. Do not test banking, passwords,
   OTPs, photos or protected apps. Observe the Android capture indicator and
   foreground notification. Do not inject Mac input: not implemented here.
6. Allow the receiver to finish its 30-second **connected** trial. Do not close
   its window early during baseline trials. After it exits, tap **Stop capture**
   on Android. Preserve `USER_STOP_REQUESTED` and the destruction diagnostics;
   if the receiver had already closed, an intervening peer failure is possible
   and must not be rewritten as a user-stop outcome. Record what happened.
7. End only the collector you launched and inspect the report:

   ```sh
    kill -TERM "$LOG_PID" 2>/dev/null || true
    wait "$LOG_PID" 2>/dev/null || true
   trap - EXIT
    python3 -m json.tool "$RUN_DIR/trial-$TRIAL.json"
    python3 - "$RUN_DIR/trial-$TRIAL.android.status" <<'PY'
from pathlib import Path
import sys
status = Path(sys.argv[1]).read_text(encoding="ascii").strip()
assert status == "STOPPED", "collector did not shut down cleanly"
print("collector_status=STOPPED")
PY
    lsof -nP -iTCP:18765 -sTCP:LISTEN
   ```

   Expect no remaining listener from this receiver. Its exit code alone does
   not mean success: failed/timed-out runs can also exit zero after reporting.
    The structured report and sanitized Android log are the diagnostic evidence;
    native receiver stdout/stderr is intentionally discarded and is not an
    evidence channel. Missing report, `FAIL`, `TIMEOUT`, `DISCONNECTED`, or
    `ABORTED` must be retained. Use a new directory for retries; never overwrite
    a failed trial. Record notes (unique `trial-N-notes.md`, no secrets) with observed
   consent/notification, real content, duration, success/failure and any manual
    actions. Do not change the raw report to replace `NOT_TESTED` values.

### First-trial diagnostic branches

Use the ordered Android diagnostic events together with the receiver JSON report
to describe how far the process got. These are diagnostic branches, not
conclusions about the cause; the earlier `accept_timeout` alone does not identify
the underlying fault.

| Observed evidence | First interpretation and next investigation |
| --- | --- |
| No `TCP_CONNECTING` | Investigate the earlier Android startup, capture, SDP, and ICE-gathering stages. |
| `TCP_CONNECTING` but no `TCP_CONNECTED` | Investigate the connection attempt, selected address/interface, and its timeout. |
| `TCP_CONNECTED` but no receiver acceptance | Compare Android-side and receiver-side timestamps and lifecycle evidence; do not treat the event alone as proof of a transport or receiver defect. |
| Receiver accepts the connection | Move on to signaling and WebRTC negotiation stages, correlating authentication, answer, ICE, peer, and frame evidence. |

Record the exact last Android event, receiver `outcome.stage`/`outcome.code`,
`diagnostics.tcpAccepted`, and receiver-relative timing values. Do not alter
timeouts, pairing, or transport behavior to force a later branch during trial 1.

## 6. Current wire format, timers and report interpretation

No signalling client should be hand-written for the physical run. The APK sends
one bounded UTF-8 newline-terminated line, fields separated by tabs:

```text
OFFER  <session UUID>  <base64url-without-padding SDP>  <HMAC hex>
ANSWER <same UUID>    <base64url-without-padding SDP>  <HMAC hex>
```

The HMAC input uses `OFFER|session|payload` / `ANSWER|session|payload`, HMAC-SHA256
with the entered code. These are format specifications, **not values to log**.
The Mac closes TCP after the answer; media continues over local WebRTC
DTLS/SRTP/UDP. Neither side configures a public ICE server/relay. Do not archive
envelopes, HMACs, candidate lists or SDP. A private IP does not authenticate a
peer; this experimental shared-code design is not production pairing.

| Stage | Current limit / semantics |
| --- | --- |
| Receiver stdin | 8–256 bytes, hidden terminal input; no deadline; before listening |
| Receiver TCP accept | 10 seconds after listener start |
| Receiver offer read | 10-second total deadline from accept, including partial reads; max 262,144 bytes |
| Receiver answer / ICE gather | Separate waits of up to 10 seconds each; send uses 10-second socket timeout |
| Android gather | 5 seconds; then sends gathered offer, no trickle exchange |
| Android TCP connect | 5 seconds using the numeric Mac IPv4; DNS is not part of this test |
| Android answer read | 10-second socket read-idle timeout; bounded line, not a whole-operation deadline |
| Receiver before connection | Safety timer at start + duration + 15 seconds (45 for this test), classified by reached stage |
| Receiver after connection | Full 30 seconds from connected; if no usable first frame at its end, `TIMEOUT`/decoder `FAIL`, not successful completion |

An Android read timeout may precede the receiver's answer/gather timeout. Retain
both sides' evidence. Android SDP/connection callbacks have no independent app
deadline: the operator must Stop on failure; do not leave capture running.

Receiver JSON currently includes `experiment`, `status`, `statusError`,
`outcome.stage`, `outcome.code`, `sessionId`, `bindAddress`, `bindInterface`,
`signalingPort`, `durationSeconds`, `diagnostics`, `connection`, `frames`,
`timing`, `security`, `capture`, `codec`, `input`, `environmentRecord` and
`libdatachannelRevision`. All timing values are receiver-relative milliseconds,
**not** end-to-end latency. `frames.rtpFramesReceived` counts depacketized H.264
frames, not individual network packets. `displayEnqueueCount` is not proof of
displayed-frame identity. `codec.negotiated` is currently a **hardcoded receiver
configuration label**, not a measured Android negotiation result; do not cite it
as evidence of payload/profile agreement. Hardware acceleration is not observed.

For a candidate baseline video success, require all three unique trials to
have real observed phone content, `status=COMPLETED`, authenticated/answer-sent
diagnostics, a non-null `connectionReadyMs`, and nonzero received/decoded/enqueued
counts with their errors/drops retained. Inspect, do not average away, failures.
Android permission/capture-stop gates remain Mac-side `NOT_TESTED`; record
physical observations separately. This does not pass HG-2 (input is absent),
HG-3 (no 240-fps end-to-end evidence), HG-4 (production security incomplete), or
HG-5 (no full lifecycle/recovery campaign). HG-1 is separate portable-core health.

## 7. Failure lookup and bounded recovery

Read `outcome.stage/code` plus Android diagnostic tokens, not just a blank
window. These classify observations; they do not identify every underlying OS
or network cause conclusively.

| Problem | Receiver evidence | Android evidence / action |
| --- | --- | --- |
| TCP failure | `tcp/bind_failed`, `accept_timeout`, `offer_disconnected`; `tcpAccepted=false` | `TCP_CONNECTION_FAILED` / `TCP_CONNECT_TIMEOUT`; check current Wi-Fi IP, listener, allowed Local Network/firewall access and client isolation. Do not disable security globally. |
| Authentication | `signaling/authentication_failed` | `AUTHENTICATION_FAILED` for a bad answer; receiver rejecting an offer can appear as `MALFORMED_SIGNALING`/closed answer on Android. Stop and enter a fresh matching code manually on both sides. Never paste code into an issue. |
| Malformed signalling | `signaling/offer_malformed`, `offer_incomplete`, `offer_oversized`, `invalid_base64`, `invalid_session` | `MALFORMED_SIGNALING`, oversized offer/read errors; ensure the current APK/binary match. No raw envelope logging. |
| ICE gathering timeout | `ice_gathering/gather_timeout` | `ICE_GATHERING_TIMEOUT`; gather may fail before any TCP attempt. Check LAN and use a new run. |
| ICE connection failure | `ice/ice_failed`, `ice/connection_timeout`, or `peer/peer_failed` for ICE/DTLS | `ICE_CONNECTION_FAILED` / `PEER_CONNECTION_FAILED`, or last enum state stuck checking. TCP success alone does not establish a media path. |
| SDP negotiation | `sdp/remote_offer_failed`, `answer_timeout`, `local_answer_failed` | `SDP_CREATE_FAILED`, `SDP_LOCAL_FAILED`, `SDP_REMOTE_FAILED`. Error text is deliberately not logged; do not bypass this with SDP dumps. |
| Decoder failure | `decoder/first_decode_failed`; `frames.decodeFailureCount`, `decoderSetupFailureCount` | No Android decoder result: Android is the sender. Check received vs decoded counts; do not claim hardware decode from CPU. |
| First-frame timeout | `first_frame/no_h264`, `no_decoded_frame`, `no_enqueued_frame` | Correlate last connection state, capture-consent/stop markers and counts. No frames on an idle/protected screen does not prove a decoder fault. |
| User shutdown | Close Mac window: `USER_STOPPED`, `user/shutdown`; Quit may be `ABORTED`, `user/shutdown` | Phone **Stop capture**: `USER_STOP_REQUESTED`; normal cleanup then `CAPTURE_SERVICE_DESTROYED`. Receiver cannot know the remote reason: it may only see `DISCONNECTED`/`peer/disconnected`. Keep both observations. |
| Android stop/revoke | Receiver cannot observe the exact capture cause | `CAPTURE_STOPPED_BY_ANDROID` is separate from app Stop. Do not label it user Stop without observation. |

No auto reconnect is implemented in this slice. After a failure: preserve the
report/logs, Stop the phone capture, close only this receiver window, verify its
port is released, relaunch only the test app if necessary, and start fresh
consent plus a new session/report directory. A session that recovers only by
restarting both ends is a restart, not successful reconnect evidence.

For a separate normal-shutdown check, make a **fourth** unique trial, establish
video and intentionally Stop on Android before the receiver timer. Record
`USER_STOP_REQUESTED` and whatever the receiver actually observes (disconnect
may not be immediate). In another optional run, close the Mac receiver window.
Do not fold these short runs into the three 30-second baselines, and do not
claim an HG-5 lifecycle pass from them.

## 8. Cleanup, evidence privacy and package self-checks

- Tap Stop capture; verify the phone's capture indicator/notification ends.
  Close any remaining receiver window and verify no test listener on 18765.
- Stop only the log collector PID started above. Do not stop the ADB server,
  clear system log buffers, terminate unrelated processes or delete output.
- Disconnect USB if desired; disable USB debugging/revoke this host afterward
  through Android settings if appropriate for the device owner. Do not modify
  unrelated debugging authorizations automatically.
- Reports, sanitized Android logs, and notes in `results/first-device.*/` are local, ignored
  evidence. Preserve them, including failed runs. Pairing values/HMACs never
  belong in them. Reports still contain LAN addresses/session IDs and model/OS
  metadata; review and redact those before sharing externally. Environment
  records from other runs may also contain hostnames/local paths. Never copy an
  unreviewed environment dump into a public issue. No cloud upload by default.
- Record status as **NOT_TESTED** for anything not actually attempted. Building
  an APK, native unit tests or local WebRTC loopback is not a physical session.

Host-only checks, with no physical-device claim (use a currently assigned local
RFC1918 test IP for the negative receiver suite, not a public/wildcard address):

```sh
python3 experiments/android-macos-vertical-slice/test-log-redaction.py
bash experiments/android-macos-vertical-slice/test-diagnostics.sh
IDROID_RECEIVER_TEST_BIND_ADDRESS="$MAC_WIFI_IP" \
  IDROID_RECEIVER_TEST_PORT=28765 \
  bash experiments/native-webrtc-h264/test-android-receiver.sh
git diff --check
```

The JVM entry point runs **12** signaling/privacy tests directly; Gradle's test
discovery does not run them automatically. `test-diagnostics.sh` compiles them
offline using the locked WebRTC AAR/Gradle Kotlin runtime. It creates a unique
ignored `.build/jvm-diagnostics-*` directory and leaves existing outputs alone.
The receiver tests use unique synthetic reports and generated in-memory test
codes, preserve all outputs, and refuse to disturb an existing test-port
listener. They test diagnostics, not a physical ICE/media session.
