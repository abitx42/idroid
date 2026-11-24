# Calls on the Mac from a phone: feasibility research

**Status:** research note only. No call, audio, or telephony code exists in this
repository, and nothing here changes the milestone's non-goals. This document
records what is achievable inside platform rules so a future `call` feature is
scoped against reality instead of against Apple's Continuity marketing.

**Question.** iDroid already carries `video` and `input` between Android and
macOS. Can it also carry a *phone call* — answer, dial, and hear/talk — on the
Mac, the way "Calls on Other Devices" does for the iPhone?

**Short answer.** Partly, and differently per phone OS:

- **iPhone → Mac: not possible for a third-party app.** Apple's relay depends on
  private frameworks and entitlements that are not grantable to third parties.
- **Android → Mac: possible for control, and possible for audio only through
  Bluetooth HFP** — with the caveat that macOS's public HFP API reportedly does
  not open the SCO audio channel, so audio may require an external Bluetooth
  endpoint (see §4).
- **Neither platform allows an app to capture cellular call audio directly.**
  Any design that assumes "record the call and stream it" is dead on arrival.

## 1. How Apple actually does it

Apple documents the *requirements*, not the mechanism: same Apple Account and
FaceTime, same Wi-Fi, "Calls on Other Devices" enabled, and macOS 26's Phone app
([Apple Support 102405](https://support.apple.com/en-us/102405)). The security
guide states the mechanism:

> "When an incoming call arrives, all configured devices are notified using
> APNs… the audio is seamlessly transmitted from the user's iPhone using a
> secure peer-to-peer connection between the two devices… ringing of nearby
> iCloud-paired devices is terminated by briefly advertising using Bluetooth
> Low Energy."
> — [Apple Platform Security](https://support.apple.com/en-us/guide/security/sec28a79bf17/web)

So: APNs for signaling, an Apple-internal peer-to-peer audio channel, BLE only
to stop the ringing. Not Bluetooth HFP, and not a public API surface.

The pieces a third party would need are private:

| Piece | Status |
| --- | --- |
| `TelephonyUtilities.framework` (`TUCall`, `TUCallCenter`, `TUDialRequest`, `TUCallHistoryController`) | Private framework; XPC peer is `callservicesd`, which enforces restricted entitlements `access-calls` / `modify-calls`. |
| IDS (identity service), AVConference, AWDL channels | Private. |
| Call history store `CallHistoryDB/CallHistory.storedata` | Direct Core Data access; not a supported API. |

There is exactly one public third-party demonstration of the private path
([tucall](https://github.com/DinosaurPotato534/tucall)), and its own README says
it needs SIP disabled and `amfi_get_out_of_my_way=0x1` to survive AMFI. That is
not shippable software: App Store Review Guideline
[2.5.1](https://developer.apple.com/app-store/review/guidelines/) requires public
APIs, and the Developer Program License Agreement forbids private API use
outright. **Do not plan against this path.**

The supported Apple-side alternative is a *different product*: a VoIP app of
your own, using CallKit (`CXProvider`, `CXCallController`) + PushKit VoIP pushes
+ `AVAudioSession` in `.voiceChat` mode, optionally as the user's default calling
app (`com.apple.developer.calling-app`, iOS 18.2+). It gives a native call UI and
your own audio path — it does not touch the iPhone's SIM. Practical constraint:
CallKit is available on iOS/Mac Catalyst, **not** native macOS
(`CXProvider` is `API_UNAVAILABLE(macos)`), so a Mac Catalyst app is required
for Apple's call UI on the desktop.

## 2. What Android permits

Android's Telecom framework is the only sanctioned route
([overview](https://developer.android.com/develop/connectivity/telecom)):

- **`ConnectionService`** — self-managed variant (`MANAGE_OWN_CALLS`) for your own
  VoIP calls; managed variant to appear in the user's Phone app.
- **`InCallService` + `RoleManager.ROLE_DIALER`** — the only way to see, answer,
  and hang up *cellular* calls. It yields call state, caller ID, and audio-route
  control (`getCallAudioState()`, `requestBluetoothAudio()`), which is UI and
  routing authority.
- Companion/wearable devices can observe calls with `MANAGE_ONGOING_CALLS` via
  `CompanionDeviceManager`.

What it never yields is the audio. `VOICE_CALL`, `VOICE_UPLINK`,
`VOICE_DOWNLINK`, and `REMOTE_SUBMIX` all require `CAPTURE_AUDIO_OUTPUT`, which
is "reserved for use by system components and is not available to third-party
applications"
([AudioSource](https://developer.android.com/reference/android/media/MediaRecorder.AudioSource)).
`MediaProjection`/`AudioPlaybackCapture` only captures `USAGE_MEDIA/GAME/UNKNOWN`
playback, never `USAGE_VOICE_COMMUNICATION`.

**Consequence for iDroid:** an Android companion app can be a legitimate
dialer-role *controller* (ring on the Mac, answer on the Mac, DTMF, hang up,
contact lookup) while the audio stays on the phone — which is exactly what KDE
Connect does, and why KDE Connect has never shipped call audio.

## 3. The audio path that does work: Bluetooth HFP

Hands-Free Profile has two roles: **AG** (the phone, owns the modem) and **HF**
(the headset/car kit, gets call control plus SCO audio). The phone routes
cellular audio to any bonded HF automatically. That is the mechanism every
shipping product uses:

- **Microsoft Phone Link** requires a Bluetooth pairing between PC and phone and
  states it does not relay a call onward to a headset — the PC is effectively the
  HF endpoint.
- **Android Auto / Automotive** — the head unit is the HF.
- **Linux (BlueZ/oFono + PipeWire)** has the most complete open implementation of
  a desktop HF with working bidirectional SCO audio.

On macOS the public API is `IOBluetoothHandsFreeDevice` (macOS 10.7+):
`dialNumber`, `acceptCall`, `acceptCallOnPhone`, `endCall`, `sendDTMF`,
`currentCallList`, `subscriberNumber`, `sendATCommand`, and
`transferAudioToComputer()` / `transferAudioToPhone()`. The inherited
`IOBluetoothHandsFree` also exposes the SCO controls that decide this feature:
`connect()`, `connectSCO()`, `disconnectSCO()`, `isSCOConnected()`. Sandbox
entitlement: `com.apple.security.device.bluetooth`. `CoreBluetooth` is GATT-only
and `ExternalAccessory` is MFi-only, so neither is a substitute.

Note what these semantics already give the product for free: `acceptCall()`
answers into the hands-free endpoint, while `acceptCallOnPhone()` answers and
leaves audio on the phone — which maps directly onto the
`HfpAudio` / `CompanionControl` backend choice in §5.

## 4. The open risk that decides the design

Multiple independent community projects report that on current macOS,
`IOBluetoothHandsFreeDevice` performs the HFP service-level connection and AT
command control (dial, answer, caller ID, DTMF) but **refuses to open the SCO
audio link**, returning `kIOReturnUnsupported`
([handsfree](https://github.com/jensb1/handsfree),
[BlueBridge](https://github.com/geosub-m/BlueBridge),
[phonebt](https://github.com/icoainc/phonebt)). One project worked around it by
putting an ESP32 in the middle as the real HF and letting the Mac own only the
microphone/speaker path.

This is **not** documented by Apple and is not reproducible in this repository.
Treat it as the single highest-risk unknown: it decides whether "talk on the Mac"
is a 2-week feature or requires shipping extra hardware.

**Spike required before any commitment:** bond a phone to this Mac, drive
`IOBluetoothHandsFreeDevice`, place a call, and record whether `connectSCO()`
succeeds and `isSCOConnected()` becomes true, and whether the resulting audio
route appears as a CoreAudio device with both an input and an output. Record the
`IOReturn` from `connectSCO()` when it fails — `kIOReturnUnsupported` is the
reported failure signature. Classification: `PASS` (SCO audio device appears and
is bidirectional) / `CONTROL_ONLY` (`endCall`/`acceptCall`/`sendDTMF` and caller
ID work, no audio) / `FAILED` (no HFP service-level connection at all). Preserve
the macOS build, phone model, and returned status as evidence, per
`docs/DECISION-GATES.md` §6.

## 5. How this would fit the existing contract

The control plane already models exactly this shape — a feature with an ordered
list of backends and a consent-gated fallback
(`docs/CONTROL-PLANE-V1.md`). A `call` feature would add:

| Aspect | Design |
| --- | --- |
| `Feature::Call` | New variant alongside `Video`/`Input` in `core/src/control_plane.rs`. |
| Backends | `HfpAudio` (Mac is HF; audio on Mac), `CompanionControl` (dialer-role control only, audio on phone), and on the Apple side nothing but `VoipOwn` for iDroid's own calls. |
| Capability reasons | `needs_user_action` for Bluetooth pairing and for the Android dialer role; `unavailable` for a phone whose OS blocks both. |
| Media session | Reuse `MediaSessionMachine` with `Feature::Call`: `Switching` already models "audio moved from phone speaker to Mac". |
| Link health | Existing 3 s/6 s heartbeat is adequate for control; SCO loss must map to `BackendChanged(HfpAudio → phone_speaker)` rather than a silent mute. |
| Security | Call state is the most sensitive metadata in the product: caller identity, timing, and live audio. It must not enter `ControlPlaneEventLog`, which today logs only transition names and kinds — keep it that way, and never log numbers or contacts. |
| Consent | Answering a call from the Mac is a privileged action: require explicit per-device authorization, revalidated after reconnect, per `THREAT-MODEL.md` invariant 4. |

Audio constraints to state up front: HFP SCO is 8/16 kHz mono (CVSD/mSBC), so
call audio on the Mac will sound like a car kit. That is a platform property, not
an iDroid defect, and it must not be sold as "high-fidelity".

## 6. Ranked options

1. **Companion control only (Android).** `InCallService` with the dialer role:
   ring, answer, hang up, DTMF on the Mac; audio stays on the phone. Zero
   compliance risk, ~1–2 weeks on top of the existing control plane, and useful
   on its own. **Recommended first step.**
2. **Mac as Bluetooth HF (Android and iPhone).** `IOBluetoothHandsFreeDevice` +
   the spike in §4. If SCO works, this is the real feature for both phones with
   no private APIs. If it does not, the fallback is an external BT endpoint,
   which changes the product from "an app" to "an app plus hardware".
3. **iDroid's own VoIP calling.** CallKit/VoIP on Apple platforms, self-managed
   `ConnectionService` on Android, WebRTC media in the existing native pipeline.
   Large, but the only fully owned, fully cross-platform call experience — and
   the only one that would ever work iPhone → Mac.
4. **Private Continuity APIs. Never.** App Store rejection, OS-update fragility,
   and a system-wide security downgrade.

## 7. What would have to change in this repository

- `Feature` is a closed two-variant enum (`Video`, `Input`) used by capability
  snapshots, the selection algorithm, and the media state machine. Adding `Call`
  is a versioned contract change: bump `CONTROL_PROTOCOL_VERSION_V1`, keep the
  highest-common-version negotiation meaningful, and add contract vectors for an
  old peer that does not advertise `Call`.
- The capability snapshot's `MAX_FEATURE_BACKENDS = 2` already fits two call
  backends, but `Backend::supports()` must gain the new pairings.
- `protocol.rs` (binary V1) has no call message kinds and should not grow them
  while the control plane still lacks a wire encoding; adding kinds to both
  would deepen the duplication described in §3 of `ARCHITECTURE.md`.

## 8. Open questions

- Does SCO open on this macOS build, or only AT control? (§4 spike)
- Does Bluetooth pairing on macOS give a stable identity that can be bound to the
  iDroid device record, or is it a separate OS-level trust the user must manage?
- What does the Play Store require of an app that requests the dialer role, and
  does a remote-control use case satisfy it?
- Is `MANAGE_ONGOING_CALLS` / companion association enough to avoid the dialer
  role entirely for a phone the Mac only observes?
