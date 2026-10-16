# Reconnection test scaffold

**Status:** test-plan scaffold only. No transport or reconnection harness
exists in this directory.

**Owner:** session/transport integration test owner (TBD).

## Acceptance criteria

- A temporary disconnect can reconnect within the configured grace window and
  preserves the intended session identity.
- Reconnect after grace expiry, duplicate sessions, repeated disconnects,
  reordered events, and transport loss have explicit expected outcomes.
- Heartbeat timeout and manual disconnect remain distinguishable in observable
  state and diagnostics.
- Reconnect tests use deterministic clocks and fault injection rather than
  timing luck.
- Any real-network result records topology, transport, OS, and build version;
  unit results are not substituted for interoperability evidence.

This document defines the future evidence bar and does not claim that a
reconnection implementation exists.
