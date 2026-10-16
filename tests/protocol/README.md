# Protocol test scaffold

**Status:** test-plan scaffold only. No protocol test suite exists in this
directory; the current baseline tests live with the Rust core.

**Owner:** protocol test owner (TBD), coordinated with the core protocol
maintainer.

## Acceptance criteria

- Round-trip tests cover every supported message and boundary-sized payload.
- Golden vectors verify byte layout independently of the encoder.
- Truncated, oversized, unknown-version, unknown-kind, invalid-ID, invalid
  reason, and trailing-byte inputs are rejected with stable diagnostics.
- Tests define compatibility behavior for future protocol versions and do not
  treat successful parsing as authentication or authorization.
- Any interoperability test records the exact producer, consumer, and wire
  version.

The directory becomes an evidence location only when these tests and their
fixtures are added; this README does not claim they have run.
