# Decision log

Append-only. A decision is either `accepted` (settled for now) or `proposed`
(waiting on a spike or owner). Supersede with a new `D-###`.

| ID | Status | Decision | Why | Consequences |
| --- | --- | --- | --- | --- |
| D-001 | accepted | Keep the accumulated research in-repo as an agent-readable knowledge base (`research/`), mandated by `AGENTS.md`. | Research was repeated across sessions and the conclusions were buried in prose docs. | Every session appends findings; `make research-check` enforces structure. |
| D-002 | accepted | Do not pursue private Apple APIs or borrowed Apple identity. | `F-033`: Apple blocks it server-side; forging attestation is circumvention, not interoperability. | iPhone support is limited to HFP calls and ReplayKit/SCK viewing. |
| D-003 | accepted | Treat LAN-first as the shipping scope; treat cross-network as a later, separately funded step. | `F-025`, `F-026`: the tail needs a relay, and a relay changes the company's shape. | No relay infrastructure in v1; keep the transport abstract enough to add one. |
| D-004 | proposed | Input path preference: Bluetooth HID first, Accessibility as fallback. | `F-012`, `F-013`: HID avoids TCC and the Play declaration; `F-016` limits gestures. | Pending `F-015`; if the digitizer spike fails, keep Accessibility for gesture-heavy use. |
| D-005 | proposed | iPhone support = HFP calls + screen viewing, never control. | `F-003`–`F-005`, `F-004` is absolute. | Marketing must not promise iPhone remote control. |
| D-006 | proposed | macOS ships as a Developer ID, notarized, direct-distribution app; a reduced App Store edition may cover phone-controlled direction only. | `F-034`, `F-035`. | Two builds to maintain, one core; the MAS edition cannot inject input into the Mac. |
| D-007 | proposed | Replace the ad-hoc discovery/transport ladder with: mDNS on LAN, BLE presence, `iroh` for cross-network. | `F-026`, `F-022`: AWDL is closed, `iroh` is Rust-native and key-addressed. | New dependency decision required; must not contaminate `core/`'s dependency-free rule. |
| D-008 | accepted | Wire the core codec onto the real connection before adding any protocol feature. | `A-001`, `A-002`, `A-005`. | Blocks new features until the identity model is unified. |

## How to use this file

If you are about to do something that contradicts an `accepted` decision, stop
and either add a `supersedes` decision with the reason, or ask the owner. A
`proposed` decision is context, not a constraint — but do not silently ignore it.
