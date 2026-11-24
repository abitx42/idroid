# Findings ledger

Append-only. One claim per row. Grades and statuses are defined in
[`README.md`](README.md). Search this file **before** starting any research.

## Calls and telephony

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-001 | settled | Apple's Continuity call relay is APNs signaling plus an Apple-internal peer-to-peer audio channel, with BLE only to stop ringing elsewhere. It is not Bluetooth HFP. | [V] | `docs/CALLS-CONTINUITY.md` §1; Apple Platform Security |
| F-002 | settled | No public API lets a third-party Mac or iOS app answer or place the iPhone's cellular calls. The private path needs `access-calls`/`modify-calls` entitlements that are not grantable. | [V] | `docs/CALLS-CONTINUITY.md` §1 |
| F-003 | settled | An iPhone will route cellular call audio to any paired HFP hands-free unit, so Mac-as-HF gives iPhone calls with **zero iOS code**. No impersonation and no private API is involved. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §1 |
| F-004 | settled | Third-party apps cannot inject touch or keyboard into iOS. There is no `AXUIElement` equivalent; XCUITest event synthesis is private and dev-only; every MDM vendor ships view-only. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §1 |
| F-005 | settled | Full-screen iOS capture is legal via the ReplayKit Broadcast Upload Extension (whole system, user-started from Control Center, 30 fps, 50 MB, no DRM) and, on iOS 27+, ScreenCaptureKit. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §1 |
| F-006 | settled | Android forbids third-party cellular call-audio capture: `VOICE_CALL`/`REMOTE_SUBMIX` need `CAPTURE_AUDIO_OUTPUT` (signature-only), and `AudioPlaybackCapture` excludes `USAGE_VOICE_COMMUNICATION`. | [V] | `docs/CALLS-CONTINUITY.md` §2 |
| F-007 | settled | macOS refuses SCO in the HFP **HF** direction (`kIOReturnUnsupported`, `HOST_REJ_RESOURCES` in `bluetoothd`) while the **AG** direction works and can surface SCO as a CoreAudio input. | [P] | `docs/PLATFORM-WORKAROUNDS.md` §2 |
| F-008 | settled | BTstack supports macOS over libusb with a USB dongle, ships HFP HF and AG demos with CVSD and mSBC, and CSR8510 dongles route SCO over USB by default with no wiring. A commercial licence is required to ship. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §2 |
| F-009 | settled | On macOS 26.x, accessing a Bluetooth controller through libusb triggers a kernel panic, so the BTstack-on-host path is not viable there; a Linux VM with USB passthrough is the workaround. | [V] | BTstack libusb README; libusb#1762 |
| F-010 | settled | The shipped workaround for call audio is an original ESP32 acting as the HFP HF radio, with 8 kHz PCM over USB serial to a native Mac app. Only the original ESP32 has Classic Bluetooth. | [P] | `docs/PLATFORM-WORKAROUNDS.md` §2 |
| F-011 | settled | Conditional call forwarding (`**61*<did>#`) to a SIP DID is a hardware-free path; caller ID survives, but carrier voicemail is displaced, MVNO support is uneven, and re-origination drops to B/C STIR-SHAKEN attestation. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §2 |

## Remote input

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-012 | settled | Bluetooth HID input bypasses the Accessibility/Input Monitoring TCC grants because it arrives through the HID stack like real hardware, not through `CGEvent.post`. Apple never states this policy; the mechanism is architectural, and shipping apps rely on it. | [A] | `docs/PLATFORM-WORKAROUNDS.md` §3 |
| F-013 | settled | Android's `BluetoothHidDevice` is Classic BR/EDR HID only, allows one registered app at a time, auto-unregisters when backgrounded, and is missing on some OEM ROMs. Keyboard, relative mouse and media keys are proven by multiple projects. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §3 |
| F-014 | settled | A Mac publishing HOGP must use the full `00001812-0000-1000-8000-00805F9B34FB` UUID, add `0x2908` Report Reference descriptors, mark characteristics encryption-required, and send an initial report on subscribe. | [P] | `docs/PLATFORM-WORKAROUNDS.md` §3 |
| F-015 | open | Whether Android accepts an **absolute HID digitizer** (Usage Page `0x0D`, absolute X/Y) from a Mac over HOGP is untested. If it does, tap-at-coordinate becomes exact and Accessibility is unnecessary for most of the feature. | [?] | `docs/PLATFORM-WORKAROUNDS.md` §3.1 |
| F-016 | settled | BLE HID gives pointer semantics only: click, drag, scroll, type. Pinch and multi-touch gestures still require Android Accessibility `dispatchGesture`. | [A] | `docs/PLATFORM-WORKAROUNDS.md` §3 |

## Wake, presence, and power

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-017 | settled | A sleeping Mac cannot be woken by a BLE peripheral. macOS wakes only for Bluetooth HID-class devices, and that toggle is hidden on current releases. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §2 |
| F-018 | settled | Bonjour Sleep Proxy can wake a sleeping Mac for a registered service: the proxy answers mDNS/ARP and sends a Wake-on-LAN magic packet on an incoming SYN. Third-party implementation is unauthenticated and permitted by discovery, but poorly verified on Apple silicon. | [P] | `docs/PLATFORM-WORKAROUNDS.md` §9.3; `F-019` |
| F-019 | open | Whether a third-party Sleep Proxy (e.g. Desomnia on a Pi/router) reliably wakes a current Apple-silicon Mac is unverified; no independent logged report exists. Requires same-L2, always-on, AC-powered host. | [?] | `docs/PLATFORM-WORKAROUNDS.md` §9.3 |
| F-020 | settled | Holding an `IOPMAssertion` while the phone's BLE presence token is in range converts the unsolvable wake problem into a solvable presence problem, at an energy cost. | [A] | `docs/PLATFORM-WORKAROUNDS.md` §4 |
| F-021 | settled | APNs/FCM wake requires a server-side signing key that cannot be shipped inside an app, so push wake implies infrastructure. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §4 |

## Discovery and transport

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-022 | settled | AWDL is undocumented and unreachable for third parties; `NWParameters.includePeerToPeer` is a black box with no frame, election, or QoS control. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §2 |
| F-023 | settled | QUIC is the right control channel for migration and resumption: `quinn` supports `Endpoint::rebind` and 0-RTT. 0-RTT may carry idempotent state descriptions but never discrete input events, which are replayable. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §6 |
| F-024 | settled | libdatachannel has no ICE restart (open since 2022), so an IP change requires tearing down and re-signaling the whole peer connection. WebRTC is right for media, wrong as the durable channel. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §6 |
| F-025 | settled | Hole punching succeeds ~70% overall, ~83% behind cone NAT and ~40% behind symmetric NAT; mobile CGNAT is the worst case, and Wi-Fi↔LTE handover breaks the five-tuple mid-session. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §7 |
| F-026 | settled | Zero-infrastructure cross-network is viable: `iroh` addresses peers by ed25519 key over QUIC with public relays (~90% direct claimed), `trystero` can signal over Nostr/MQTT/trackers, and Cloudflare TURN includes 1 TB/month free. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §5 |
| F-027 | settled | IPv6 direct connectivity is a viable opportunistic path (CGNAT does not block it) but only ~40–50% of users have it, so it is a bonus rung, not a plan. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §5 |
| F-028 | settled | Android 17 (targetSdk 37) makes `ACCESS_LOCAL_NETWORK` a runtime permission enforced in the network stack for all LAN traffic, not just mDNS. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §5 |
| F-029 | settled | `NsdManager.resolveService` is deprecated in API 34 and single-flight; resolving every `onServiceFound` yields `FAILURE_ALREADY_ACTIVE`. Use `registerServiceInfoCallback` or the discovery picker. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §5 |
| F-030 | settled | macOS 15+ gates Bonjour behind local-network TCC; missing `NSLocalNetworkUsageDescription`/`NSBonjourServices` can silently deny discovery. LaunchAgents are subject; root daemons and Terminal-launched binaries are exempt. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §5 |

## Identity, policy, and impersonation

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-031 | settled | Impersonation splits into three gates: cosmetic (BLE/mDNS/model strings — trivial but buys nothing), protocol interop (feasible), and account identity (IDS/iCloud/Apple-signed certs — infeasible). | [V] | `docs/PLATFORM-WORKAROUNDS.md` §9 |
| F-032 | settled | AirDrop "Everyone" mode has no hardware attestation, so non-Apple devices can interoperate; Google ships this in Quick Share. "Contacts Only" requires an Apple-account certificate and validation record. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §9.2 |
| F-033 | settled | Borrowed Apple identity is not sustainable: Beeper Mini worked for days then was blocked server-side; IDS needs server-side validation data and Apple ID sign-in needs Anisette bound to real Apple hardware. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §9.4 |
| F-034 | settled | The full macOS feature set cannot ship on the Mac App Store: Accessibility control is incompatible with App Sandbox, which every MAS app must use. Direct distribution with Developer ID plus notarization is the path. | [V] | `docs/SEAMLESS-CONNECTIVITY.md` §10 |
| F-035 | settled | Splitting by direction makes an App Store edition possible: controlling the *phone* needs only Bluetooth, while controlling the *Mac* needs Accessibility. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §9; `F-012` |
| F-036 | settled | A trusted USB pairing to an iPhone yields backups (and therefore SMS/call history by parsing), syslog, and Darwin notifications by name — but no live call state, no SMS send, and no call control. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §9.6 |
| F-037 | settled | A third-party AirPlay 2 video receiver is not available: there is no public API, receiver approval is MFi-gated, and shipping receivers are unlicensed AirPlay-1-class implementations capped at 1080p with ~150 ms latency. | [V] | `docs/PLATFORM-WORKAROUNDS.md` §1 |

## Rules of engagement with Apple platforms

| ID | Status | Conclusion | Grade | Source |
| --- | --- | --- | --- | --- |
| F-038 | disproven | "An Android phone can join Continuity/Handoff as a peer." It cannot: session keys come from iCloud Keychain material (`RPIdentity-SameAccountDevice`) plus APNs-brokered pairing. | [V] | `F-031`, `F-033` |
| F-039 | disproven | "MediaProjection or `REMOTE_SUBMIX` can capture a cellular call." No: `CAPTURE_AUDIO_OUTPUT` is signature-only and playback capture excludes voice communication. | [V] | `F-006` |
| F-040 | disproven | "A Mac can be woken by a BLE peripheral." No: macOS wakes only for Bluetooth HID devices. | [V] | `F-017` |
| F-041 | disproven | "A third-party Mac app can be an AirPlay 2 receiver." No public API exists, and receiver trust is MFi-gated. | [V] | `F-037` |
| F-042 | disproven | "iOS can be made to act as Android to unlock a privileged bridge." There is no Android-side privileged gateway; the direction has no capability to unlock. | [A] | `docs/PLATFORM-WORKAROUNDS.md` §9.5 |
