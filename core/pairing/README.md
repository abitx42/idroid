# Pairing and security scaffold

**Status:** security-boundary scaffold only. The current traits and
deterministic fixtures are in `core/src/security.rs`; no production pairing or
cryptography exists in this directory.

**Owner:** security/pairing maintainer (TBD), with platform owners for consent
and secure storage.

## Intended responsibility

This area will define user-approved pairing, identity verification, credential
lifetime, revocation, channel authentication, and authorization hand-off. A
valid frame or registered device must never be treated as permission to act.

## Acceptance criteria

- A threat-model-backed pairing flow covers first use, consent, rejection,
  revocation, expiry, recovery, and device replacement.
- Production cryptography and key storage use reviewed platform primitives;
  fixture equality checks remain test-only.
- Authentication, authorization, replay protection, and audit events have
  separate policies and negative tests.
- Secrets are excluded from logs, fixtures, container layers, and repository
  configuration.
- Security review evidence names the exact implementation and supported
  platforms before any security claim is published.

This README marks a future boundary and is not a claim that pairing is secure
or implemented.
