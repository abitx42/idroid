# Session log

Append-only. One entry per working session, newest at the bottom. Keep it short:
what was asked, what was produced, which IDs were added.

## S-001 — 2026-11-12 — initial audit and call feasibility

**Asked:** analyse the whole repository and research how to build Mac↔phone
calling.

**Produced:** `docs/CALLS-CONTINUITY.md`. Verified baseline: 51 tests pass,
Clippy clean, `fmt` clean, no `unsafe` in `core/`.

**Findings:** F-001, F-002, F-006, F-037. **Audit:** A-001 – A-006, A-014 – A-020,
A-019.

**Left open:** whether `IOBluetoothHandsFreeDevice` can open SCO at all.

## S-002 — 2026-11-12 — seamless connectivity research

**Asked:** deeper research on how seamless cross-device connectivity is built.

**Produced:** `docs/SEAMLESS-CONNECTIVITY.md`.

**Findings:** F-017, F-022 – F-030. **Audit:** A-007 – A-012, A-014.

**Key conclusion:** Apple's seamlessness is account directory + push + P2P, not
better local networking. LAN-only is the honest ceiling without infrastructure.

## S-003 — 2026-11-12 — workarounds and bypasses

**Asked:** find loopholes, tricks, and ways around the blocking issues.

**Produced:** `docs/PLATFORM-WORKAROUNDS.md` §1–§8 (later extended); correction
added to `docs/SEAMLESS-CONNECTIVITY.md` §7 about free relay tiers.

**Findings:** F-003 – F-005, F-007 – F-016, F-018 – F-021, F-026. **Audit:** A-017,
A-018.

**Key conclusion:** HFP makes iPhone calls possible with zero iOS code; BLE HID
removes the Accessibility requirement for input.

## S-004 — 2026-11-12 — impersonation analysis

**Asked:** can iOS be made to look like Android, or Android like iOS?

**Produced:** `docs/PLATFORM-WORKAROUNDS.md` §9 (three gates, impersonation ledger,
Sleep Proxy, Beeper boundary, USB bridge).

**Findings:** F-031 – F-036, F-038 – F-042.

**Key conclusion:** cosmetic spoofing buys nothing; protocol interop is the real
door (AirDrop Everyone, Sleep Proxy); account identity is closed.

## S-005 — 2026-11-12 — knowledge base and agent instructions

**Asked:** make all research readable and continuously updated by any agent that
opens the folder, so nothing is researched twice.

**Produced:** `AGENTS.md`, `CLAUDE.md`, `research/` (this directory),
`scripts/research_check.py`, `make research-check`.

**Findings:** none new; entries F-001 – F-042 and A-001 – A-020 were consolidated
from S-001 – S-004.

**Decisions:** D-001 – D-008.

**Note for the next session:** start at `AGENTS.md` §1. If you add a finding,
update `research/index.json` and rerun the check.

## S-006 — 2026-11-24 — skill mirror and phase-0 hygiene

**Asked:** apply the staged plan, starting with repository hygiene, after making
the installed Antigravity skills available to OpenCode.

**Produced:** a global OpenCode skill mirror (`~/.config/opencode/skills`, 338
skills: 19 advertised, 319 hidden-but-loadable) plus a sync script and
documentation; removed the Instagram export experiment (`ebdcace`); pinned local
Makefile checks to the rustup toolchain and added `check-toolchain`; added
`SECURITY.md`; added `conventional-commits` and related engineering skills to
the advertised set.

**Findings:** none new. **Audit:** A-014 fixed, A-015 fixed, A-016 partially
addressed (`LICENSE` pending an owner decision).

**Left open:** license choice; Phase 1 (codec/identity unification) not started.

## S-007 — 2026-11-24 — Phase 1 protocol groundwork

**Asked:** continue phase by phase after Phase 0.

**Produced:** `FrameReader` in `core/src/protocol.rs` with chunk-boundary, partial
frame, invalid header, oversized length, and stalled-stream tests; installed nine
audited open-source skills globally (Rust best practices/review, cargo-fuzz,
Android Intent/JNI security, macOS SwiftPM packaging, threat modeling, TDD, and
a hidden Trail of Bits Rust security review).

**Findings:** none new. **Audit:** A-017 fixed; A-018 (real fuzzing) remains open.

**Left open:** wire-codec decision (D-008), identity unification (A-002), and
golden cross-language vectors (A-005).

## S-008 — 2026-11-25 — Phase 1 wire codec unification and dual licensing

**Asked:** check and execute next phase.

**Produced:** `ProtocolVersion::V2` and 10 control-plane message kinds framed in
`core/src/protocol.rs`; binary wire encoder/decoder in `core/src/control_wire.rs`;
C-ABI FFI codec in `core-ffi/src/codec.rs`; synchronized FFI headers in
`apps/mac-native/`; cross-language golden vectors in `tests/protocol/control-plane-v2.txt`
(`golden_vectors.rs`); decode safety & fuzzing tests in `core/tests/decode_safety.rs`;
dual licensing (`LICENSE-APACHE`, `LICENSE-MIT`) and README update; Makefile
`check-headers`.

**Findings:** none new.

**Decisions:** D-008 accepted.

**Audit:** A-001, A-002, A-005, A-016, A-018 fixed. Baseline now 80 tests.

**Left open:** Phase 2 platform slice integration (`apps/mac-native` and Android
vertical slice wiring up FFI codec), Android supervisor lifecycle (A-004, A-008,
A-010), and macOS app bundle (A-007).

## S-009 — 2026-11-25 — Phase 2 platform slice bundle & supervisor hardening

**Asked:** execute next phase across platforms.

**Produced:** macOS app bundle packaging (`iDroid.app`), `Info.plist` declaring
local network, Bonjour, and Bluetooth descriptions, `entitlements.plist`, and
ad-hoc codesigning in `build-mac.sh` (`A-007`); constant-time pairing token
comparison and 6-digit SAS derivation in `MacController.swift` (`A-013`);
Android `ContinuityService` supervisor with resilient accept loop, atomic
generation counter, `ConnectivityManager.NetworkCallback` network monitoring,
and persistent session identifier (`A-008`, `A-010`).

**Findings:** none new.

**Audit:** A-007, A-008, A-010, A-013 fixed. Both platform builds verified:
`build-mac.sh` produces signed `iDroid.app` bundle; `build-android.sh` produces
`app-debug.apk`.

**Left open:** A-004 / A-011 state machine refinement on Android, A-006 native
Keychain generation in Swift, A-009 NSD resolution serialization.

## S-010 — 2026-11-25 — Phase 3 discovery modernization and lifecycle unification

**Asked:** next step.

**Produced:** modernized Android `NsdDiscovery` with API 34+ `registerServiceInfoCallback`,
serialized resolve fallback queue, role filtering (`role=mac`), and 1-second
deduplication window (`A-009`); unified session lifecycle model by adding
`SessionConfig::for_control_plane()` aligned with `LinkMachine` timings (`A-003`);
documented `apps/mac-tauri` planning scaffold in root `README.md` (`A-020`).

**Findings:** none new.

**Audit:** A-003, A-009, A-020 fixed. Baseline now 81 tests passing. Verified
`build-android.sh` (:app:assembleDebug) and `make test`.

**Left open:** A-004/A-011 Android state machine transitions, A-006 native Keychain
RSA generation without openssl CLI, A-012 two-sided ConnectionArbiter.

## S-011 — 2026-11-25 — Phase 4 Android session continuity & dial arbitration

**Asked:** next continue.

**Produced:** mapped `has_session` through `idroid_core_jni.cpp` and `NativeCoreLink.kt`
to Rust FFI `idroid_link_handshake_ready` (`A-004`); reused `NativeCoreLink` instance
across reconnects and throughout the `ContinuityService` lifetime; updated `requireResult`
to treat `InvalidTransition` (2) as non-fatal idempotent observation (`A-011`);
added outbound dial gating on Android and duplicate discovery suppression (within 500ms)
on macOS to prevent dial thrashing and race conditions (`A-012`).

**Findings:** none new.

**Audit:** A-004, A-011, A-012 fixed. Verified: `make test` (81 tests pass),
`apps/mac-native/build-mac.sh` (signed bundle), and `build-android.sh` (:app:assembleDebug).

**Left open:** A-006 native Keychain RSA generation in Swift, A-019 abstraction pruning.

## S-012 — 2026-11-25 — Phase 5 Native Keychain identity & ASN.1 self-signed certificate

**Asked:** next continue.

**Produced:** eliminated `/usr/bin/openssl` and `Process` dependency in `apps/mac-native/Sources/IdroidMac/IdentityStore.swift` (`A-006`); implemented native RSA 2048 key generation via `SecKeyCreateRandomKey` directly in Keychain with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`; constructed self-signed X.509 v3 certificate via lightweight ASN.1 DER encoder and signed using `SecKeyCreateSignature(privateKey, .rsaSignatureMessagePKCS1v15SHA256, ...)`; imported certificate to Keychain and resolved `SecIdentity`; verified persistence, idempotency, and clean bundle codesigning.

**Findings:** none new.

**Audit:** A-006 fixed. Verified: `make test` (81 tests pass), standalone Swift persistence test, and `apps/mac-native/build-mac.sh` (signed bundle).

**Left open:** A-019 abstraction pruning in `core/src/discovery.rs`, `pairing.rs`, `connections.rs`.





