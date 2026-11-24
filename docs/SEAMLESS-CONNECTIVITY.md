# Seamless connectivity: what it actually takes

**Status:** research and architecture note. No implementation in this repository
depends on it, and nothing here is measured on iDroid hardware. Claims are
labelled by evidence class: **[DOC]** vendor documentation, **[RE]** peer-reviewed
reverse engineering, **[OSS]** open-source implementation, **[EST]** estimate.

## 1. The uncomfortable truth

"Seamless" is not a networking property. It is the product of three things that
Apple gets for free and a third party must buy:

1. **An account-level identity directory.** Every device knows every other
   device's public keys, and can address them by name. Apple: IDS. A third party:
   your own account service, or nothing.
2. **A push channel to a sleeping/backgrounded endpoint.** Apple: APNs. A third
   party: APNs *and* FCM — both are usable, but a device token must be
   registered somewhere, which means a server.
3. **A peer-to-peer data path.** Apple: AWDL/LLW. A third party: infrastructure
   Wi-Fi, or Wi-Fi Direct via an Android group owner, or a relay.

Remove (1) and (2) and the honest ceiling is **"works on the same Wi-Fi while
both apps are running and the Mac is awake."** That is where iDroid is today.
It is a legitimate product, but it is not Continuity, and no amount of protocol
polish will change that.

The strategic fork, stated plainly:

| | LAN-only | Full seamless |
| --- | --- | --- |
| Discovery | mDNS + BLE proximity | + account directory |
| Wake | none (Mac must be awake) | APNs (Mac) + FCM (Android) |
| Cross-network | impossible | relay + hole punching |
| Infrastructure | none | one small service, permanently |
| Team cost | small | an on-call rotation |

KDE Connect lives on the left. RustDesk, Tailscale and Phone Link live on the
right. Both are defensible; choosing neither is not.

## 2. How Apple actually does it

Continuity is four reusable layers, not one protocol — each feature picks a
subset:

| Layer | Mechanism | Evidence |
| --- | --- | --- |
| Presence | BLE advertisements with Continuity TLVs (`0x0C` Handoff/Universal Clipboard, `0x05` AirDrop, `0x0D`/`0x0E` hotspot, `0x10` Nearby Info) | [RE] PoPETs 2019/2020 |
| Discovery | mDNS over AWDL (`_companion-link._tcp`) and over infrastructure Wi-Fi | [RE] USENIX Sec '21 |
| Identity | IDS directory + APNs + iCloud Keychain; Ed25519 long-term keys; per-pair AES keys established out-of-band over APNs | [DOC]+[RE] |
| Session | private `rapportd`/Rapport (OPACK/TLV, ChaCha20-Poly1305), `ScreenSharingKit` for Mirroring, `avconferenced` for media | [RE] |

AWDL itself is 802.11 vendor action frames (PSF/MIF) with a 16-slot channel
sequence cycling through social channels 6/44/149, a master election, and IPv6
link-local data frames. It costs **≥25% radio duty even idle** — Apple
deliberately trades battery for availability. It is not reachable by third
parties: `NWParameters.includePeerToPeer` gives you a black box with no frame
control, and Apple's TN3151 says peer-to-peer Wi-Fi "only works between Apple
devices."

The wake story is the part most people get wrong:

| Mechanism | Works for third parties? | Limits |
| --- | --- | --- |
| **Bonjour Sleep Proxy** ("Wake on Demand") | **Yes** | Sleeping Mac hands its mDNS registrations to an always-on proxy (Apple TV/HomePod/AirPort); an incoming **TCP connection to a registered Bonjour service** wakes it. A ping does not. Needs AC power and, on laptops, a display or clamshell-with-power. |
| `pmset proximitywake` | No | Apple-only, same-iCloud-ID BLE proximity. |
| BLE peripheral → wake Mac | **No** | macOS only wakes for Bluetooth **HID** devices, and that toggle is hidden on current releases. No public API. |
| WoL magic packet | Yes | Wired Ethernet in practice; unreliable over Wi-Fi. |
| APNs | Yes | The only sanctioned Mac wake channel for a background agent (PushKit macOS 10.15+, UserNotifications). |

So a sleeping Mac **can** be woken by a third-party app — but only via a Bonjour
service registration (same LAN, proxy present) or an APNs push (needs a server).
There is no BLE shortcut.

## 3. How shipping products do it

| Product | Discovery | Transport | Reconnect trick worth copying |
| --- | --- | --- | --- |
| **KDE Connect** [OSS] | UDP broadcast **and** mDNS, same port, re-announced from every local IP on network change | TCP + TLS, newline JSON, TLS roles **inverted** vs TCP roles | If outbound connect fails, send "you connect to me"; debounce network changes through a 0 ms single-shot; swap the socket under the live device object (`reset()`) so UI state survives; tuned keepalive 30/10/3 instead of the 2 h default |
| **RustDesk** [OSS] | rendezvous server (hbbs) | direct hole punch → relay (hbbr); secretbox after X25519 | Server **signs** each peer's public key, so the rendezvous is a trusted introducer that never sees plaintext; LAN shortcut before punching; UDP and TCP punch pools kept separate |
| **Moonlight/Sunshine** [OSS] | mDNS `_nvstream._tcp`, four address candidates per host (LAN v4, WAN v4, v6, manual) | RTSP handshake, separate video/control/audio UDP streams | **Urgent control channel** that never queues behind video; IDR-on-demand + reference-frame invalidation; hard 10 s liveness timeout; resume via ANNOUNCE/PLAY rather than restart |
| **Tailscale/ZeroTier** [DOC] | coordination server | WireGuard; direct → peer relay → DERP | "Keep trying direct forever, in the background" is the entire UX; stable identity-derived addressing so users never type IPs; DERP chosen by latency |
| **scrcpy** [OSS] | adb | 3 TCP sockets, client **listens** so the server connects out (no connect race) | 12-byte frame headers; no video jitter buffer; change-driven capture; separate threads per stream |
| **Phone Link / Unison** [DOC] | Microsoft/Intel account + BT proximity | **Cloud relay** for messages/photos/notifications; P2P for mirroring; **BT HFP for calls** | Confirms two things: cloud relay is what buys seamlessness, and Bluetooth is the only path to telephony |
| **Samsung Flow** [DOC] | passkey on both screens | BT **or** Wi-Fi **or** USB, user-chosen | "Reconnect through the previous connection method" — cheapest seamlessness win there is |

## 4. Where iDroid actually stands

Current slice: `NWListener`/`NWBrowser` with `includePeerToPeer = true`, TLS 1.3
mutual auth with certificate pinning, newline JSON, a custom `_acontinuity._tcp`
service type, Android `NsdManager` + an `SSLServerSocket` on a fixed port, and a
`connectedDevice` foreground service.

That is a reasonable KDE-Connect-class foundation. Measured against the five
layers:

| Layer | iDroid today | Gap |
| --- | --- | --- |
| Identity | X.509 self-signed cert, fingerprint pinned, Keystore/Keychain | Identity is a certificate fingerprint, but `protocol.rs` still models `DeviceId(u64)`; no key rotation; no revocation propagation |
| Presence | mDNS only | No BLE advertisement, so no "nearby" without a network and no proximity signal for pairing |
| Wake | none | Mac must be awake with the app running; no Sleep Proxy registration, no APNs |
| Session | TCP + TLS 1.3 + JSON lines | No QUIC, so a Wi-Fi→cellular change costs a full handshake; no app-level heartbeat on the wire (only the core's simulated one) |
| Cross-network | none | No rendezvous, no relay, no hole punching |

## 5. Discovery, done properly

The ladder, cheapest first. Every rung is a **hint**; identity is only ever
established by the authenticated handshake.

1. **Cached last-good address** (already a `CandidateSource` in core).
2. **BLE presence advertisement** — a rotating token in service data, not a
   stable identifier. Android: `BluetoothLeAdvertiser`; macOS: `CBPeripheralManager`.
   This is also what gives a real "Nearby" state before any network exists.
3. **mDNS / DNS-SD** — `NWBrowser`/`NWListener` on macOS, `NsdManager` on Android.
4. **Subnet probe / gateway probe** — last resort on networks that block multicast.

Three traps that will cost days each:

- **macOS 15+ local network privacy.** Bonjour register/browse/resolve now
  requires the local-network TCC grant (TN3179). LaunchAgents are subject to it;
  root LaunchDaemons are not, and a binary launched directly from Terminal is
  normally auto-allowed — which is exactly why this hides during development.
  Missing `NSLocalNetworkUsageDescription`/`NSBonjourServices` in Info.plist
  can produce **silent denial: no prompt and no Privacy pane entry**, so the app
  simply never discovers anything. iDroid's SwiftPM executable has **no
  Info.plist and no entitlements at all** (`build-mac.sh` does not create a
  bundle), so the packaging work for TCC is not merely unverified — it does not
  exist yet, and adding BLE or microphone later requires a real `.app` bundle
  (a missing `NSBluetoothAlwaysUsageDescription` *crashes* on CoreBluetooth use).
- **Android `NsdManager.resolveService` is deprecated (API 34)** and is
  single-flight: resolving every `onServiceFound` produces
  `FAILURE_ALREADY_ACTIVE` and a resolver that never recovers. Use
  `registerServiceInfoCallback`, or the `DiscoveryRequest` picker.
- **Android 17 (targetSdk 37)** makes `ACCESS_LOCAL_NETWORK` a runtime permission
  enforced in the network stack — all sockets, not just mDNS.

And the correctness rule Apple states in TN3213: never identify a peer by its
service *name* or TXT record. `NsdDiscovery` currently filters itself out by
comparing `serviceName == "iDroid Android"`, which breaks the moment the system
renames a colliding registration. Filter by TXT device id and role instead.

## 6. Session and transport

**QUIC beats TLS/TCP for this workload**, specifically for two features:

- **Connection migration.** Connection IDs survive an IP change; `quinn` exposes
  `Endpoint::rebind(socket)` so a live connection can follow the interface that
  still works. Cost is one path-validation RTT plus a congestion-window reset.
- **0-RTT resumption.** Saves the handshake on reconnect. **But 0-RTT is
  replayable** — RFC 9001 requires the application to say how it mitigates that.
  The rule for a remote-control channel: 0-RTT may carry *state descriptions*
  ("my last applied input seq is 812"), never *discrete events* (a keystroke, a
  tap, "launch app"). A replayed keystroke is an injected keystroke. Keep a
  monotonic per-direction counter and a key epoch, and drop anything outside the
  window.

**WebRTC is right for media and wrong as the durable channel.** ICE restart is a
signaling operation requiring a live side channel, consent freshness detects
failure in ~5 s, and — decisively — **libdatachannel has no ICE restart** (open
issue since 2022). On an IP change the current media path must tear down and
re-signal, which means seconds, not milliseconds. That is acceptable *if* a QUIC
control channel stays alive to renegotiate quickly. It is not acceptable if
signaling and media share the same fate.

**Liveness** needs three layers, not one: TCP keepalive tuned to 30/10/3 (KDE
Connect's choice, fixing a real "socket looks alive, sends go nowhere" bug),
an application ping every 5–10 s, and the core's degraded/lost thresholds. The
core already has the right thresholds (3 s / 6 s); the wire has none.

## 7. Cross-network, and why a relay is not optional

**Correction (see `PLATFORM-WORKAROUNDS.md` §5):** a relay is mandatory only for
the *tail*, and that tail can be free — Cloudflare TURN includes 1 TB/month, and
`iroh` ships public relays. "You must operate infrastructure" was too strong;
"you must not assume direct connectivity" is the accurate constraint.

Measured failure rates:

- **22% of WebRTC sessions require a TURN relay**; 85% of failures that never
  connect are NAT/firewall traversal [DOC/industry stats].
- Conditional hole-punch success is **~70%** overall, ~83% with cone NAT and
  ~40% with symmetric NAT, across 4.4 M attempts [EST, DCUtR study].
- Tailscale reports >90% direct in typical conditions but states directly that
  two hard NATs cannot connect without a relay.

Design the ladder, and make it visible in the UI: **direct LAN → direct WAN
(hole punch) → relay**. Tailscale's and ZeroTier's seamlessness is mostly this
one decision — keep retrying direct forever in the background while the relay
carries traffic, and upgrade silently when it succeeds.

If a relay is ever built, copy RustDesk's identity trick rather than a PKI: the
server **signs** `{device id, device public key}` with its own Ed25519 key, the
client verifies that signature against a pinned server key, and all session
traffic is then end-to-end encrypted with a key the relay cannot read. That makes
the relay a trusted introducer, not a trusted reader.

## 8. Reconnect choreography

The sequence that produces the "it just reconnects" feeling:

1. **Detect** — interface change notification *and* heartbeat miss. Debounce
   bursts through a single 0 ms timer so one network change causes one attempt.
2. **Re-announce** — mDNS requires re-registering address records after an IP
   change (RFC 6762 §8.4); cached entries elsewhere are now poison.
3. **Resume, don't restart** — carry `session_id` + `epoch` and let the peer
   resume. Never mint a new session id on reconnect; never let a reconnect
   inherit authority revoked while offline (this is `THREAT-MODEL.md` invariant 4).
4. **Dial, then reverse-dial** — if the outbound attempt fails, tell the peer to
   connect back. This beats retry storms and fixes asymmetric firewall rules.
5. **Keep UI state** — replace the transport under a stable device object.
6. **Backoff with jitter**, and stop when the user stops the session.

Four anti-patterns visible in the field: identifying a peer by service name;
letting keepalive be the only liveness signal; tearing down session state on
transport loss; and letting a failed accept loop wedge the listener forever.

## 9. Trust and pairing

QR + TLS pinning (today's design) is TOFU-with-a-physical-check. It is
reasonable, but it has two weaknesses: the QR token is a **bearer secret** (a
photo of the screen pairs an attacker's device), and there is no confirmation
that the *phone* is the device the user thinks it is.

The stronger pattern, in order of value per line of code:

1. **SPAKE2 (RFC 9382)** over the QR's short code, so the code is never sent and
   cannot be brute-forced offline. Both sides derive the same key or fail.
2. **SAS confirmation** — a 6-digit value derived from the transcript, shown on
   both screens, confirmed on both. This is what Bluetooth numeric comparison
   does, and why "Just Works" pairing has no MITM protection.
3. **Channel binding** — mix the TLS exporter (RFC 9266) into the transcript, so
   a relay cannot terminate two connections and forward PAKE messages.
4. **Pin the SPKI hash**, not the leaf certificate, so rotation is possible. Rotate
   by signing `{old_pub, new_pub, timestamp}` with the old key, with an overlap
   window.
5. **Treat pairing as capability grants, not blank authority** — `screen.view`,
   `input.control`, `clipboard`, `file.transfer`, `call.control` — and re-check
   them on every reconnect.
6. **Revocation must work offline** and must fail closed: if revocation state is
   unknown, drop to view-only rather than allowing input.

## 10. Platform policy: the two gates that decide the product

**macOS.** The full feature set **cannot ship on the Mac App Store**.
Accessibility-API control of other apps is categorically incompatible with App
Sandbox, and every MAS app must be sandboxed. Synthetic input via `CGEvent.post`
is technically sandbox-compatible but has been rejected under Guideline 2.4.5 as
non-accessibility use. Shipping path: Developer ID + hardened runtime +
notarization + direct distribution, a per-user LaunchAgent (not a root daemon —
a daemon has no TCC context and cannot prompt), and `SMAppService` for
registration. A reduced MAS app (view-only mirroring, clipboard, files) is a
separate product, not a wrapper around the full one.

Also note: `com.apple.security.screen-capture` and
`com.apple.security.accessibility` **do not exist**. Screen Recording is pure
TCC; input injection is TCC `PostEvent`. Cargo-culting those entitlements is a
common and harmless-looking mistake that signals a misunderstanding to reviewers.

**Android.** Remote control is permitted but *declared*: AccessibilityService
requires the Permission Declaration Form, an in-app prominent disclosure, and a
demo video showing consent, denial, and the core feature; autonomous
"initiate/plan/execute" behavior is prohibited; `FLAG_SECURE` must be respected;
each foreground-service type must be declared in Play Console with its own demo
video. `QUERY_ALL_PACKAGES` almost certainly will not be approved — use
`<queries>`.

## 11. What to build, in order

| # | Step | Why first | Effort [EST] |
| --- | --- | --- | --- |
| 1 | **Wire the core protocol onto the real connection** (one framing, one identity model, one lifecycle) | Everything else is built on a fiction until this is true | 1–2 wks |
| 2 | **Info.plist + entitlements + TCC onboarding** on macOS; fix the Android NSD loop and add a `ConnectivityManager.NetworkCallback` | Without these, discovery fails silently on current macOS and the listener wedges on network change | 3–5 days |
| 3 | **App-level heartbeat + resume-not-restart + reverse-dial** | This alone produces most of the "seamless" feeling on a LAN | 1 wk |
| 4 | **BLE presence advertisement** on both sides | Real "Nearby" state, better pairing proximity, foundation for wake | 1 wk |
| 5 | **QUIC control channel (quinn)** replacing TCP+TLS, with 0-RTT limited to idempotent state | Survives Wi-Fi↔cellular and sleep without a full handshake | 2–3 wks |
| 6 | **Bonjour Sleep Proxy registration** | Wakes a sleeping Mac on the same LAN with no server | 3–5 days |
| 7 | **Account service + APNs/FCM wake** | Cross-network and background wake; the point of no return for infrastructure | 4–6 wks |
| 8 | **Relay + hole punching** with a signed-identity handshake | Last-resort connectivity; makes the promise true | 4–8 wks |

Steps 1–4 are worth doing regardless of which side of the §1 fork the product
chooses. Steps 7–8 are a different company shape.

## 12. Decisions the product has to make

1. LAN-only or full seamless? (§1)
2. Is the Mac allowed to require the app to be running, or must it wake? (§2)
3. Direct distribution only, or a permanently reduced App Store edition? (§10)
4. Is a permanent relay cost acceptable, and who operates it?
5. Is iPhone ever in scope? If yes, only own-VoIP is available (see
   `CALLS-CONTINUITY.md`), which is a different product again.

## Sources

Apple: [TN3151 choosing a networking API](https://developer.apple.com/documentation/technotes/tn3151-choosing-the-right-networking-api),
[TN3179 local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy),
[TN3213 Multipeer → Network.framework](https://developer.apple.com/documentation/technotes/tn3213-moving-from-multipeer-connectivity-to-network-framework),
[Platform Security: call relay](https://support.apple.com/en-us/guide/security/sec28a79bf17/web),
[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice),
[App Sandbox](https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox).

Reverse engineering: [One Billion Apples' Secret Sauce (arXiv 1808.03156)](https://arxiv.org/html/1808.03156v1),
[Stute et al., USENIX Sec '19](https://www.usenix.org/system/files/sec19-stute.pdf),
[Stute et al., USENIX Sec '21](https://www.usenix.org/system/files/sec21-stute.pdf),
[Martin et al., PoPETs 2019](https://petsymposium.org/popets/2019/popets-2019-0057.pdf),
[PoPETs 2020 (BLE type map)](https://petsymposium.org/2020/files/papers/issue1/popets-2020-0003.pdf).

Android: [Telecom](https://developer.android.com/develop/connectivity/telecom),
[NsdManager](https://developer.android.com/reference/android/net/nsd/NsdManager),
[CompanionDeviceManager](https://developer.android.com/reference/android/companion/CompanionDeviceManager),
[local network permission](https://developer.android.com/privacy-and-security/local-network-permission),
[BLE background](https://developer.android.com/develop/connectivity/bluetooth/ble/background),
[FGS types](https://developer.android.com/develop/background-work/services/fgs/service-types),
[AccessibilityService policy](https://support.google.com/googleplay/android-developer/answer/10964491).

Protocols and prior art: [KDE Connect protocol](https://invent.kde.org/network/kdeconnect-meta/blob/master/protocol.md),
[RustDesk rendezvous.proto](https://github.com/rustdesk/hbb_common/blob/main/protos/rendezvous.proto),
[Moonlight internals](https://github.com/moonlight-stream/moonlight-common-c/blob/master/src/Limelight-internal.h),
[scrcpy develop.md](https://github.com/Genymobile/scrcpy/blob/master/doc/develop.md),
[Tailscale DERP](https://tailscale.com/docs/reference/derp-servers),
[ZeroTier protocol](https://docs.zerotier.com/protocol/).

Standards: [RFC 9000](https://www.rfc-editor.org/rfc/rfc9000) (QUIC),
[RFC 9001 §9.2](https://www.rfc-editor.org/rfc/rfc9001) (0-RTT replay),
[RFC 6762](https://www.rfc-editor.org/rfc/rfc6762) (mDNS),
[RFC 9382](https://www.rfc-editor.org/rfc/rfc9382) (SPAKE2),
[RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) (TLS exporter binding),
[RFC 7675](https://www.rfc-editor.org/rfc/rfc7675) (ICE consent freshness).
