# AGENTS.md — read this first

**Last updated:** 2026-11-12 · **Knowledge base:** [`research/`](research/README.md)

This repository is **iDroid**: a cross-platform continuity bridge between Android
and macOS with a portable Rust core. Most of the repository is research and
scaffolding, not shipped software.

## 1. Before you research anything

**The knowledge base exists so you do not repeat work.** In this order:

1. `research/findings.md` — every concluded question, with a stable ID (`F-###`),
   an evidence grade, and a source. Search it before opening a browser.
2. `research/audit.md` — known defects (`A-###`), their status, and where they live.
3. `research/decisions.md` — decisions already taken (`D-###`), so you do not
   relitigate them.
4. `research/sessions.md` — what previous sessions did, in order.
5. `research/README.md` — the protocol for adding to the KB.

If a question is already answered, **cite the ID and stop**. If it is marked
`settled` and you believe it is wrong, add a new finding that `supersedes` it —
do not silently edit the old one.

## 2. Hard rules

1. **Append, never rewrite.** Findings and audit entries are append-only. To
   correct one, add a new entry with `supersedes: F-0xx` and set the old entry's
   status to `superseded`.
2. **Never claim verification you did not perform.** Use the evidence grades in
   `research/README.md`: `[V]` documented, `[P]` working project or first-hand
   report, `[A]` architectural inference, `[?]` untested. A doc you read is not a
   test you ran.
3. **Record what you changed.** Every session appends a `S-###` entry to
   `research/sessions.md` listing new findings, new decisions, and files touched.
4. **Update the index.** New findings must be added to `research/index.json`
   under the right topic, and `make research-check` must pass.
5. **Do not build on private Apple APIs or borrowed Apple identities.** This is a
   settled decision (`D-003`); see `F-029` and `docs/PLATFORM-WORKAROUNDS.md` §9.4.
6. **Do not add dependencies to `core/`** unless a finding or decision says so —
   the core is deliberately dependency-free and must keep building on Linux.
7. **Keep the pinned toolchain.** `rust-toolchain.toml` pins 1.85.0; CI uses it.
   A local `rustc` from Homebrew is not evidence (`A-014`).

## 3. Project facts

| Fact | Value |
| --- | --- |
| Workspace crates | `core` (portable, `#![forbid(unsafe_code)]`), `core-ffi` (C ABI + JNI shim) |
| Baseline check | `cargo test --locked --workspace --all-targets` → 51 tests |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check` |
| Platform slices | `apps/mac-native` (Swift menu-bar), `experiments/android-macos-vertical-slice` (Kotlin) |
| Docs | `docs/ARCHITECTURE.md`, `docs/CONTROL-PLANE-V1.md`, `docs/CALLS-CONTINUITY.md`, `docs/SEAMLESS-CONNECTIVITY.md`, `docs/PLATFORM-WORKAROUNDS.md` |
| No product executable yet | The Rust core is the only verified deliverable |

## 4. Current state in one paragraph

The Rust core is solid and tested; the platform slices are unverified. The core
protocol (`core/src/protocol.rs`) is **not used on the wire** — the Android and
macOS slices speak JSON over TLS, so the codec is a parallel universe (`A-001`).
Two identity models and two lifecycle timer sets coexist (`A-002`, `A-003`). The
macOS app has no bundle, Info.plist, or entitlements (`A-007`), and the Android
listener can wedge silently with no network-change handling (`A-008`, `A-010`).
The highest-value open technical questions are the two spikes in
`docs/PLATFORM-WORKAROUNDS.md` §2.1 and §3.1.

## 5. Where the research lives

| Question | Read |
| --- | --- |
| Can a Mac place/receive phone calls? | `docs/CALLS-CONTINUITY.md`, `F-001`–`F-011` |
| How do real seamless systems work? | `docs/SEAMLESS-CONNECTIVITY.md`, `F-022`–`F-026` |
| What can be bypassed, and at what cost? | `docs/PLATFORM-WORKAROUNDS.md`, `F-012`–`F-021` |
| Can Android impersonate an iPhone? | `docs/PLATFORM-WORKAROUNDS.md` §9, `F-027`–`F-030` |
| What is broken in the code? | `research/audit.md` |

## 6. Definition of done

A change is done when: the relevant check passes, the finding or audit entry is
recorded, `make research-check` is green, and the final message states what was
verified and what was not.
