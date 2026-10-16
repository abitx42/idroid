# iDroid milestone architecture

**Status:** engineering baseline for the portable-core milestone

This document describes what the repository can do today and the seams that the
Android and macOS products must fill next. “Implemented” means that the behavior
exists in `core/` and is covered by the current unit or integration tests.
“Scaffolded” means that an interface or data shape exists but a production
implementation is intentionally absent. “Unverified” means that no test or
real-device evidence is present in this repository.

The milestone is a **portable Rust core foundation**, not a working Android
application, macOS application, transport, or remote-control product. The
architecture keeps those platform concerns out of the core so that the protocol
and lifecycle rules can be tested on Linux and reused by both products.

## 1. System shape

The intended production flow is:

```text
 Android adapter/agent                 macOS adapter/host
 (capture, input, permissions)         (display, input, permissions)
          |                                     |
          | normalized events + opaque auth     |
          +-------------- transport ------------+
                             |
                    +--------v---------+
                    |  idroid-core     |
                    |                  |
                    | protocol codec   |
                    | device registry  |
                    | session manager  |
                    | security ports   |
                    +--------+---------+
                             |
                  application policy/UI/logging
```

Only the box labelled `idroid-core` exists in this milestone. The arrows are
integration contracts, not current executable paths. In particular, the core
does not open sockets, capture a screen, synthesize input, prompt for an OS
permission, or persist a session.

### Trust and ownership boundaries

1. **Platform boundary.** Android and macOS code owns OS APIs, permission
   prompts, lifecycle callbacks, screen capture, input injection, device
   discovery, and resource limits. The core accepts normalized values and must
   not assume that a platform callback is trustworthy merely because it came
   from an adapter.
2. **Transport boundary.** A future transport owns connection establishment,
   ordering, back-pressure, cancellation, and bytes in flight. The core codec
   parses a complete frame; it is not a stream reader and does not provide
   retransmission, encryption, or congestion control.
3. **Security-provider boundary.** `IdentityVerifier` and
   `MessageAuthenticator` are dependency-injected ports. A production provider
   owns cryptographic identity, key storage, credential lifetime, and message
   authentication. The core only receives the result or an error.
4. **Application-policy boundary.** The host application decides whether a
   verified device may perform a requested action. A valid frame is not an
   authorization decision, and a registered device is not automatically allowed
   to control a screen or transfer a file.
5. **Persistence boundary.** Current device and session state is process memory.
   A future persistence layer must define crash recovery, replay protection,
   deletion, and secret handling before it can be treated as part of the
   product.

## 2. Implemented core components

| Component | Current behavior | Evidence | Milestone status |
| --- | --- | --- | --- |
| `ids` | `DeviceId` and `SessionId` wrap non-zero `u64` values; fallible constructors reject zero. | `core/src/ids.rs` and callers/tests | **Implemented** |
| `device` | `DeviceProfile` describes phone, tablet, emulator, or other devices and a bitset of display, input, file-transfer, and debug-bridge capabilities. `DeviceRegistry` rejects duplicate IDs. | `core/src/device.rs`; integration test | **Implemented** |
| `device_state` | Orthogonal connection, capture, input, and permission dimensions plus a derived headline for Nearby, Pairing, Connecting, Connected, Sharing paused, Reconnecting, Offline, and Permission required. | `core/src/device_state.rs`; unit tests | **Implemented** |
| `session` | In-memory lifecycle for connected, temporarily disconnected, offline, and terminated sessions; heartbeat, reconnect grace, stale cleanup, duplicate replacement, and monotonic timestamps. | `core/src/session.rs`; unit/integration tests | **Implemented** |
| `protocol` | Strict V1 binary framing with a magic value, version, message kind, bounded length, lifecycle, pairing, status, input, permission, capture, error, and data bodies, encode/decode round trips, and malformed-frame errors. | `core/src/protocol.rs`; unit/integration tests | **Implemented** |
| `security` | Interfaces for identity storage/revocation, pairing nonces/codes, authenticated sessions, replay protection, and message verification plus deterministic exact-byte fixtures. | `core/src/security.rs`; tests | **Scaffolded; fixtures are not security** |
| crate boundary | Library exports the core types and forbids Rust `unsafe` code. | `core/src/lib.rs` | **Implemented** |

The core has no dependency on an async runtime, socket library, GUI toolkit,
Android SDK, Apple framework, JNI, or Objective-C runtime. This is intentional:
the foundation can be built and tested in a Linux environment without carrying
platform code into every consumer.

## 3. Protocol contract

The current codec represents one complete frame as follows:

| Offset | Size | Field | Current rule |
| ---: | ---: | --- | --- |
| 0 | 4 | magic | ASCII `IDR0`; any other value is rejected |
| 4 | 1 | version | `1` (`ProtocolVersion::V1`) only |
| 5 | 1 | message kind | Lifecycle/data plus pairing, status, input, permission, capture, and error kinds |
| 6 | 4 | payload length | little-endian `u32`; declared frame payload is at most 64 KiB |
| 10 | N | payload | message-specific bytes; the decoder rejects truncation and trailing bytes |

Message bodies currently contain:

- `Hello { device_id }`;
- `Heartbeat { session_id, sequence }`, with a non-zero sequence;
- `Disconnect { session_id, reason }`;
- `Reconnect { session_id }`;
- `Terminate { session_id, reason }`; and
- `Data { session_id, payload }`, with a non-empty bounded payload;
- `PairingInitiation` and `PairingCompletion`;
- `SessionEstablishment` and `DeviceStatus`;
- `TapInput`, `PointerMovement`, and `KeyboardInput`;
- `PermissionChange` and `CaptureState`; and
- `Error { session_id, code, message }`, with bounded UTF-8 text.

IDs are encoded little-endian as `u64` values. Reasons have explicit wire
values and unknown reason values are rejected. `ProtocolMessage::decode` does
not authenticate bytes, enforce authorization, deduplicate messages, or prove
that a sequence is newer than a previous heartbeat. A non-zero heartbeat
sequence is a structural invariant only. The future transport/security layer
must supply those semantics before a frame can drive a real device.

The 64 KiB limit is a frame-safety limit. A `Data` frame also carries an
8-byte session ID, so an adapter must leave room for framing overhead when it
chooses a data chunk size. No fragmentation or streaming contract exists yet.

## 4. Session state machine

The implemented state transitions are:

```text
Connected --disconnect / heartbeat timeout--> Disconnected
    ^                                           |
    | reconnect within grace                   | grace expiry
    +-------------------------------------------v
                                             Offline

Connected, Disconnected, or Offline --terminate / duplicate--> Terminated
Offline or Terminated --stale cleanup-------------------------> removed
```

The exact rules are:

- `start_session` validates the device ID, sweeps old state, allocates a new
  non-zero session ID, and marks it `Connected`.
- Starting a second session for the same device terminates the old session
  with `TerminationReason::Duplicate` and reports the superseded ID. This is
  true even when the old session is temporarily disconnected.
- `disconnect` is a resumable transition. It records `Requested` and retains
  the device-to-session association so that reconnect can use the same session
  ID. A heartbeat timeout uses `HeartbeatTimeout` instead.
- `reconnect` succeeds only from `Disconnected` and within the configured
  grace period. On success it clears the disconnect reason and refreshes the
  heartbeat timestamp. After the window, it marks the session `Offline` and
  returns `ReconnectWindowExpired`.
- `sweep` moves connected sessions past the heartbeat timeout to
  `Disconnected`, disconnected sessions past reconnect grace to `Offline`,
  and removes offline or terminated sessions past the stale period.
- `terminate` is final for that in-memory record. No heartbeat or reconnect is
  accepted after termination.
- Every per-session time comparison rejects a clock that moves backwards.
  `Timestamp` is supplied by the caller; the core does not read a wall clock.

The default configuration is a 30-second heartbeat timeout, 15-second
reconnect grace, and 5-minute stale period. These are code defaults, not a
claim that they are suitable for every network or device. There is no durable
session resume across process death, device reboot, or application restart.

## 5. Security integration point

The security module deliberately stops at narrow traits:

```text
presented identity/credential ---> IdentityVerifier ---> VerifiedIdentity
pairing nonce/code --------------> PairingProvider
frame bytes + opaque tag --------> MessageAuthenticator -> accepted/rejected
session transcript --------------> SessionAuthenticator
identity/session/sequence -------> ReplayGuard
```

`DeviceIdentityStore`, `PairingProvider`, `SessionAuthenticator`, and
`ReplayGuard` describe production ownership boundaries. They do not perform
cryptography. `FixtureIdentityVerifier` compares bytes in a map and
`FixtureMessageAuthenticator` compares an expected tag. They are deterministic
test fixtures, not cryptography, key derivation, encryption, certificate
validation, replay protection, or authorization. `ProtocolMessage` does not
call any security trait automatically, so the application must establish the
order of operations and fail closed when verification is unavailable.

The production boundary must eventually bind a verified identity to a device
record, negotiate an authenticated and encrypted channel, apply an explicit
capability policy, and make revocation/expiry observable. See
[`THREAT-MODEL.md`](THREAT-MODEL.md) for the required controls and current
gaps.

## 6. Platform adapters and constraints

### Android

The Android side is a future adapter or companion agent. It must own device
discovery, user-approved pairing, screen capture, input delivery, foreground
service/background limits, battery/thermal behavior, and OEM variation. A
typical implementation will need to account for `MediaProjection` consent for
screen capture, the restrictions and user consent associated with
`AccessibilityService` or another input path, and—if ADB is used—developer
options, USB debugging, host-key authorization, and transport loss. No root or
silent input authority can be assumed. The core currently contains no Android
target, JNI/NDK bridge, Gradle project, capture pipeline, or input adapter.

### macOS

The macOS side is a future host adapter. Screen capture requires the user’s
Screen Recording permission; synthetic input requires Accessibility/Input
Monitoring approval and is subject to TCC state. A shippable app must also
decide its code-signing, hardened-runtime, notarization, Intel/Apple-Silicon,
network-firewall, and update strategy. The core currently contains no
ScreenCaptureKit/CGDisplayStream integration, `CGEvent` input path, Swift or
Objective-C UI, entitlement file, or signed bundle.

### Linux

Linux is the shared repository-health target, even though no Linux desktop
adapter is part of this milestone. A clean Linux checkout must be able to run
the pinned Rust toolchain and `cargo test --locked`. Platform work is not
allowed to make the portable core fail on Linux; a Linux build failure blocks
the milestone independently of the weighted product score. See
[`PLATFORM-SUPPORT.md`](PLATFORM-SUPPORT.md) and
[`DECISION-GATES.md`](DECISION-GATES.md).

## 7. Deliberate non-goals for this milestone

The following are outside the implemented foundation and must not be implied by
the public types:

- live display mirroring, input injection, clipboard, audio, or file transfer;
- peer discovery, pairing UX, sockets, WebRTC, USB/ADB transport, or retries;
- production identity, cryptography, TLS, key storage, authorization, or audit;
- persistent devices/sessions, crash recovery, multi-process locking, or
  cross-device conflict resolution;
- an Android APK, macOS application, permissions flow, or release packaging;
- a latency, throughput, thermal, battery, or memory claim; and
- support for iOS, Windows, or an arbitrary Android OEM/configuration.

Any implementation that crosses one of these boundaries must update the threat
model, platform matrix, experiment evidence, and decision gates before it is
called supported.

## 8. Repository health and verification

The repository pins Rust `1.85.0` in `rust-toolchain.toml` and carries a
`Cargo.lock`. The baseline check is:

```text
cargo test --locked
```

Run it from the repository root on Linux as the shared health check. The
current tests exercise IDs, registry behavior, protocol round trips and
malformed frames, session transitions, and the explicit non-cryptographic
fixtures. They do not prove Android/macOS behavior, network interoperability,
production security, or 240-fps responsiveness.
