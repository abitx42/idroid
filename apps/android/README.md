# Android application scaffold

**Status:** the platform-owned thin-slice code currently lives in
`experiments/android-macos-vertical-slice`. It includes a Keystore identity,
CameraX/ML Kit QR scanning, pinned TLS 1.3 pairing, NSD
advertising/browsing, and a connected-device foreground service. This directory
remains the product ownership scaffold.

**Owner:** Android platform owner (TBD).

## Intended responsibility

The Android target will own device discovery, user-approved pairing, capture,
input delivery, foreground/background lifecycle, and device resource limits.
It must adapt Android callbacks and permissions into normalized operations at
the portable-core boundary.

## Acceptance criteria before implementation is called complete

- A pinned Android/Gradle toolchain produces a reproducible debug artifact and
  a documented release artifact from a clean checkout.
- Screen capture consent, input/accessibility authority, notifications, and
  background/foreground behavior are explicit and tested on supported API
  levels.
- Transport loss, process death, battery/thermal limits, and OEM variation
  have bounded behavior and evidence.
- Pairing and capability decisions are user-approved and fail closed when
  permissions or verification are unavailable.
- Unit, instrumented, and device-matrix tests cover the adapter without
  treating an emulator-only result as device support.

No Android support claim should be made until the project, artifact, and
physical-device test evidence exist.
