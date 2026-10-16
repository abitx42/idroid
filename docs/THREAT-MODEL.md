# iDroid milestone threat model

**Scope:** portable core plus the planned Android/macOS integration boundary
**Security posture:** the current core supplies validation seams and explicit
non-cryptographic fixtures; it is not a production trust system.

This document prevents an API scaffold from being mistaken for a security
feature. A control or display product handles device identity, screen/input
data, credentials, and authority to act on a user’s machine. The production
integration must turn the boundaries below into enforced controls before a
real-device pilot is called secure.

## 1. Assets and security objectives

| Asset | Security objective | Failure impact |
| --- | --- | --- |
| Device identity and pairing credentials | Authenticity, confidentiality, revocation, bounded lifetime | An attacker impersonates a phone or host. |
| Session identity/state | Integrity, uniqueness, correct ownership, recovery without resurrection | Input or data reaches a superseded/wrong device, or a terminated session returns. |
| Screen, input, clipboard, and file data | Confidentiality and integrity; minimum necessary exposure | Private content leaks or an attacker alters a user action. |
| Control authority and capabilities | Explicit authorization, least privilege, user consent | A connected peer controls the wrong resource or action. |
| Protocol availability | Bounded work, memory, and cleanup under hostile input | A peer exhausts CPU/memory or wedges recovery. |
| Audit and diagnostic records | Useful accountability without secrets or sensitive payloads | Incidents cannot be investigated, or logs become another leak. |
| Build and release artifacts | Integrity and reproducibility | A compromised dependency or bundle ships with broader authority. |

## 2. Actors and assumptions

| Actor | Capability/intent | Trust assumption |
| --- | --- | --- |
| Legitimate user/operator | Pairs devices, grants/revokes OS permissions, initiates control | User intent is authoritative only after an explicit, visible consent flow. |
| Remote network attacker or MITM | Reads, drops, reorders, replays, modifies, or floods transport traffic | Never trusted; transport may be hostile even on a local network. |
| Malicious or compromised peer | Possesses a valid but abused credential, or tries unauthorized capabilities | Identity alone is not authorization; capability policy is required. |
| Malicious local process | Attempts to access sockets, IPC, logs, key material, or input authority | OS permissions and key storage must enforce the boundary. |
| Compromised Android app/device or macOS host | Can observe or tamper with data inside that platform | The product cannot make a compromised OS trustworthy; minimize authority and disclose the limit. |
| Resource-exhaustion sender | Sends malformed, oversized, concurrent, or slow traffic | Input and queues must be bounded and cancellable. |
| Supply-chain/release attacker | Alters dependencies, build inputs, or signed artifacts | Pinned/reviewed dependencies and signing provenance are required. |

The threat model does not assume a secure LAN, a trusted ADB endpoint, a
non-jailbroken device, or that a user’s OS permission remains unchanged after
pairing.

## 3. Trust boundaries and current enforcement

```text
   Android/macOS OS + permissions
              |
     platform adapter / transport       (future, untrusted input)
              |
       [frame bytes + credentials]
              v
  +-------------------------------+
  | idroid-core                   |
  | decode/validate -> lifecycle  |
  | device registry -> policy     |
  +-------------------------------+
       |                 |
       v                 v
 security provider     app/UI/telemetry
 (future trust)        (future policy)
```

Current controls in the core:

- `#![forbid(unsafe_code)]` applies to the crate;
- zero device/session IDs are rejected;
- protocol magic, version, kind, exact lengths, reason values, non-empty data,
  and the 64 KiB frame bound are validated;
- malformed, truncated, trailing, unknown-version, unknown-kind, and oversized
  frames return errors rather than being accepted;
- session state transitions are explicit, one active session is tracked per
  device, duplicate starts terminate the prior session, and per-session time
  cannot move backwards; and
- test fixtures make rejection behavior deterministic without pretending to
  provide cryptography.

These are **input hygiene and state-integrity controls**, not authentication or
confidentiality. Structural validation must happen before expensive decoding or
application action, but it does not prove who sent a valid frame.

## 4. Current gaps and required mitigations

| Threat | Current status | Required production mitigation | Gate |
| --- | --- | --- | --- |
| Peer impersonation/pairing theft | `FixtureIdentityVerifier` only compares exact bytes; no cryptographic protocol or secure storage. | Use device-bound cryptographic identity, authenticated pairing, key rotation/revocation, and an explicit user-visible approval. | HG-4 |
| MITM/eavesdropping/tampering | `MessageAuthenticator` accepts an opaque tag, but no implementation is supplied; frame codec is plaintext. | Use authenticated encryption with endpoint binding and key lifecycle. Fail closed when the provider is unavailable. | HG-4 |
| Replay or reordering | Heartbeat sequence must be non-zero but is not checked for freshness; data has no nonce/order semantics. | Bind monotonically checked counters/nonces to an authenticated session and define reconnect/rekey behavior. Test replay, reorder, and duplicate frames. | HG-4/HG-5 |
| Unauthorized action | Device capabilities are descriptive bits; no policy evaluator or per-action authorization exists. | Map identity to an allowlist/capability policy, require consent for control, and re-check authorization after reconnect and permission changes. | HG-2/HG-4 |
| OS permission abuse | No Android or macOS adapter exists; the core cannot observe TCC, MediaProjection, Accessibility, or ADB consent. | Show permission state, fail closed on denial/revocation, minimize requested scopes, and test restart/revocation. | HG-2/HG-4 |
| Resource exhaustion | Frame payloads are bounded, but there is no transport rate limit, queue budget, concurrency limit, or CPU budget. | Bound all queues and sessions, rate-limit peers, cancel work, and measure memory/CPU under malformed/flood traffic. | HG-3/HG-5 |
| Session confusion after failure | In-memory manager handles duplicate and grace transitions; no durable epoch or cross-process ownership exists. | Bind session/connection epochs to authenticated peers, serialize lifecycle events, and define crash/restart invalidation. | HG-5 |
| Stale credentials or records | No persistence, expiry, revocation store, or secure deletion exists. | Implement bounded credential lifetime, revocation, encrypted storage, and stale-session cleanup with audit. | HG-4/HG-5 |
| Sensitive logs/telemetry | No logging policy is implemented. | Redact credentials, tags, raw screen/input/file data, and payloads; log identifiers/events with access controls and retention. | Security score |
| Malicious build/dependency | Core has no external runtime dependencies today, but release provenance is not implemented. | Review/pin dependencies, reproduce the Linux build, sign artifacts, and protect release credentials. | HG-1 / release review |

## 5. Security invariants for the integration

The platform and transport layers must preserve these invariants:

1. No display, input, clipboard, audio, or file action is performed before the
   peer is authenticated **and** authorized for that capability.
2. A successful protocol decode never implies authentication, authorization,
   user consent, or confidentiality.
3. Unknown, revoked, expired, unavailable, malformed, oversized, replayed, or
   out-of-policy input fails closed and produces a bounded, diagnosable error.
4. A reconnect cannot silently inherit authority that was revoked while the
   device was disconnected; identity, session epoch, and capability policy are
   revalidated.
5. Secrets are never placed in protocol error text, normal diagnostics, source
   files, fixtures used by release code, or evidence artifacts.
6. The core’s exact-byte fixtures remain test-only and are labelled as such in
   code and documentation. They cannot satisfy a security gate.
7. Platform permission revocation or app background/termination is treated as a
   loss of authority, not merely a transport disconnect.

## 6. Abuse cases to test

The security experiment must retain a failure result for each attempted abuse:

- modify magic, version, type, length, ID, reason, sequence, or payload in
  transit;
- replay a valid frame after disconnect/reconnect, after termination, and
  after credential revocation;
- present a valid credential for a different device or request a capability not
  allowed to that identity;
- flood with maximum-size frames, incomplete frames, trailing bytes, concurrent
  sessions, or repeated reconnect attempts;
- revoke Android/macOS permission while a session is active and while it is
  reconnecting;
- restart one side, duplicate a device start, or roll the clock backward; and
- inspect logs/crash artifacts for credentials, authentication tags, screen
  pixels, input payloads, or file contents.

The current tests cover structural malformed-frame rejection, ID validation,
state transition errors, and fixture mismatch. They do not cover the abuse
cases above across a real transport or OS boundary.

## 7. Security decision

The milestone may be described as a **portable core with security injection
points**, not as authenticated or encrypted remote control. Production security
is a hard gate in [`DECISION-GATES.md`](DECISION-GATES.md): a weighted score
cannot compensate for fixture-based identity, missing channel protection, or
uncontrolled OS permissions. See [`ARCHITECTURE.md`](ARCHITECTURE.md) for the
ownership of each boundary and [`EXPERIMENT-PLAN.md`](EXPERIMENT-PLAN.md) for
the negative-test sequence.
