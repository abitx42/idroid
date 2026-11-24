# Session log

Append-only. One entry per working session, newest at the bottom. Keep it short:
what was asked, what was produced, which IDs were added.

## S-001 — 2026-11-12 — initial audit and call feasibility

**Asked:** analyse the whole repository and research how to build Mac↔phone
calling.

**Produced:** `docs/CALLS-CONTINUITY.md`. Verified baseline: 51 tests pass,
Clippy clean, `fmt` clean, no `unsafe` in `core/`.

**Findings:** F-001, F-002, F-006, F-037. **Audit:** A-001 – A-006, A-014 – A-020,
A-019.

**Left open:** whether `IOBluetoothHandsFreeDevice` can open SCO at all.

## S-002 — 2026-11-12 — seamless connectivity research

**Asked:** deeper research on how seamless cross-device connectivity is built.

**Produced:** `docs/SEAMLESS-CONNECTIVITY.md`.

**Findings:** F-017, F-022 – F-030. **Audit:** A-007 – A-012, A-014.

**Key conclusion:** Apple's seamlessness is account directory + push + P2P, not
better local networking. LAN-only is the honest ceiling without infrastructure.

## S-003 — 2026-11-12 — workarounds and bypasses

**Asked:** find loopholes, tricks, and ways around the blocking issues.

**Produced:** `docs/PLATFORM-WORKAROUNDS.md` §1–§8 (later extended); correction
added to `docs/SEAMLESS-CONNECTIVITY.md` §7 about free relay tiers.

**Findings:** F-003 – F-005, F-007 – F-016, F-018 – F-021, F-026. **Audit:** A-017,
A-018.

**Key conclusion:** HFP makes iPhone calls possible with zero iOS code; BLE HID
removes the Accessibility requirement for input.

## S-004 — 2026-11-12 — impersonation analysis

**Asked:** can iOS be made to look like Android, or Android like iOS?

**Produced:** `docs/PLATFORM-WORKAROUNDS.md` §9 (three gates, impersonation ledger,
Sleep Proxy, Beeper boundary, USB bridge).

**Findings:** F-031 – F-036, F-038 – F-042.

**Key conclusion:** cosmetic spoofing buys nothing; protocol interop is the real
door (AirDrop Everyone, Sleep Proxy); account identity is closed.

## S-005 — 2026-11-12 — knowledge base and agent instructions

**Asked:** make all research readable and continuously updated by any agent that
opens the folder, so nothing is researched twice.

**Produced:** `AGENTS.md`, `CLAUDE.md`, `research/` (this directory),
`scripts/research_check.py`, `make research-check`.

**Findings:** none new; entries F-001 – F-042 and A-001 – A-020 were consolidated
from S-001 – S-004.

**Decisions:** D-001 – D-008.

**Note for the next session:** start at `AGENTS.md` §1. If you add a finding,
update `research/index.json` and rerun the check.

## S-006 — 2026-11-24 — skill mirror and phase-0 hygiene

**Asked:** apply the staged plan, starting with repository hygiene, after making
the installed Antigravity skills available to OpenCode.

**Produced:** a global OpenCode skill mirror (`~/.config/opencode/skills`, 338
skills: 19 advertised, 319 hidden-but-loadable) plus a sync script and
documentation; removed the Instagram export experiment (`ebdcace`); pinned local
Makefile checks to the rustup toolchain and added `check-toolchain`; added
`SECURITY.md`; added `conventional-commits` and related engineering skills to
the advertised set.

**Findings:** none new. **Audit:** A-014 fixed, A-015 fixed, A-016 partially
addressed (`LICENSE` pending an owner decision).

**Left open:** license choice; Phase 1 (codec/identity unification) not started.

## S-007 — 2026-11-24 — Phase 1 protocol groundwork

**Asked:** continue phase by phase after Phase 0.

**Produced:** `FrameReader` in `core/src/protocol.rs` with chunk-boundary, partial
frame, invalid header, oversized length, and stalled-stream tests; installed nine
audited open-source skills globally (Rust best practices/review, cargo-fuzz,
Android Intent/JNI security, macOS SwiftPM packaging, threat modeling, TDD, and
a hidden Trail of Bits Rust security review).

**Findings:** none new. **Audit:** A-017 fixed; A-018 (real fuzzing) remains open.

**Left open:** wire-codec decision (D-008), identity unification (A-002), and
golden cross-language vectors (A-005).
