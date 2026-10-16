# iDroid milestone decision gates

**Decision owner:** product and engineering together
**Decision point:** end of the three-week / 15-working-day feasibility sprint

This is a go/no-go rubric for turning the portable core foundation into a
validated Android-to-macOS product slice. A weighted score helps compare
trade-offs, but it cannot waive a hard gate. Evidence must be from the real
platform path when a gate names Android or macOS; Rust fixtures and unit tests
are useful for core correctness but cannot stand in for device evidence.

## 1. Decision vocabulary

- **Green:** acceptance evidence exists, is reproducible, and the owner has
  recorded the environment and result.
- **Amber:** partial evidence, a bounded defect, or an explicitly accepted
  limitation remains. Amber may reduce the score but cannot satisfy a hard
  gate that requires green evidence.
- **Red:** no evidence, a failed threshold, an unsafe behavior, or a defect
  without a credible fix and retest date.
- **Go:** every hard gate is green and the weighted score is at least 80/100.
- **Conditional:** every hard gate is green, but the score is 70–79/100 and
  the product owner explicitly limits the pilot scope. Conditional is not a
  general-support release.
- **No-go/stop:** any hard gate is red, or the score is below 70/100.

## 2. Non-negotiable hard gates

| ID | Hard gate | Pass evidence | Why it blocks |
| --- | --- | --- | --- |
| HG-1 | **Shared Linux buildability** | From a clean Linux checkout, the pinned Rust `1.85.0` toolchain and committed lockfile complete `cargo test --locked` without source edits or network-only assumptions. | Linux is shared repository health. Platform-specific work cannot hide a broken portable core. |
| HG-2 | **Real Android/macOS vertical slice** | One physical Android device and one supported macOS host complete user-approved pairing, identity verification, session start, a representative display/input action, and clean teardown. Emulator-only or fixture-only evidence does not pass. | The product value is cross-platform control; a library-only demo is not the product. |
| HG-3 | **Measured responsiveness** | The reference slice is measured with a 240-fps camera and reports startup, input response, glass-to-glass, frame freshness, p50/p95/p99, and dropped/misapplied actions. The pass thresholds are frozen at G0 after the baseline rehearsal; 80 ms median is a stretch goal, not a pre-declared result. | A timestamp in a log or a subjective “feels fast” report is not a latency result. |
| HG-4 | **Production security boundary** | The pilot uses cryptographic device identity, authenticated/encrypted transport, explicit authorization, OS permission consent, credential/key lifecycle, and negative tests for unknown/revoked peers. The fixture verifiers are absent from the release path. | The current core fixtures are intentionally non-cryptographic; shipping them as trust would be unsafe. |
| HG-5 | **Lifecycle and recovery correctness** | At least 100 scripted disconnect/reconnect/duplicate/stale cycles on the real slice show no incorrect device control, session collision, or unrecoverable state; failure and teardown are observable. | The core’s state machine is only a foundation until transport and platform failures are exercised. |

The thresholds in HG-3 are the milestone pass bar. If product leadership changes
the bar, the change must be recorded before the first measurement and must not
be retrofitted to make an existing result pass.

## 3. Weighted score

Score each category from 0 to 5 using the evidence rubric below, then calculate
`category score / 5 × weight`. The weights are fixed for this milestone and
sum to 100%:

| Category | Weight | A score of 5 requires |
| --- | ---: | --- |
| Reliability and reconnection | **30%** | HG-5 passes, including heartbeat timeout, reconnect grace, duplicate replacement, stale cleanup, sleep/wake recovery, and clean shutdown behavior. |
| Video/input latency and frame freshness | **25%** | HG-3 passes, interaction remains usable under the declared network/device conditions, and the evidence includes startup, input, glass-to-glass, p50/p95/p99, stale-frame and dropped-action results. |
| CPU, memory, and battery | **15%** | The 30-minute soak has recorded CPU, memory, thermal and battery evidence with no unexplained growth or unacceptable sustained resource use. |
| Native macOS integration | **20%** | The surviving candidate meets the declared menu-bar, window, focus, permission, reduced-motion and recovery checks with evidence on the supported macOS target. |
| Maintenance and build complexity | **10%** | A fresh engineer can build, test, diagnose and reproduce the slice using pinned, documented commands; platform-specific work remains isolated from the portable core. |

Use these anchors to avoid optimistic scoring:

| Score | Meaning |
| ---: | --- |
| 0 | No implementation or no credible evidence. |
| 1 | Concept or scaffold only; behavior is unverified. |
| 2 | Core or mocked path works, but the real acceptance path is missing or unreliable. |
| 3 | Real path works in a narrow, repeatable setup with known limitations. |
| 4 | Real path meets the intended bar with minor, bounded gaps and repeatable evidence. |
| 5 | Real path meets the bar across the declared matrix, with negative and recovery tests and no open release-blocking risks. |

The final record must show each raw score, the weighted calculation, links to
the experiment artifacts, and the owner/date for every amber item. A high
experience score never compensates for a red security or build gate.

## 4. Stage reviews

| Review | Timing | Required decision/evidence |
| --- | --- | --- |
| G0 — scope and health | Day 1 | Freeze the reference Android/macOS pair, measurement setup, support assumptions, and Linux command. Baseline `cargo test --locked`. |
| G1 — core contract | Day 5 | Protocol/state/security boundary reviewed. Malformed frames, duplicate sessions, timeout/reconnect, and fixture disclaimers are reproducible. No platform adapter may silently redefine the contract. |
| G2 — platform feasibility | Day 10 | At least one physical Android and one macOS host can reach the adapter boundary with user-visible permission state and diagnostic logs. Any OS blocker has an owner and a dated experiment. |
| G3 — day-15 decision | Day 15 | Complete 240-fps latency run, security negative tests, lifecycle stress run, weighted score, and all hard-gate decisions. Apply the stop rule below. |

## 5. Day-15 stop rule

At the Day-15 review, **stop the milestone and do not spend additional scope on
polish** if any of the following is true:

1. any hard gate is red;
2. the weighted score is below 70/100;
3. the score is 70–79/100 but the product owner will not accept a narrowly
   scoped conditional pilot; or
4. a failed gate has no technically credible owner, fix, and retest date that
   fits the next decision window.

“Stop” means freeze new feature work, preserve the core and evidence, write the
failure and pivot recommendation, and decide whether to change the product
scope, platform pair, or measurement bar through an explicit decision. It does
not mean hiding a failing experiment, replacing real-device evidence with
fixtures, or silently extending the sprint.

If all hard gates are green and the score is at least 80, record **Go** and
define the next hardening milestone. If the result is conditional, record the
exact pilot limits, supported device pair, expiry/review date, and the missing
evidence; conditional status must not be communicated as broad platform
support.

## 6. Evidence rules

Every result must record:

- commit or source revision, toolchain, build command, and whether the run was
  from a clean checkout;
- Android model, OS/API level, capture/input path, macOS model/OS, permissions,
  transport, display refresh rate, power/thermal state, and network conditions;
- test count, warm-up count, failures/drops, p50/p95/p99/max where applicable,
  and raw artifact location;
- whether the path used production security or a fixture (fixtures can only
  support core tests); and
- the owner, date, and next action for any amber or red result.

See [`EXPERIMENT-PLAN.md`](EXPERIMENT-PLAN.md) for the execution sequence and
the 240-fps measurement method. See [`THREAT-MODEL.md`](THREAT-MODEL.md) for
what a security gate must prove.
