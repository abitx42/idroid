# Platform workarounds: bypassing the walls

**Status:** research note. Nothing here is implemented or measured on iDroid
hardware. Each claim is graded:

- **[V]** verified in vendor docs or primary source
- **[P]** verified by a working project or first-hand report
- **[A]** architectural inference — sound but not guaranteed by documentation
- **[?]** unverified, worth a spike

The point of this document is to separate "impossible" from "expensive", because
several constraints that look absolute have a legal, shippable bypass.

## 0. The walls, and whether each falls

| Wall | Absolute? | Bypass |
| --- | --- | --- |
| iPhone calls on the Mac | **No** | HFP. Any phone routes call audio to a paired hands-free unit — no iOS code, no private API. |
| iPhone *control* from the Mac | **Yes** | None. No public input injection on iOS, ever. View and call only. |
| Android cellular call audio capture | **Yes** | None for the app. But the OS itself will route audio to an HFP endpoint, so capture is unnecessary. |
| macOS can't open SCO as a hands-free unit | **No** | Drive a USB dongle from user space (BTstack), a Linux VM with USB passthrough, or an ESP32 HF. |
| macOS Accessibility needed for remote input | **No** | BLE HID: the OS treats it as real hardware. No TCC grant, and an App Store app already ships this. |
| Mac App Store rejects input injection | **No** | Split by direction: controlling the *phone* needs only Bluetooth; controlling the *Mac* needs Accessibility and stays direct-distribution. |
| Wake a sleeping Mac over BLE | **Yes** | None. But Bonjour Sleep Proxy, WoL, or simply not sleeping solves it. |
| Cross-network without any server | **No** | Public signaling relays + Cloudflare TURN free tier + "bring your own relay". |

## 1. iPhone: calls are possible, control is not

The earlier conclusion in `CALLS-CONTINUITY.md` was too pessimistic about the
iPhone. Apple's *Continuity* path is closed — private `TelephonyUtilities`, IDS,
AWDL, non-grantable entitlements. But **HFP is a Bluetooth profile that iOS
implements for every car kit and headset in the world**. A Mac that presents
itself as a hands-free unit gets:

- incoming-call notification with caller ID (`+CLIP`),
- answer / reject / hang up (`ATA`, `CHUP`, `+CHUP`),
- dial (`ATD`), DTMF (`+VTS`), and call waiting/hold,
- two-way audio over SCO.

None of that requires anything on the iPhone beyond Bluetooth pairing. **[V]** for
HFP; **[P]** for macOS implementing the HF side (multiple projects do call
control today, audio is the only gap — §2).

For the screen, iOS has a legal capture path that most people miss:

- **ReplayKit Broadcast Upload Extension** — captures the *whole system*, not
  one app, and streams to an arbitrary endpoint, including a LAN server. The
  user starts it from Control Center. Limits: 30 fps, 50 MB extension memory,
  no DRM content, ~100–200 ms latency **[V]**.
- **ScreenCaptureKit on iOS 27+** replaces it: in-process, full-display or
  per-app, background capture, no extension and no 50 MB ceiling **[V]**.

So an iPhone edition is: HFP for calls + ReplayKit/SCK for view. What it can
never do is inject a touch — there is no `AXUIElement` equivalent, XCUITest's
event synthesis is private and dev-only, and MDM vendors (TeamViewer, Esper,
ManageEngine) all ship view-only for exactly this reason **[V]**. Even Apple's
own in-call Remote Control (iOS 26) has no API.

**Consequence for scope:** iPhone is a *viewer plus speakerphone*, not a
controlled device. That is still a product — it is what "AirPlay to Mac" should
have been — but it is not the Android feature set.

## 2. Android call audio: stop trying to capture it, be the headset

No app may capture cellular call audio (`CAPTURE_AUDIO_OUTPUT` is
signature-only). The bypass is to never capture it: the phone's own telephony
stack will route call audio to a bonded HFP hands-free unit.

The only obstacle is macOS. `IOBluetoothHandsFreeDevice` does call control but
`bluetoothd` refuses the SCO link — `kIOReturnUnsupported` on macOS 26,
`HOST_REJ_RESOURCES` in the daemon log, `0x0D` as far back as 10.8 **[P]**. Note
the asymmetry: the **AG** role does work on macOS 13+ — a public-API project
exposes an HFP audio-gateway SCO link as a CoreAudio input device at 16 kHz
**[P]**. So SCO itself is not broken; the *HF-for-a-phone* direction is refused.

Three ways around it, in increasing hardware cost:

1. **BTstack over libusb on a USB dongle** — BTstack supports macOS, ships
   `hfp_hf_demo` with CVSD + mSBC over SCO-over-HCI, and CSR8510 dongles route
   SCO over USB **by default with no wiring** **[V]**. There is direct precedent
   of HFP AG + SCO audio working on OS X with a CSR8510 **[P]**. Two blockers:
   BTstack is **commercially licensed for products**, and **macOS 26.x kernel
   panics when libusb touches a Bluetooth controller** (libusb#1762) **[V]**.
2. **Linux VM with USB passthrough** — BTstack's own recommendation for macOS
   26: UTM + Ubuntu + CSR8510, confirmed working. The Mac app then carries PCM
   over a local socket. Ugly, but it is a pure-software answer for current macOS
   **[V]**.
3. **ESP32 as the HF radio** — an original ESP32 (only the original has Classic
   Bluetooth) runs BTstack, pairs with the phone, and streams 8 kHz PCM over USB
   serial to a native Mac app. This is the configuration a community project
   actually shipped after hitting both walls above **[P]**. Cost: ~$5, and it
   turns a software feature into a small accessory.

If hardware is unacceptable, the fallback is telephony rather than Bluetooth:

4. **Conditional call forwarding to a SIP DID.** The user dials `**61*<number>#`
   once; missed/busy calls forward to a number the app controls; the Mac is a
   softphone. Caller ID is preserved on the forwarded leg, and Plivo/Telnyx run
   ~$0.005–0.009/min inbound **[V]**. Costs: carrier voicemail is displaced, MVNO
   support is spotty, no programmatic setup without the default-dialer role
   (`TelecomManager` explicitly launches the dialer for MMI codes instead), and
   re-originated calls drop to B/C STIR-SHAKEN attestation, which invites
   "Spam Likely" labelling **[V]**. It works everywhere, including iPhones on
   macOS 26, and it needs no Bluetooth at all.

**Recommendation:** treat the call feature as *HFP with a hardware ladder*, and
keep CCFC as the "no dongle" fallback. Do not spend more time on
`IOBluetoothHandsFreeDevice` audio until §2.1 is measured.

### 2.1 The ten-minute spike

Before any of the above, log the daemon, not the API:

```
log stream --predicate 'process == "bluetoothd"' --level debug
```

Place a call with `IOBluetoothHandsFreeDevice` connected, call `connectSCO()`,
and watch for `DEVMGR_SCO` / `HOST_REJ_RESOURCES`. The framework can report
"audio connected" while no audio flows **[P]**, so the return value is not
evidence. This single log decides whether the feature is software or hardware.

## 3. The Accessibility bypass: BLE HID

This is the most valuable finding in this document.

macOS gates input in two places: **PostEvent** (`CGEvent.post`, shown as
Accessibility) and **ListenEvent** (event taps, shown as Input Monitoring)
**[V]**. Both gate *API use by a process*. A Bluetooth HID device is not an API
call — it is bound by the Bluetooth stack and `IOHIDFamily`, and its events
arrive through the same path as a physical mouse **[A]**, corroborated by:
Deskflow/Synergy *do* require Accessibility because they inject; store apps that
make a Mac act as an ordinary HOGP keyboard require nothing **[P]**.

Two directions, both public API:

- **Android → Mac (phone controls the Mac):** `BluetoothHidDevice` (API 28+)
  registers the app as a Classic HID peripheral with a supplied report
  descriptor. Keyboard + relative mouse + media keys are proven by several
  open-source projects, including ones that target macOS explicitly, on macOS
  12 through 26 **[P]**. No Mac-side software at all. Caveats: only one
  registered app at a time, auto-unregistered when backgrounded (needs a
  foreground service), and some OEM ROMs do not expose the profile **[P]**.
- **Mac → Android (Mac controls the phone):** the Mac publishes HOGP with
  `CBPeripheralManager`. `add()` rejects the short `0x1812` UUID — spell it out
  as `00001812-0000-1000-8000-00805F9B34FB` — add the `0x2908` Report Reference
  descriptor, mark characteristics read-encryption-required, and send an initial
  report on subscribe **[P]**. A shipping App Store app does exactly this and
  lists Android as a supported host **[P]**, though at least one Android 11
  device refused it while Android 16 accepted **[P]**.

For iDroid, the second direction is the one that matters, and it changes the
product shape: **if input arrives as a HID device, the Android app needs no
Accessibility permission, no `dispatchGesture`, and no Play declaration form.**
The phone app becomes a screen-capture app only. That removes the single largest
Play-policy risk in `SEAMLESS-CONNECTIVITY.md` §10.

### 3.1 The spike that decides it

Relative motion gives you a pointer, which covers clicking, dragging, scrolling
and typing — but not absolute "tap this coordinate", and not pinch. The question
worth answering on real hardware: does Android accept an **absolute HID
digitizer** (Usage Page `0x0D`, Tip Switch, absolute X/Y) from a Mac over HOGP?
If yes, coordinate mapping becomes exact and taps/drags are trivially correct. If
no, fall back to relative pointer plus visual cursor tracking from the captured
screen. Test on the declared pilot device, and record the OEM ROM because this
is where device variance lives.

Honest limits even if it works: no multi-touch pinch, no gesture fidelity, and
some apps ignore pointer input entirely. Accessibility `dispatchGesture` remains
the only path for true touch semantics — so HID is a *substitute for most of the
feature*, not a replacement for all of it.

## 4. Wake and proximity: what actually helps

BLE cannot wake a sleeping Mac **[V]**. What can:

| Trick | Works | Cost |
| --- | --- | --- |
| **Bonjour Sleep Proxy** — register `_acontinuity._tcp`; the proxy answers mDNS and ARP for the sleeping Mac and wakes it on an incoming TCP SYN | Yes, third-party legal **[V]** | Needs an always-on proxy (Apple TV/HomePod/AirPort) on the same link, AC power, and a display or clamshell-with-power on laptops |
| **Keep-awake while the phone is near** — Mac scans a BLE presence token from the phone and holds `IOPMAssertion`/`caffeinate` only while it is present | Yes **[V]** | Energy. But it converts an unsolvable wake problem into a solvable presence problem, and it is the only trick that needs no extra hardware |
| **WoL magic packet from the phone** | Yes over Ethernet **[V]** | Requires wired Mac; unreliable over Wi-Fi |
| APNs push to the Mac | Yes **[V]** | Needs a server holding the key. You cannot ship an APNs auth key in an app |

The keep-awake trick is the pragmatic one: it makes "seamless" true in the case
that matters (phone in the room) and fails honestly otherwise.

## 5. Cross-network with no infrastructure

The earlier claim that a relay is mandatory was too strong. A relay is mandatory
*for the tail*, but the tail can be free:

- **Signaling:** `iroh` (Rust) dials by ed25519 public key over QUIC and ships
  three free public relays, with ~90% direct success **[V]**. This is a
  near-perfect fit for iDroid: the core already identifies devices by a 32-byte
  fingerprint, and iroh addresses peers by exactly that kind of key. It replaces
  mDNS-plus-TCP as the cross-network path while mDNS stays the LAN fast path.
- **Native alternative:** `trystero`'s signaling strategies (Nostr, MQTT,
  BitTorrent trackers) run under Node/Bun via `werift` if you prefer JS **[V]**.
  Treat public brokers as plaintext broadcast and derive the key from a shared
  password.
- **Relay tail:** Cloudflare's TURN gives **1 TB/month free**, which covers a
  four-person team at 8 Mbps for ~634 GB/month at zero cost **[V]**. Hetzner
  CX22 is €4.49/month for 20 TB if you want full control **[V]**.
- **Hole punching** succeeds ~70% overall, ~83% behind cone NAT, ~40% behind
  symmetric NAT **[V]**; mobile CGNAT is the worst case, and Wi-Fi↔LTE handover
  breaks the five-tuple mid-session, so migration logic matters more than the
  first-connect rate.
- **IPv6 bonus path:** ~40–50% of users have it; CGNAT does not block it; a free
  DDNS AAAA record plus one firewall pinhole gives a direct route with no
  signaling at all **[V]**. Worth having as an opportunistic rung, not a plan.

Net: **you can ship a cross-network MVP with no servers you operate**, then add
a self-hosted relay for the tail. "Bring your own relay" preserves most of the
UX if the app ships with the relay address and public key baked in **[V]**.

## 6. Concrete fixes for the current code

These are the "loopholes" that matter for the defects found in this audit. Each
is small and independent.

| Defect | Fix | Trick / note |
| --- | --- | --- |
| `ContinuityService` accept loop wedges forever (`catch` swallows, `server` non-null, `START_STICKY` redelivers to a dead object) | try/catch **inside** the accept loop, close and rebind on failure, and restart from `onStartCommand` regardless of `server` | Track a generation counter; treat "listener alive" as a heartbeat, not a variable |
| No network-change handling | Register `ConnectivityManager.NetworkCallback`; on `onAvailable`/`onLost`: re-register mDNS, re-bind the socket, bump the race generation | Also `NsdManager` requires re-registration after an IP change (RFC 6762 §8.4) |
| Self-discovery loop and name-based identity | Advertise a random instance UUID plus TXT `id`/`role`; ignore any candidate whose TXT `id` equals your own identity | Apple TN3213 is explicit that names/TXT are heuristics — bind identity to the TLS handshake |
| `resolveService` single-flight failures | Serialize resolves through a queue with a timeout; on API 34+ use `registerServiceInfoCallback`, never resolve every `onServiceFound` | Dedupe by instance name; treat `onServiceLost` as advisory |
| Link machine driven ceremonially | Hold **one** `NativeCoreLink` for the service lifetime; call only transitions that are valid for the observed state; treat `InvalidTransition` as "ignore" | Return the session id **from** the FFI after `handshake_ready` instead of passing `elapsedRealtime()` |
| Session id is a clock | Mint a random 64-bit session id at pairing, persist it, and reuse it across reconnects | It is an identity, not a timestamp |
| Reconnect is a hard cut | **Make-before-break**: on macOS use `NWConnection.betterPathUpdateHandler` + `viabilityChangedHandler` to establish a second connection and swap; on Android pre-connect on `onAvailable` | Public API, no QUIC required. This is the cheapest "seamless" win available |
| Listener can't rebind after an interface change | Set local-endpoint reuse on `NWParameters`, and `SO_REUSEADDR` on the Android server socket | Avoids `EADDRINUSE` during rapid reconnect |
| No liveness on the wire | App ping every 5–10 s, plus tuned TCP keepalive 30/10/3 | KDE Connect's bug tracker documents why keepalive alone is too coarse |
| Duplicate connections | Call `ConnectionArbiter::admit` on Android too; suppress duplicate discovery within ~500 ms | The core already implements this; only the Mac uses it |
| Pairing token compared in variable time, bearer secret | Compare SHA-256 digests (equalizes length and timing), burn the token after success, and add a 6-digit SAS derived from `SHA256(sorted fingerprints || TLS exporter)` shown on both screens | The exporter binds the code to the actual connection, so a relay can't forward the handshake |
| No app bundle / Info.plist | Generate a minimal `.app` in `build-mac.sh`: `LSUIElement`, `NSLocalNetworkUsageDescription`, `NSBonjourServices` = `_acontinuity._tcp`, `NSBluetoothAlwaysUsageDescription`; sign with entitlements | Without `NSBonjourServices`, macOS 15 can silently deny discovery |
| `openssl` shell-out for identity | `SecKeyCreateRandomKey` + `SecKeyCreateSignature`, key in Keychain with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` | Removes LibreSSL parsing and `Process` from the trust path |

## 7. What each bypass costs

| Bypass | Honest downside |
| --- | --- |
| HFP for calls | Audio is 8/16 kHz mono (CVSD/mSBC) — car-kit quality, not hi-fi. No in-app call UI unless you add it. |
| USB dongle / VM / ESP32 | Extra hardware or a background VM; BTstack needs a commercial licence; macOS 26 breaks the libusb path outright. |
| CCFC + SIP | Manual star codes, displaced carrier voicemail, MVNO gaps, STIR/SHAKEN downgrade, per-minute cost, E911 caveats. |
| BLE HID input | Relative pointer only (unless the digitizer spike succeeds), no pinch/gestures, one HID app at a time, OEM variance, and macOS may reclassify a live BLE HID device as "incompatible". |
| ReplayKit / SCK | 30 fps and a visible recording indicator; no DRM; user must start the broadcast. |
| Keep-awake via proximity | Energy cost; the Mac is awake when it otherwise would not be. |
| iroh / public relays | Public relays are rate-limited and best-effort; you still need a relay for the tail; metadata (who talks to whom, when) is visible to the relay operator. |
| Bring-your-own relay | Configuration friction; support tickets about ports and "why is it relayed?". |

## 8. The no-regret set

If none of the bigger bets is approved, these five are worth doing regardless:
they are small, they fix real defects, and every later option depends on them.

1. Fix the Android accept loop, network callback, and NSD self-filter.
2. One `CoreLink` per service, real session id, make-before-break on macOS.
3. Generate the `.app` bundle with Info.plist, Bonjour services, and entitlements.
4. App-level heartbeat + reverse-dial fallback.
5. Run the two spikes: `bluetoothd` SCO log (§2.1) and HID digitizer over HOGP
   (§3.1). They are hours, not weeks, and they decide the product.

## 9. Impersonation: can Android be an iPhone?

This is the question people actually mean by "trick iOS", so it is worth being
precise. There are **three independent gates**, and they are not equally strong.

| Gate | Root of trust | Spoofable? | What it unlocks |
| --- | --- | --- | --- |
| **Cosmetic** — BLE manufacturer data (`0x004C`), Continuity message types, mDNS service names/TXT, model strings, Bluetooth Class of Device, BLE Appearance | none | **Trivially** — a $20 micro:bit has done it **[V]** | Presence in AirDrop/Handoff UI, generic popups, Wi-Fi-password prompts, DoS, tracking. **No session.** |
| **Protocol interop** — wire formats that are unauthenticated or protected only by a pairing you can perform | the protocol itself | **Yes**, with work **[P]** | AirDrop in *Everyone* mode, Bonjour Sleep Proxy, HFP, BLE HID |
| **Account identity** — IDS registration, iCloud Keychain circles, Apple-signed Apple ID certificates and validation records | Apple ID + SEP-backed keys | **No** **[V]** | Handoff, Universal Clipboard, Continuity calls, Sidecar, Universal Control, iPhone Mirroring |

### 9.1 What cosmetic spoofing actually buys

Almost nothing. Handoff and Universal Clipboard derive session keys from
`RPIdentity-SameAccountDevice` material synced through iCloud Keychain, with
Ed25519 long-term keys and per-device AES keys established out-of-band over
APNs **[V]**. The BLE advertisement is encrypted with a 1-byte auth tag — you can
freeze or desync the UI, but you cannot decrypt the payload or complete a session.
AirDrop *contacts-only* needs an Apple-signed certificate **and** a validation
record matching the receiver's address book **[V]**. A plausible-looking device is
still a stranger.

### 9.2 The legitimate exception, and the best precedent in the industry

**AirDrop in "Everyone" mode has no hardware attestation.** Identity is an
Apple-account certificate plus validation record — account-scoped, not
device-bound — and the transport is ordinary mDNS + TLS + HTTP on 8770/8771 **[V]**. So a non-Apple device can implement it: OpenDrop, omdrop and opendrop-rs
do exactly that, and third-party receivers appear in the iPhone share sheet.

More importantly, **Google shipped it**: Quick Share on Android now interoperates
with AirDrop on unrooted Pixels and Galaxies, using "Everyone for 10 minutes",
with Apple's local protocols reimplemented and no partnership **[V]**. Contacts
Only still excludes non-Apple senders, and Google lists it as future work.

That is the answer to "can Android be an iPhone?": **not as a Continuity peer,
but yes as an AirDrop peer in the one mode Apple leaves open.** It is a file
transfer feature, not a call/mirror feature — but it is a real, shipping,
legally clean one, and it would give iDroid an "iPhone → Mac" file path that
needs no Apple cooperation at all.

### 9.3 The unauthenticated Apple protocol worth implementing: Sleep Proxy

The Bonjour Sleep Proxy protocol has **no authentication**: the host sends DNS
UPDATE messages carrying its service records plus an EDNS0 Owner option (MACs,
optional SecureOn password, sleep/wake sequence), and the proxy then answers
mDNS and ARP/NDP on its behalf and sends a Wake-on-LAN magic packet when a
defined service port receives a SYN **[V]**. macOS discovers any
`_sleep-proxy._udp` advertisement and prefers the lowest metric — there is no
vendor check **[V]**.

Apple's open-source mDNSResponder is not a usable portable server: the POSIX
layer stubs `mDNSPlatformSendWakeupPacket` to a no-op **[V]**. But **Desomnia**
(GPL-3, active) implements the server and client, including ARP/NDP claiming,
leases and SecureOn, and claims verification against real Apple hardware **[P]**.
The older kfix server is stale (Python 2, last tested on OS X 10.10).

Caveats before anyone plans around it: same L2 segment, always-on mains-powered
host, Mac's "Wake for network access" enabled, AC power, and WoWLAN for Wi-Fi
(Ethernet is far more reliable). No independent, logged confirmation of a
third-party proxy waking a current Apple-silicon Mac exists **[?]** — so this is
a spike, not a plan. It is still the most promising way to get Mac wake without
an Apple TV in the house.

### 9.4 The path that works and then gets killed: borrowing Apple identity

Beeper Mini is the definitive experiment. It registered as an iMessage client
from Android hardware using a borrowed identity and worked for days; Apple
blocked it server-side, saying Beeper was "pinging the notification protocols
from a genuine Apple device, when it obviously wasn't" **[V]**. Beeper's own
open-source bridge requires a **real Mac or jailbroken iPhone** as the
registration provider, and Apple then blocked Macs used as relays **[V]**.

The technical reason is that IDS registration needs a server-side
"validation data" blob, and Apple ID sign-in needs Anisette (`X-Apple-I-MD*`)
produced by Apple's obfuscated ADI library, bound to a UDID and checked by the
FairPlay driver, with VM detection **[V]**. Community Anisette servers work by
running Apple's own binaries, are fingerprinted, and get accounts locked **[P]**.

So: **do not build on borrowed Apple identity.** Apple can and does revoke it,
the macOS SLA forbids running macOS on non-Apple hardware, and forging a device
identity that Apple's CA will certify is circumvention, not interoperability —
17 U.S.C. §1201(f) covers analysing a program for interoperability, not defeating
a server-side attestation service **[V]**.

### 9.5 The iOS-as-Android direction

There is nothing to gain. Android offers no privileged gateway that iOS lacks —
no Continuity equivalent, no system-level tether to a desktop, and its equivalent
features (Quick Share, Phone Link) are Android-side clients. Making an iPhone
behave like an Android device unlocks no capability, and the interesting
direction (an Android phone appearing as an Apple peer) is bounded by §9.1.

### 9.6 The legitimate "bridge": a trusted USB pairing

There is a real, non-spoofing way to get iOS data onto a Mac: the lockdown
pairing model. After the user taps Trust, `afc`, `house_arrest`, `mobilebackup2`,
`syslog_relay`, `notification_proxy` and crash-report services are reachable over
USB, and over Wi-Fi after one cable pairing **[V]**.

What that actually yields: media and app-container files, full or incremental
backups (from which SMS, iMessage and call history can be parsed — there is no
live stream), syslog, and Darwin notifications by name. What it does **not**
yield: sending SMS/iMessage, placing or answering calls, live call state, or live
filesystem access. Screenshots require Developer Mode plus a mounted developer
disk image and an RSD tunnel on iOS 17+, and live screen video only arrives with
iOS 27. **[V]**

Practically: a desktop-only, direct-distribution feature (no App Store: sandbox
and public-API rules), with backup-polling latency measured in minutes. Useful
for "show me my iPhone's messages on the Mac", useless for calls.

### 9.7 Impersonation ledger

| Goal | Verdict |
| --- | --- |
| Appear in AirDrop/Handoff UI with a fake Apple device | Feasible, cosmetic only |
| Receive AirDrop from an iPhone in Everyone mode | **Feasible today** (OpenDrop, omdrop, Google Quick Share) |
| AirDrop or Continuity as a *contact* | Needs an Apple-account certificate + validation record |
| Handoff / Universal Clipboard with a non-Apple peer | Needs the account's synced keys; not demonstrated end-to-end |
| Sidecar, Continuity Camera, Universal Control, iPhone Mirroring | No third-party implementation exists; model checks and account keys are not the only gate |
| Register a new IDS identity | Not obtainable; forgeries blocked server-side (Beeper) |
| Wake a Mac via a third-party Sleep Proxy | Plausible and unauthenticated; spike required |
| Calls from an iPhone via HFP | **Works** — no impersonation needed at all (§2) |
| Control an iPhone from the Mac | Impossible on any path |

## Sources

Apple: [TN3151](https://developer.apple.com/documentation/technotes/tn3151-choosing-the-right-networking-api),
[TN3179](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy),
[TN3213](https://developer.apple.com/documentation/technotes/tn3213-moving-from-multipeer-connectivity-to-network-framework),
[IOBluetoothHandsFree](https://developer.apple.com/documentation/iobluetooth/iobluetoothhandsfree),
[ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit),
[ReplayKit broadcast](https://developer.apple.com/documentation/replaykit/rpbroadcastsamplehandler),
[local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy).

Android: [BluetoothHidDevice](https://developer.android.com/reference/android/bluetooth/BluetoothHidDevice),
[MediaRecorder.AudioSource](https://developer.android.com/reference/android/media/MediaRecorder.AudioSource),
[NsdManager](https://developer.android.com/reference/android/net/nsd/NsdManager),
[TelecomManager](https://developer.android.com/reference/android/telecom/TelecomManager).

Bluetooth audio: [BTstack chipsets (SCO over HCI vs PCM)](https://github.com/bluekitchen/btstack/blob/master/chipset/README.md),
[BTstack libusb port](https://github.com/bluekitchen/btstack/blob/master/port/libusb/README.md),
[libusb#1762 macOS 26 panic](https://github.com/libusb/libusb/issues/1762),
[BTstack HFP-AG on OS X with CSR8510](https://groups.google.com/g/btstack-dev/c/pZmp9QVWYBk),
[handsfree (ESP32 HF)](https://github.com/jensb1/handsfree),
[phonebt SCO rejection](https://github.com/icoainc/phonebt/issues/1),
[DJI Mic 2 Bridge (macOS AG + CoreAudio)](https://github.com/fky1990/DJI-Mic-2-Bridge).

HID: [darwin-bt-remote (macOS HOGP peripheral)](https://github.com/jqssun/darwin-bt-remote),
[CoreBluetooth HID notes (UUID trap, 0x2908)](https://github.com/sryo/clak/blob/main/docs/corebluetooth-hid-notes.md),
[android-bt-remote](https://github.com/jqssun/android-bt-remote),
[Linkpad](https://github.com/Devdas-gupta/linkpad).

Connectivity: [iroh FAQ](https://docs.iroh.computer/about/faq),
[iroh relays](https://docs.iroh.computer/concepts/relays),
[trystero signaling](https://trystero.dev/docs/signaling-strategies),
[Cloudflare TURN pricing](https://developers.cloudflare.com/realtime/sfu/pricing),
[Hetzner Cloud](https://www.hetzner.com/cloud/regular-performance),
[DCUtR](https://docs.libp2p.io/docs/dcutr),
[RFC 6762](https://www.rfc-editor.org/rfc/rfc6762).

Impersonation: [USENIX Sec '21 (Handoff/Rapport)](https://www.usenix.org/system/files/sec21-stute.pdf),
[AirDrop security](https://support.apple.com/guide/security/airdrop-security-sec2261183f4/web),
[PrivateDrop](https://www.usenix.org/system/files/sec21-heinrich.pdf),
[OpenDrop](https://github.com/seemoo-lab/opendrop),
[Google: Quick Share ↔ AirDrop](https://security.googleblog.com/2025/11/android-quick-share-support-for-airdrop-security.html),
[Beeper Mini](https://www.theverge.com/2023/12/9/23995150/beeper-imessage-android-apple-statement),
[Beeper bridge requires a real Mac](https://github.com/beeper/imessage),
[iMessage/IDS reverse engineering](https://jjtech.dev/reverse-engineering/imessage-explained),
[Apple ID device identifiers (Leith)](https://www.scss.tcd.ie/Doug.Leith/pubs/iphone_identifiers.pdf),
[Attestation process security](https://support.apple.com/guide/security/attestation-process-security-sec97eb9e2f2/web),
[Bonjour Sleep Proxy (Cheshire)](https://stuartcheshire.org/sleepproxy),
[Desomnia sleep proxy](https://desomnia.readthedocs.io/en/latest/modules/network/sleepproxy.html),
[libimobiledevice](https://libimobiledevice.org/),
[pymobiledevice3](https://github.com/doronz88/pymobiledevice3).

Telephony: [GSM MMI codes (AOSP)](https://android.googlesource.com/platform/frameworks/opt/telephony/+/refs/tags/android-13.0.0_r1/src/java/com/android/internal/telephony/gsm/GsmMmiCode.java),
[Twilio voice pricing](https://www.twilio.com/en-us/voice/pricing/us),
[Telnyx Mobile Voice](https://telnyx.com/products/mobile-voice),
[STIR/SHAKEN attestation](https://developers.telnyx.com/docs/voice/stir-shaken/attestation-behavior).
