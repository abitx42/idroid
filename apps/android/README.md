# Android application scaffold

**Status:** planning scaffold only. No Gradle project, APK, Android source,
device agent, or instrumented tests exist in this directory.

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

No Android support claim should be made until the project, artifact, and test
evidence exist.
