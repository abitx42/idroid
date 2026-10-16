# Tauri macOS application scaffold

**Status:** planning scaffold only. No Tauri configuration, frontend, Rust
commands, bundle, or application tests exist in this directory.

**Owner:** desktop shell/platform owner (TBD).

## Intended responsibility

This target is an alternative desktop shell for a macOS host UI. It will own
windowing, user-facing state, command/event wiring, and packaging while
delegating portable protocol and lifecycle rules to the core. It must document
which operations are privileged native commands and which are ordinary UI
state updates.

## Acceptance criteria before implementation is called complete

- A pinned Tauri toolchain and lockfile build a signed macOS bundle from a
  clean checkout.
- Native commands have explicit input validation, authorization boundaries,
  and tests for failures as well as successful calls.
- Permission prompts and denied-permission states are observable in the UI;
  no silent screen capture or input authority is assumed.
- Frontend/native integration tests cover reconnect, shutdown, and transport
  error states without bypassing the core contract.
- Release packaging and notarization checks are reproducible in CI.

This directory remains a scope and ownership marker until those artifacts are
added; it is not evidence that a Tauri app exists.
