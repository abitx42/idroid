# Security test scaffold

**Status:** security test-plan scaffold only. No production security provider
or dedicated security test suite exists in this directory.

**Owner:** security test owner (TBD), with review from the security maintainer.

## Acceptance criteria

- Tests distinguish identity verification, message authentication, replay
  protection, authorization, revocation, expiry, and audit behavior.
- Negative tests cover missing, malformed, expired, revoked, mismatched, and
  replayed credentials or tags, with fail-closed outcomes.
- Secret material is generated for tests, redacted from output, and never
  committed as a fixture or example value.
- Approved platform primitives and threat-model assumptions are recorded for
  every production provider.
- Deterministic fixture tests are labeled as fixtures and cannot establish a
  cryptographic security claim.

No security assurance follows from this README or from the current fixture
implementations.
