# iDroid

## Status

This repository is at the **portable Rust core foundation** milestone. The
workspace contains a dependency-free `idroid-core` library and deterministic
tests for its current in-memory behavior. A bounded, non-product Android-to-
macOS feasibility experiment is also present under
`experiments/android-macos-vertical-slice/`; transport interoperability,
production security, and release packaging remain unimplemented.

The repository's acceptance baseline is a clean checkout that can run the
pinned Rust core tests on Linux:

```text
cargo test --locked
```

`Cargo.lock` and `rust-toolchain.toml` are committed so that this check uses a
known Rust channel and a locked dependency graph.

## Setup

Requirements:

- Rustup and Cargo;
- GNU Make for the convenience targets; and
- Docker only for container validation.

From a clean checkout, install the repository toolchain and lint components:

```sh
rustup toolchain install 1.85.0 --profile minimal
rustup component add rustfmt clippy --toolchain 1.85.0
cp .env.example .env
```

The `.env` file contains only local, non-secret placeholders. The current core
does not consume these values; application configuration will be connected when
an application exists.

## Run and test

There is no product Android, macOS, or server executable yet. The commands
below exercise the repository's current deliverable: the portable Rust core.
The bounded macOS-only and Android build experiments have their own exact
commands in their local READMEs and are not covered by `make test`.

| Command | Status and exact scope |
| --- | --- |
| `make test` | Implemented and cache-only: runs `cargo test --locked --offline --workspace --all-targets`. The current workspace has no external crates. |
| `make lint` | Configured: checks `rustfmt` and runs Clippy with warnings denied. Requires the two components installed above. |
| `make test-mac` | Implemented for portable-core tests on macOS. It explicitly does **not** test a native macOS app; that app does not exist yet. |
| `make test-offline` | A clearly non-claiming cache-only check using Cargo `--offline`; failure can mean the local cache is incomplete. It is not evidence of network independence or release readiness. |
| `make clean` | Removes Cargo build output. |
| `cargo test --locked` | The minimal cross-platform health check documented for this milestone. |

For Linux validation in the pinned container:

```sh
docker build --tag idroid-ci:local .
docker run --rm idroid-ci:local
```

The image build runs the tests, and the default container command runs them
again. The image installs only `make` and CA certificates in addition to the
pinned Rust toolchain; the workspace has no external Rust crates at this
milestone.

## CI

`.github/workflows/ci.yml` has two intentionally separate jobs:

- **Linux portable-core:** installs Rust 1.85.0, runs the core tests, and runs
  the configured lint checks.
- **macOS core-only:** runs `make test-mac`. A green job proves only that the
  portable Rust core tests pass on macOS; native macOS application tests are
  not present yet.

## Implemented scope

The current `core/` crate provides:

- non-zero `DeviceId` and `SessionId` value types;
- an in-memory device registry and capability model;
- orthogonal connection, capture, input, and permission state dimensions with
  derived user-facing headlines;
- an in-memory session lifecycle with heartbeat timeout, reconnect grace,
  duplicate replacement, termination, and stale cleanup;
- strict version-one binary protocol framing and malformed-frame errors; and
- dependency-injected identity, pairing, session-authentication, replay, and
  message-verification interfaces with deterministic fixtures for tests.

The fixtures are test doubles, not cryptography. The protocol codec does not
provide authentication, authorization, encryption, transport retries, or
stream handling.

## Unverified and intentionally absent scope

The following are **not** implemented or verified by this repository:

- Product Android agent behavior, capture, input delivery, permissions, or OEM
  compatibility. The experimental debug APK scaffold is buildable but has no
  physical-device evidence;
- native macOS or Tauri application behavior, permissions, signing, or
  notarization;
- sockets, discovery, pairing UX, WebRTC, USB/ADB, or network interoperability;
- production cryptography, key storage, authorization, revocation, replay
  protection, or audit logging;
- durable persistence, crash recovery, or cross-process coordination;
- latency, throughput, battery, thermal, memory, or soak claims; and
- deployable artifacts or a deployment target.

The scaffold READMEs under `apps/`, `core/`, `tests/`, and `benchmarks/` state
the owner and acceptance criteria for these future boundaries without claiming
that implementation exists.

The first WKWebView kill-switch harness is also present under
`experiments/wkwebview-webrtc/`. It is a macOS-only, local-loopback feasibility
probe. Its report is **NOT TESTED** until it is actually built and run on a
logged-in macOS host; it is not a product application or a cross-device result.

## Deploy status

**Not deployable yet.** There is no product service, signed macOS bundle,
release APK, release artifact, or deployment configuration. The experimental
debug APK is not a deployable product artifact. The Docker image is a Linux
validation container only; it is not a product runtime or release image.
Deployment becomes meaningful after a platform application, transport,
production security boundary, packaging/signing process, and release checks are
implemented and verified.
