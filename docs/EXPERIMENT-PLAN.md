# iDroid three-week experiment plan

**Objective:** determine whether the portable Rust core can support a secure,
responsive, repeatable Android-to-macOS vertical slice without sacrificing
Linux repository health.

This is a 15-working-day, three-week feasibility plan. The current repository
already supplies a testable core contract; the experiments below deliberately
separate that evidence from the unbuilt platform and production-security work.
The decision rubric is in [`DECISION-GATES.md`](DECISION-GATES.md).

## 1. Questions and hypotheses

| Question | Hypothesis | Falsifier |
| --- | --- | --- |
| Does the core survive real transport and lifecycle failures? | The explicit session states and bounded protocol provide a stable adapter contract for timeout, reconnect, duplicate, and stale cleanup. | Repeated real disconnects create incorrect active sessions, unbounded state, or ambiguous recovery. |
| Can the reference Android/macOS pair meet the interaction bar? | A narrow capture/input path can meet a declared, baseline-informed 240-fps target on one pair before broad device support. | The reference pair cannot meet the threshold frozen at G0, or loses/misapplies more than the declared action-error budget. |
| Can identity and permissions be made trustworthy? | Injected security ports can be backed by cryptographic identity and OS consent without exposing credentials to the core. | The only workable path relies on fixture tags, root/silent control, or unverifiable permission state. |
| Can the project remain portable? | Platform adapters can stay outside `idroid-core`, leaving a clean Linux build and test. | Android/macOS dependencies or conditional code make `cargo test --locked` fail on clean Linux. |

## 2. Three-week schedule

### Week 1 — contract, instrumentation, and health (Days 1–5)

| Day | Work | Exit evidence |
| ---: | --- | --- |
| 1 | Freeze the reference device pair, supported OS assumptions, user journey, latency threshold, and owners. Run the baseline Linux check. | G0 record; clean-checkout `cargo test --locked` result; no unowned assumptions. |
| 2 | Review the protocol and state machine. Enumerate every adapter event, error, cancellation, and permission state that must cross the boundary. | Contract table and state/event mapping; unresolved questions assigned. |
| 3 | Build the measurement harness design and camera rig. Define input marker, visible response marker, warm-up, sample count, and raw-data format. | Rehearsal at 240 fps with frame-numbered artifacts; timing calculation reviewed. |
| 4 | Exercise malformed frames, size limits, duplicate sessions, heartbeat timeout, reconnect grace, stale cleanup, and clock-rewind handling. Add no production dependency solely for the experiment. | Core regression output and list of missing real-transport tests. |
| 5 | Review Android permission/capture/input options and macOS TCC, code-signing, and input/capture constraints. Select one feasible path per platform. | G1 review; platform decision log; explicit unsupported paths. |

### Week 2 — narrow vertical slice (Days 6–10)

| Day | Work | Exit evidence |
| ---: | --- | --- |
| 6 | Implement the thinnest Android adapter/agent boundary for the chosen device and a macOS host boundary. Keep platform code out of the portable core. | Device discovery/pairing smoke path; permission states visible. |
| 7 | Connect the real transport to V1 frame encode/decode and session lifecycle. Add fail-closed handling for malformed, unknown, unauthenticated, and revoked input. | One authenticated session can start and stop; negative cases are observable. |
| 8 | Exercise display/input action with a single representative payload. Validate back-pressure, cancellation, foreground/background behavior, and teardown. | End-to-end action trace on the reference pair; no fixture security in the path. |
| 9 | Run a pilot latency and reliability set. Tune only documented, bounded parameters; do not hide dropped frames or actions. | Preliminary p50/p95/p99 and failure log; list of blockers. |
| 10 | Run the platform and security review. Decide whether the pair is sufficiently real to enter final measurement. | G2 decision; red/amber/green status for HG-2 and HG-4. |

### Week 3 — evidence and decision (Days 11–15)

| Day | Work | Exit evidence |
| ---: | --- | --- |
| 11 | Freeze code/config for measurement. Verify clean Linux buildability again and capture the exact environment. | Reproducible build record; no unreviewed measurement changes. |
| 12 | Run the full 240-fps latency experiment under the declared reference conditions and one declared stress condition. | Raw camera clips/frame index, computed distribution, drops, and setup sheet. |
| 13 | Run at least 100 disconnect/reconnect/duplicate/stale cycles and security negative tests (unknown/revoked/expired/replayed/permission denied). | Lifecycle and security reports with failed cases retained. |
| 14 | Re-run failures, review threat-model mitigations, score all five weighted categories, and prepare the decision packet. | Draft scorecard, open-risk register, and owner/date for each gap. |
| 15 | Hold the final gate review. Apply the stop rule exactly; record Go, Conditional, or No-go and preserve artifacts. | Signed decision, support matrix, next milestone or pivot recommendation. |

The Day-15 rule is not a request for more polish: if a hard gate is red, the
score is below 70, a conditional pilot is not accepted, or no credible fix fits
the decision window, stop under [`DECISION-GATES.md`](DECISION-GATES.md#5-day-15-stop-rule).

## 3. 240-fps latency experiment

### What is measured

Measure **end-to-end motion-to-photon latency**: the interval from a known
input event becoming visible at the Android input boundary to the first stable
corresponding visual change at the macOS display boundary. If the product slice
uses the opposite direction, define the two endpoints explicitly and do not
combine directions into one number.

At 240 frames per second, one camera frame is approximately **4.167 ms**. The
camera provides a physical observation of both endpoints and therefore catches
queueing, transport, capture, decode, render, and permission-path delays that
software timestamps alone can miss.

### Apparatus and procedure

1. Use a calibrated 240-fps camera with a fixed exposure/shutter configuration,
   stable mounting, and a visible field containing both the input marker and
   the destination display marker. Record the camera model and actual frame
   rate; do not assume the nominal setting.
2. Drive a deterministic input marker (for example, a controlled LED or
   physical button) at the Android boundary and render a high-contrast marker
   on the macOS display when the event is applied. The marker must be
   generated by the same path as the user action, not by a test overlay that
   bypasses transport.
3. Warm up the devices and transport, then discard at least 30 warm-up events.
   Capture at least 100 measured events in each declared condition. Use the
   same display refresh rate, resolution, power mode, network/USB path, app
   build, permissions, and background load for the complete run.
4. For each trial, identify the first camera frame showing the input marker and
   the first frame showing a stable correct response. Convert the frame delta
   using the measured camera rate and retain the frame numbers or clips as the
   raw artifact. Record dropped, late, duplicated, and incorrect actions as
   failures, not as omitted samples.
5. Report median/p50, p95, p99, maximum, sample count, drop/misapply rate, and
   an uncertainty note of at least ± one camera frame (about ±4.167 ms at the
   target rate). Also report a stage breakdown if instrumentation is available,
   but do not substitute the breakdown for the physical end-to-end result.

### Conditions and pass bar

Run a **reference** condition (the selected device pair, stable local
transport, foreground apps) and a **stress** condition (the declared realistic
network/background/thermal constraint). The hard gate uses the reference
condition and the threshold frozen at G0 after the baseline rehearsal. The
initial 80 ms median aspiration is a stretch goal, not a pre-existing pass bar.
Report the action-error budget separately and do not tune either threshold after
seeing candidate results. The stress result is reported with its own
distribution and informs the platform score; it must not be silently merged
into the reference result.

The current Rust tests do not measure latency. A protocol round trip or a log
timestamp is not motion-to-photon evidence, and no latency claim may be made
until the camera artifact and environment sheet exist.

## 4. Reliability and security runs

### Lifecycle run

Use the same physical device pair for at least 100 cycles containing a mixture
of clean disconnects, abrupt transport loss, heartbeat timeout, reconnect within
grace, reconnect after grace, duplicate start, explicit terminate, and process
shutdown. Assert that there is at most one active session per device, a
resumable disconnect retains the expected session identity, terminated sessions
cannot reconnect, stale records are removed, and no control action is delivered
to a superseded device. Preserve the first failing trace and state snapshots.

### Security run

The real path must exercise cryptographic identity and authenticated/encrypted
messages. Negative tests must cover an unknown identity, wrong credential,
tampered frame/tag, replayed message, revoked/expired identity, unavailable
security provider, denied Android/macOS permission, malformed frame, oversized
payload, and unauthorized capability. The current exact-byte fixtures can
support deterministic core tests only; they cannot pass this run.

## 5. Deliverables and decision packet

At Day 15, retain these artifacts under the project’s agreed evidence location
or link them from the review record:

- clean Linux build/test output and toolchain/lockfile revision;
- protocol/state contract and list of known compatibility gaps;
- Android/macOS device, OS, permission, entitlement, and transport sheet;
- raw 240-fps clips/frame indices, calculation script or reproducible method,
  distributions, and failure count;
- lifecycle traces and security negative-test results;
- weighted score (30% / 25% / 15% / 20% / 10%), hard-gate status, and open risks;
- the support matrix and a decision with owner, date, scope, and next step.
