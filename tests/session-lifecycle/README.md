# Session-lifecycle test scaffold

**Status:** test-plan scaffold only. No dedicated lifecycle suite exists in
this directory.

**Owner:** session/core test owner (TBD).

## Acceptance criteria

- Tests cover connected, heartbeat-timeout, requested disconnect, reconnect,
  grace expiry, offline, termination, duplicate replacement, and stale cleanup.
- Boundary timestamps and backwards-clock inputs are exercised explicitly.
- Device-to-session ownership remains consistent across replacement and
  termination, including failure paths.
- Tests state which behavior is process-memory only and do not imply durable
  resume after restart.
- CI output identifies the exact configuration and test set used.

Until dedicated tests and evidence are present, this scaffold is not a claim
of complete lifecycle coverage.
