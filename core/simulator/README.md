# Core simulator scaffold

**Status:** test-infrastructure scaffold only. No simulator, fake transport,
fault model, or deterministic scenario runner exists in this directory.

**Owner:** core test-infrastructure maintainer (TBD).

## Intended responsibility

The simulator will provide reproducible peers and controlled clock, transport,
message-ordering, disconnect, and malformed-input scenarios for the portable
core. It must remain visibly separate from production adapters and security
providers.

## Acceptance criteria

- Every scenario has a recorded seed, input sequence, expected state, and
  failure output that can be replayed offline.
- Clock jumps, duplicate messages, truncation, reordering, loss, and reconnect
  boundaries are modeled without introducing nondeterminism.
- Simulated credentials are clearly labeled fixtures and cannot be selected as
  production security providers.
- Scenarios are small enough for CI and a longer suite can be selected
  separately without changing semantics.
- Results identify simulator limitations and are not presented as real-device
  or network evidence.

No simulator behavior should be inferred from this planning scaffold.
