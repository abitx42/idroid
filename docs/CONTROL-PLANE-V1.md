# Android/macOS control plane v1

**Status:** typed contract and deterministic state-machine implementation in
`core/src/control_plane.rs`.

This contract makes the Android application the single peer of the Mac. A
shell/helper process is an Android-side backend behind that application. It has
no device identity, pairing relationship, or direct Mac control channel. If it
dies, the Android app reports the capability change and its watchdog may
restart it.

The contract assumes an authenticated, ordered, message-framed transport that
reports the peer's key fingerprint. It does not implement transport, wire
encoding, cryptography, key storage, or platform permissions.

## Messages

There are nine contract rows. `Error` and `Bye` are separate terminal message
forms under the final row and therefore have separate message kinds in the
typed API.

| Message | Direction | Fields |
| --- | --- | --- |
| `Hello` | both | `version_min`, `version_max`, `device_id`, `role`, `app_version`, optional `last_session_id` |
| `HelloAck` | both | chosen `version` |
| `Capabilities` | Android app → Mac | complete per-feature backend snapshot |
| `CapabilitiesUpdate` | Android app → Mac | replacement snapshot, sent whenever a capability changes |
| `SessionRequest` | Mac → Android app | non-zero `request_id`, `feature`, optional `preferred_backend` |
| `SessionResult` | Android app → Mac | request ID, feature, and `accepted`, `needs_user_action`, or `rejected` outcome |
| `Heartbeat` | both | non-zero `seq`, `link_state`, optional active backend selection |
| `BackendChanged` | Android app → Mac | feature, `from`, `to`, and reason |
| `Error` / `Bye` | both | error/close code, retryability, and optional user action |

`request_id` correlates every `SessionResult` with exactly one request. A
`CapabilitiesUpdate` is a complete replacement rather than a patch, so the Mac
can safely discard stale backend entries.

### Identity and version rules

- `Hello.device_id` is a 32-byte public-key fingerprint, not an IP address or
  a display name.
- The value must exactly equal the fingerprint reported by the authenticated
  transport. A mismatch produces `Error(identity_mismatch)` and the adapter
  must disconnect immediately.
- Version negotiation chooses the highest value in the two inclusive version
  ranges. No overlap produces `Error(version_unsupported)` and disconnect.
- `last_session_id` is the Android/Mac session identity the peer wants to
  resume. The link state machine preserves it across reconnect and rejects a
  different ID during a resumed handshake.

## Capabilities

The v1 feature set is exactly `video` and `input`.

| Feature | Backends |
| --- | --- |
| Video | `helper`, `mediaprojection` |
| Input | `helper`, `accessibility` |

Each feature lists its backends in preference order. A backend is:

- `available` with no reason;
- `needs_user_action` with a reason that maps to a user action; or
- `unavailable` with a reason code.

The selection algorithm tries an explicitly preferred backend first, then the
advertised order. An available fallback wins over a backend that would require
a prompt. If no backend is available, the result is `needs_user_action` when a
prompt can make one usable, otherwise `unavailable`.

The helper is therefore advertised by the Android app as a normal backend. The
Mac never authenticates or connects to the helper as a second peer.

## Link state machine

```text
Searching → Connecting → Handshaking → Ready → Degraded → Lost → Searching
```

- Three missed one-second heartbeats (`3 s`) moves `Ready` to `Degraded`.
- Six missed heartbeats (`6 s`) moves `Degraded` to `Lost`.
- A heartbeat received in `Degraded` returns the link to `Ready`.
- Any network-change event immediately moves the link to `Searching`; when it
  starts from `Ready`, both `Ready → Degraded` and `Degraded → Searching` are
  observable.
- A heartbeat does not revive `Lost`; the transport must reconnect and finish
  a new handshake.

The Mac supplies every timestamp using one monotonic clock. The v1 reconnect
metric measures from leaving `Ready` until returning to `Ready`, including time
spent in `Degraded`, `Lost`, `Searching`, `Connecting`, and `Handshaking`. A
successful resumed handshake reports the preserved session ID and the completed
reconnect time.

The core exposes a small `Clock` trait and `ManualClock` for deterministic
tests. Rust 1.85's Darwin standard-library source selects
`CLOCK_UPTIME_RAW` for `std::time::Instant`, confirming that `Instant` is not
safe for this metric. The macOS adapter must implement `Clock` with
`clock_gettime(CLOCK_MONOTONIC, ...)` (or an equivalent sleep-inclusive
continuous source). Darwin's `clock_gettime(3)` documentation says
`CLOCK_MONOTONIC` continues while the system is asleep; `CLOCK_UPTIME_RAW`,
which corresponds to `mach_absolute_time()` after conversion, does not. Rust's
default `Instant` implementation must not be assumed to be sleep-inclusive on
macOS. No wake-time product measurement is claimed until a physical sleep/wake
probe has been recorded.

`ControlPlaneEventLog` records link transitions and message kinds with the
injected host clock. It intentionally omits message payloads so diagnostic logs
cannot accidentally retain pairing or application data. Its stable NDJSON form
is suitable for the reconnect harness:

```json
{"at_ms":1000,"event":"link_transition","from":"ready","to":"degraded"}
{"at_ms":1000,"event":"link_transition","from":"degraded","to":"searching"}
{"at_ms":1001,"event":"message","kind":"hello"}
```

The manual physical clock probe is
[`experiments/mac-clock-sleep/run.sh`](../experiments/mac-clock-sleep/run.sh).
It compares `CLOCK_MONOTONIC` and `CLOCK_UPTIME_RAW` around an actual
`pmset sleepnow` cycle and requires an interactive administrator authorization.

## Media session state machine

Each requested feature has its own small state machine:

```text
Idle → Starting → Active → Switching → Ended
```

`Starting` may carry an awaited user action for the initial request.
`Switching` carries the old backend, target backend, and optional awaited user
action. For example, when the helper dies:

1. Android sends `CapabilitiesUpdate` with helper unavailable.
2. Android sends `BackendChanged(helper → mediaprojection, helper_lost)`.
3. The session enters `Switching(awaiting_user=grant_media_projection)`.
4. After the consent callback, the session becomes `Active(mediaprojection)`.

The UI must show this as a consent-gated fallback rather than a hung session.

## Error codes

`version_unsupported`, `identity_mismatch`, `not_paired`,
`backend_unavailable`, `user_action_required`, `permission_denied`,
`helper_lost`, `timeout`, and `internal` are the complete v1 error vocabulary.

`Error` and `Bye` carry `retryable` and an optional `user_action`. A normal
`Bye` has no error code, is not retryable, and has no user action.

## Executable test vectors

The public integration tests in `core/tests/control_plane.rs` cover:

1. highest-common-version negotiation;
2. no-overlap and identity-mismatch failures;
3. helper-first video selection;
4. live capability replacement and MediaProjection fallback;
5. helper loss entering `Switching(awaiting_user)`;
6. a four-second Wi-Fi reconnect that preserves `session_id` and avoids `Lost`;
7. reconnect accounting that includes a real `Degraded` interval;
8. heartbeat timeout through `Degraded` to `Lost`; and
9. capability change without link reconnect;
10. independent helper/backend and link failures;
11. immediate network-change transition to `Searching`; and
12. validation, stable kinds, and injected-clock event logging for every
    control message.

These are contract/state tests, not evidence that Android, macOS, ADB, or a
real network implementation works.

## Reproduction

From the repository root, with the pinned `rust-toolchain.toml` selected:

```sh
rustc --version
# rustc 1.85.0 (4d91de4e4 2025-02-17)
make test
make lint
```

`make test` runs `cargo test --locked --offline --workspace --all-targets`.
`make lint` runs the formatter check and Clippy with warnings denied. The
current verification run used Rust `1.85.0` and passed 51 tests (17 unit,
12 control-plane integration, 13 discovery/pairing integration, 3 connection
arbitration integration, 5 existing core integration, and 1 C-ABI integration
test).
