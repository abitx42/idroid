# Discovery and pairing v1

**Status:** deterministic portable-core contracts and fakes. Real Bonjour,
gateway sockets, TLS, Android NSD, BLE, and platform key stores are not yet
implemented.

The Android application remains the single peer of the Mac. Discovery produces
address hints; the authenticated certificate fingerprint decides identity.
The helper is not a discovery peer.

## Pairing

The Mac creates a one-time QR payload:

```text
{ version, mac_cert_fingerprint, one_time_token[16], expires_at, service_hint }
```

The core contract enforces:

- a non-zero 128-bit token;
- a two-minute expiry window;
- at most three failed token attempts;
- listener validity only while the QR token is active;
- explicit listener closure when the QR screen is dismissed;
- exact Mac certificate-fingerprint pinning before the phone sends the token;
- single-use token consumption; and
- persistent peer records containing fingerprint, name, paired-at timestamp,
  and local revocation state.

`PairingStore` is the persistence boundary. `InMemoryPairingStore` is a test
fake only. A production implementation must use the platform secure stores and
must perform mutual TLS outside this module.

The phone treats `expires_at` as display metadata; the Mac's active listener
clock is authoritative for expiry. This avoids making token acceptance depend
on synchronized phone and Mac clocks.

`forget` deletes the trust record. `revoke` keeps a negative record so a
reconnecting peer is rejected as `not_paired` at the control-plane UI boundary.
Unpaired and revoked reconnects never become authenticated race winners.

## Discovery candidates

`CandidateSource` implementations produce validated candidates:

| Source | v1 | Contract |
| --- | --- | --- |
| Cached address | yes | keyed by target fingerprint and network identity |
| Bonjour/mDNS | yes | `_acontinuity._tcp`; service data remains only an address hint |
| Gateway probe | yes | default-gateway address for hotspot topologies |
| BLE | later | reserved source kind; no v1 implementation |

Each candidate contains address, port, source, network identity, and an optional
expected peer fingerprint. A candidate is never trusted merely because it came
from cache, mDNS, or the gateway.

`DiscoveryRace` starts the supplied sources, records every candidate, and then
tries candidates until the first authenticated result matches the requested
fingerprint. A mismatch is logged and the next candidate is tried. A network
change increments the race generation, clears all candidates, records
`NetworkChanged`, and makes the previous generation unusable.

The v1 Bonjour advertisement does not contain a stable device identifier;
service names and addresses are hints only. A rotating privacy-preserving tag
is a v2 item.

## Connection arbitration

Both peers may listen and dial. After TLS identity and the Hello fingerprint are
validated, `ConnectionArbiter` admits at most one active connection per peer:

- opposite-direction simultaneous dials keep the connection whose initiator has
  the lower certificate fingerprint;
- duplicate paths with the same initiator keep the first admitted connection;
- a replaced or cancelled loser is closed without being reported as a new
  `ConnectFailed` event; and
- a stale loser close cannot remove the surviving connection.

The arbiter runs after authentication, so an address or transport path never
changes peer identity.

The race records:

- time to first candidate;
- time to authenticated handshake; and
- winning source.

The timestamps come from the injected Mac-side `Clock`.

The link-integrated race moves `Searching → Connecting` before launching
transport attempts. An authenticated winner moves the link to `Handshaking`,
where the control-plane adapter continues with Hello/version/session validation
and eventually calls `handshake_ready`. If all attempts fail, the link returns
to `Searching`. Those transitions are written to the same event log as the
candidate and handshake events.

## Event vocabulary

The control-plane event log now supports:

```text
NetworkChanged
CandidateFound { source }
ConnectStarted
ConnectFailed { reason }
HandshakeOk
PairingTokenIssued
PairingTokenConsumed
PairingTokenExpired
```

Event payloads are deliberately limited to source, reason, kind, and timing;
addresses, tokens, certificates, and application data are not serialized into
the NDJSON diagnostics.

## Android permission findings

The official Android documentation confirms:

- Apps targeting Android 13/API 33+ that manage Wi-Fi connections should
  request `NEARBY_WIFI_DEVICES`; older compatibility paths may require
  `ACCESS_FINE_LOCATION` for APIs such as Wi-Fi scans.
- Android NSD is the supported DNS-SD API for registering, browsing, resolving,
  and receiving service addresses.
- Android 16 local-network protection is opt-in. The documented opt-in uses
  `NEARBY_WIFI_DEVICES` to restore local-network traffic.
- Android 17 makes local-network protection mandatory for apps targeting SDK
  37+. Broad persistent local discovery then needs `ACCESS_LOCAL_NETWORK`, or
  an app may use the system NSD picker where that user-mediated flow fits.

Therefore v1 onboarding must treat Nearby-device/local-network access as an
explicit capability and must surface denial. It must not claim that
`NEARBY_WIFI_DEVICES` alone is the permanent Android 17 answer for broad
background mDNS. A platform adapter may also need a multicast lock while it is
actively receiving multicast traffic; that behavior is not part of the core
contract.

Sources:

- [Android NSD](https://developer.android.com/develop/connectivity/wifi/use-nsd)
- [Nearby Wi-Fi permission](https://developer.android.com/develop/connectivity/wifi/wifi-permissions)
- [Local network permission](https://developer.android.com/privacy-and-security/local-network-permission)

## Deterministic test vectors

`core/tests/discovery_pairing.rs` covers:

1. valid token and matching QR fingerprint pair successfully and burn the token;
2. expired/reused tokens are rejected without storing a peer;
3. invalid token attempts are limited and close the listener;
4. a Mac certificate mismatch in the QR aborts phone pairing;
5. a cached-address fingerprint mismatch falls through to the next candidate;
6. two paired Macs are selected by the requested certificate fingerprint;
7. mDNS failure still allows a gateway-probe winner;
8. network change invalidates the old race and starts a new generation;
9. an unpaired reconnect maps to `not_paired`;
10. a revoked peer is rejected and maps to `not_paired`;
11. the pairing listener closes after QR expiry;
12. an authenticated winner advances the link to `Handshaking` and logs it; and
13. a failed race returns the link to `Searching` and logs it;
14. simultaneous opposite dials use the lower-fingerprint tie-break;
15. duplicate paths retain the first connection; and
16. a cancelled race loser is not logged as a spurious connection failure.

These tests use no sockets, multicast, TLS, QR renderer, BLE, Android API, or
secure storage. They prove contract behavior only.
