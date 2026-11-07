# Native macOS application scaffold

**Status:** thin experimental slice. The Swift Package builds a menu-bar
application with a Keychain-backed self-signed identity, QR pairing window,
Bonjour listener/browser, TLS 1.3 connection parameters, and the Rust
`idroid-core` link state machine through `core-ffi`.

**Owner:** macOS platform owner (TBD).

## Intended responsibility

This target will own the native host experience: device discovery, pairing
consent, display presentation, input delivery, lifecycle handling, and macOS
permission/TCC integration. It must consume the portable core through an
explicit adapter boundary rather than moving platform behavior into `core/`.

## Acceptance criteria before implementation is called complete

- A reproducible Xcode project builds on the supported Intel and Apple Silicon
  configurations.
- Screen Recording, Accessibility/Input Monitoring, and network permission
  behavior is user-visible, denied-by-default, and covered by macOS tests or
  documented manual evidence.
- The adapter handles transport loss and process shutdown without claiming
  session persistence that has not been implemented.
- A signed/notarized packaging path and versioned release artifact are tested
  in CI.
- Native unit/UI tests and a clean-core integration test identify exactly what
  behavior is covered.

This is not a signed release application. The first identity is generated with
the system OpenSSL executable and imported into Keychain; the pairing listener
accepts an unknown client certificate only for the one-time QR exchange, then
restarts with the stored Android certificate pin. The control-plane log is
written to `~/Library/Application Support/iDroid/control-plane.ndjson`.

Build on macOS:

```sh
bash apps/mac-native/build-mac.sh
```

The package has no committed build artifacts. A physical Android pairing and
reconnect run is still required before this is called a working product.
