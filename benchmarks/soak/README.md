# Soak benchmark scaffold

**Status:** benchmark-plan scaffold only. No long-running soak harness or
measurements exist in this directory.

**Owner:** reliability/performance owner (TBD), with platform owners for
resource telemetry.

## Acceptance criteria

- The duration, event rate, reconnect/fault schedule, data sizes, and random
  seed are fixed and recorded for each run.
- Memory, CPU, file descriptors, queues, battery/thermal state, and session
  count are measured with clear start/end conditions.
- The harness detects leaked sessions, unbounded growth, stale cleanup errors,
  and silent data loss rather than only checking process exit status.
- Runs identify OS, hardware, toolchain, build profile, and exact commit.
- A passing soak result is scoped to its workload and does not imply support
  for every platform or device.

No endurance or resource-use claim is made until those measurements exist.
