# Protocol contract scaffold

**Status:** contract scaffold only. The current portable codec is in
`core/src/protocol.rs`; this directory contains no additional implementation.

**Owner:** core protocol maintainer (TBD).

## Intended responsibility

This area will hold the reviewed wire-contract material around framing,
version negotiation, message semantics, limits, compatibility, and migration.
The transport remains a separate concern and must not be implied by a codec
specification.

## Acceptance criteria

- A canonical versioned frame specification states byte layout, size limits,
  endianness, invalid inputs, and forward/backward compatibility rules.
- Golden vectors and malformed-frame cases are stable and checked in tests.
- Every message has an ownership, authorization, ordering, and replay policy;
  parsing alone is not treated as authorization.
- A compatibility decision records how a future version is negotiated and
  rejected safely.
- Interoperability evidence identifies the exact producer, consumer, and
  protocol version.

Until those artifacts are added, this README is not a protocol release or an
interoperability claim.
