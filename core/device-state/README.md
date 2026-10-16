# Device-state scaffold

**Status:** design scaffold only. The current in-memory device model is in
`core/src/device.rs`; no persistent or platform-backed state implementation
exists in this directory.

**Owner:** core state maintainer (TBD), with platform owners responsible for
adapter inputs.

## Intended responsibility

This area will define device identity, capabilities, discovery observations,
state transitions, and the boundary between process memory and durable state.
It must distinguish an observed capability from a user-authorized capability.

## Acceptance criteria

- State ownership and invariants are documented for discovery, registration,
  removal, capability changes, and duplicate identities.
- Persistence, crash recovery, deletion, and secret-handling behavior are
  specified before any durable storage is treated as supported.
- Stale or contradictory platform observations are rejected or made visible;
  they do not silently grant control authority.
- Deterministic tests cover updates, conflicts, restart behavior, and bounded
  resource use.
- Evidence names the supported platform adapters and does not generalize from
  a fixture to real devices.

This scaffold must not be read as evidence of persistence or device discovery.
