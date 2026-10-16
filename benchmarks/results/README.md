# Benchmark results scaffold

**Status:** evidence-index scaffold only. No benchmark results are recorded in
this directory.

**Owner:** performance/release evidence owner (TBD).

## Acceptance criteria for a recorded result

- Each result names the benchmark, commit, toolchain, platform, hardware,
  build profile, workload, configuration, seed, and timestamp.
- Raw output and the summary use consistent units and state sample count,
  percentiles, failures, and omitted runs.
- Results link to the corresponding latency or soak procedure and are
  independently reproducible from a clean checkout.
- A comparison states the baseline and threshold; a single favorable run is
  not a support claim.
- Security-sensitive values and user/device identifiers are redacted before
  publication.

Do not add placeholder numbers here. An empty evidence index is more accurate
than an unverified performance claim.
