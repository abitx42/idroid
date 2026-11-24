# Security policy

## Supported versions

iDroid has no shipped release yet. The `main` branch is the only supported
line; security fixes are not backported to older commits.

## Reporting a vulnerability

Please **do not open a public issue** for a suspected vulnerability.

Use GitHub's private vulnerability reporting for this repository
(**Security → Advisories → Report a vulnerability**). If that channel is not
available, contact the repository owner directly through GitHub.

Include, where possible:

- the affected component (`core/`, `core-ffi/`, the Android slice, or the macOS
  adapter);
- a minimal reproduction or the exact commit;
- the impact you believe is possible;
- any suggested fix or mitigation.

## Scope

In scope:

- the Rust protocol codec, framing, identity, pairing, discovery, and control
  plane in `core/` and `core-ffi/`;
- the Android vertical slice under `experiments/android-macos-vertical-slice/`;
- the Swift macOS adapter under `apps/mac-native/`.

Out of scope for this repository: third-party services, carrier networks, and
Apple platform vulnerabilities that are not caused by this code.

## Disclosure

We aim to acknowledge a report within three business days and to agree on a
remediation and disclosure timeline with the reporter. There is no bug bounty.
