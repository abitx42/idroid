# iDroid platform support

**Milestone scope:** a portable Rust core foundation plus an unvalidated thin
Android/macOS pairing and reconnect slice. This matrix is intentionally
conservative: buildability is not the same as a validated product platform.

## 1. Support levels

| Level | Meaning |
| --- | --- |
| **Implemented** | Code exists in this repository and the listed behavior has a reproducible test. |
| **Build target** | The architecture is intended to compile there, but a platform adapter or clean-build evidence is still missing. |
| **Pilot candidate** | One declared device/OS pair has passed real-device acceptance and the decision gates. |
| **Not supported** | Out of scope for this milestone; no compatibility promise. |

## 2. Current matrix

| Platform/role | Portable core | Platform adapter/app | Permissions and system constraints | Evidence/status |
| --- | --- | --- | --- | --- |
| Linux CI/reference host | **Implemented target**: library is standard Rust with no OS-specific source. | No desktop UI, capture, or input adapter. | Requires pinned Rust `1.85.0` and the committed lockfile. | `cargo test --locked` is the shared health check; clean Linux validation is required at the decision gate. |
| macOS host | **Build target**; no macOS-specific behavior in core. | **Thin slice built/unverified**: Swift menu-bar app, Keychain identity, QR window, Bonjour/TLS listener/browser, and Rust FFI link state; no capture/input bundle or signing. | Screen Recording is required for capture; Accessibility/Input Monitoring is required for synthetic input. TCC state, code signing, hardened runtime, notarization, Apple Silicon/Intel, and firewall behavior must be tested. | Physical pairing/reconnect not run; not a product-support claim. |
| Android physical device | **Build target**; Rust JNI bridge builds for arm64. | **Thin slice built/unverified**: debug APK, Keystore identity, QR scanner, pinned TLS pairing, NSD, and connected-device foreground service; capture path remains separate. | MediaProjection consent, foreground/background limits, battery/thermal behavior, OEM/API variation, and Accessibility or another input path must be handled. ADB, if selected, requires developer options, USB debugging, and user-approved host keys. | Xiaomi `22101320I` is the declared trial device; no physical pass yet. |
| Android emulator | Core can be exercised by a host test, but device behavior is not validated. | Not an acceptance target. | Emulator timing/capture/input does not represent physical-device power, thermal, OEM, or permission behavior. | Useful for development only; not a pilot candidate. |
| Windows/iOS/other | No platform code or compatibility promise. | **Not supported** for this milestone. | No entitlement, input, capture, or release plan. | Defer until the Android/macOS contract is validated. |

## 3. Android constraints and integration contract

The Android side must be treated as an untrusted, user-consented adapter. It is
responsible for:

- pairing and device identity without assuming a stable serial number is an
  authentication credential;
- obtaining and continuously reflecting user consent for screen capture
  (`MediaProjection` or an explicitly selected alternative);
- using an allowed input path (for example, an approved accessibility-based
  path or an explicitly authorized ADB path) without assuming root or silent
  injection;
- handling Android lifecycle/background restrictions, foreground-service
  requirements, battery optimization, process death, rotation, lock state,
  network changes, and OEM-specific behavior;
- bounding capture resolution/frame rate and data queues so thermal or memory
  pressure cannot turn into unbounded core work; and
- converting permission loss, app pause, device lock, transport loss, and
  capture/input failure into explicit session events.

The portable core does not currently know an Android API level, device model,
permission state, or capture format. A supported API-level floor must be
declared only after the chosen capture/input path is tested; until then,
“Android support” means architecture intent, not compatibility.

## 4. macOS constraints and integration contract

The macOS side must own:

- Screen Recording consent and the actual capture API;
- Accessibility/Input Monitoring consent and an input path that respects TCC;
- code signing, hardened runtime, entitlements, notarization, update
  provenance, and the Intel/Apple-Silicon decision;
- app foreground/background and sleep/wake behavior, display changes, lock
  state, multiple monitors, refresh-rate changes, and network firewall prompts;
- explicit user-visible pairing, revoke, and stop controls; and
- translation of permission revocation, app termination, display loss, and input
  refusal into fail-closed session transitions.

The thin macOS package links Apple frameworks for its menu-bar, Bonjour, and TLS
adapter, but declares no product entitlements. A successful package build does
not prove that screen capture, input, pairing, or sleep/wake recovery works.

## 5. Linux as shared repository health

Linux is a cross-cutting requirement rather than a score category that can be
traded away. Every platform change must preserve:

```text
cargo test --locked
```

from a clean Linux checkout with Rust `1.85.0` and the committed `Cargo.lock`.
The check must not require macOS frameworks, Android SDK/NDK, an IDE, or a
network-only dependency. A failure blocks the decision even if the Android and
macOS demo appears successful, because the shared core is no longer a
reproducible foundation.

The repository now includes a Makefile, Dockerfile, and CI configuration for
the portable core. These provide reproducible commands and a separate macOS
core-only job; they do not constitute a native macOS application build or
real-device evidence. The first experiment must still capture the clean-machine
output and preserve it as evidence.

## 6. Pilot support criteria

An Android/macOS pair can be called a **pilot candidate** only when all of the
following are recorded:

1. exact Android model/API level and macOS model/OS/build;
2. capture, input, transport, display refresh, and power/thermal settings;
3. permissions/entitlements before, during, and after a revoke/restart test;
4. authenticated pairing and explicit capability authorization;
5. the 240-fps latency artifact and lifecycle/security negative-test results;
6. clean teardown, reconnect, and process-restart behavior; and
7. a green hard-gate result and support scope in
   [`DECISION-GATES.md`](DECISION-GATES.md).

Until then, the correct label is **scaffolded/unverified**, even if a local
developer can manually move bytes between a phone and a Mac.
