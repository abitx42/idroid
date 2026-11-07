#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
BUILD="$ROOT/.build"
mkdir -p "$BUILD"

cc -std=c11 -O2 -Wall -Wextra \
  "$ROOT/Sources/clock_probe.c" \
  -o "$BUILD/clock-probe"

BEFORE="$BUILD/before.txt"
AFTER="$BUILD/after.txt"
"$BUILD/clock-probe" before | tee "$BEFORE"
printf '%s\n' 'The next command sleeps this Mac. Wake it manually to continue.'
/usr/bin/sudo /usr/bin/pmset sleepnow
"$BUILD/clock-probe" after | tee "$AFTER"

python3 - "$BEFORE" "$AFTER" <<'PY'
import json
import re
import sys

pattern = re.compile(r"^\w+ monotonic_ns=(\d+) uptime_raw_ns=(\d+)$")

def read(path):
    match = pattern.match(open(path, encoding="ascii").read().strip())
    if match is None:
        raise SystemExit(f"invalid probe output: {path}")
    return tuple(map(int, match.groups()))

before = read(sys.argv[1])
after = read(sys.argv[2])
monotonic_delta = after[0] - before[0]
uptime_delta = after[1] - before[1]
result = {
    "monotonicDeltaMs": monotonic_delta / 1_000_000,
    "uptimeRawDeltaMs": uptime_delta / 1_000_000,
    "sleepIncludedByMonotonic": monotonic_delta > uptime_delta + 1_000_000,
}
print(json.dumps(result, indent=2, sort_keys=True))
if not result["sleepIncludedByMonotonic"]:
    raise SystemExit("probe did not observe a sleep interval")
PY
