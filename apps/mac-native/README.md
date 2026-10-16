# Native macOS application scaffold

**Status:** planning scaffold only. No Xcode project, Swift source, native
bundle, or macOS application tests exist in this directory.

**Owner:** macOS platform owner (TBD).

## Intended responsibility

This target will own the native host experience: device discovery, pairing
consent, display presentation, input delivery, lifecycle handling, and macOS
permission/TCC integration. It must consume the portable core through an
explicit adapter boundary rather than moving platform behavior into `core/`.

## Acceptance criteria before implementation is called complete

- A reproducible Xcode project builds on the supported Intel and Apple Silicon
  configurations.
- Screen Recording, Accessibility/Input Monitoring, and network permission
  behavior is user-visible, denied-by-default, and covered by macOS tests or
  documented manual evidence.
- The adapter handles transport loss and process shutdown without claiming
  session persistence that has not been implemented.
- A signed/notarized packaging path and versioned release artifact are tested
  in CI.
- Native unit/UI tests and a clean-core integration test identify exactly what
  behavior is covered.

Until those artifacts and evidence exist, this scaffold must not be described
as a working macOS application.
