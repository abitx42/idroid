# Latency benchmark scaffold

**Status:** benchmark-plan scaffold only. No latency harness or measurements
exist in this directory.

**Owner:** performance owner (TBD), with protocol and platform owners defining
representative workloads.

## Acceptance criteria

- The workload, message size, transport path, platform, build profile, CPU,
  and toolchain are recorded for every run.
- Warm-up, sample count, clock source, percentile calculations, and outlier
  handling are defined before collecting numbers.
- Results separate codec, scheduling, transport, and end-to-end latency.
- Baselines and regression thresholds are reproducible offline where possible
  and never use synthetic results as device claims.
- A result is linked to a commit and stored with enough metadata to reproduce
  it.

Until a harness and recorded run exist, the project makes no latency claim.
