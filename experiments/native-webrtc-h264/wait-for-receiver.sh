#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -lt 5 || "$#" -gt 6 ]]; then
  printf 'usage: %s RECEIVER BIND_ADDRESS PORT OUTPUT ACCEPT_TIMEOUT_SECONDS [READINESS_TIMEOUT_SECONDS]\n' "$0" >&2
  exit 64
fi

receiver="$1"
bind_address="$2"
port="$3"
output="$4"
accept_timeout="$5"
readiness_timeout="${6:-10}"

case "$readiness_timeout" in
  ''|*[!0-9]*) printf 'invalid readiness timeout\n' >&2; exit 64 ;;
esac
if (( readiness_timeout < 1 )); then
  printf 'readiness timeout must be positive\n' >&2
  exit 64
fi

deadline=$((SECONDS + readiness_timeout))
while (( SECONDS < deadline )); do
  pids=()
  while IFS= read -r pid; do
    [[ -n "$pid" ]] && pids+=("$pid")
  done < <(lsof -nP -t -a -iTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)

  if (( ${#pids[@]} == 1 )); then
    pid="${pids[0]}"
    endpoint_ok=0
    while IFS= read -r field; do
      [[ "$field" == "n${bind_address}:${port}" ]] && endpoint_ok=1
    done < <(lsof -nP -a -p "$pid" -iTCP:"$port" -sTCP:LISTEN -F n 2>/dev/null || true)

    executable_ok=0
    while IFS= read -r field; do
      [[ "$field" == "n$receiver" ]] && executable_ok=1
    done < <(lsof -nP -a -p "$pid" -d txt -F n 2>/dev/null || true)

    command_line="$(ps -p "$pid" -o command= 2>/dev/null || true)"
    command_ok=0
    if [[ "$endpoint_ok" -eq 1 && "$executable_ok" -eq 1 ]]; then
      if python3 - "$receiver" "$bind_address" "$port" "$accept_timeout" "$output" "$command_line" <<'PY'
import shlex
import sys

receiver, bind_address, port, accept_timeout, output, command_line = sys.argv[1:]
try:
    argv = shlex.split(command_line)
except ValueError:
    raise SystemExit(1)
expected = [
    receiver,
    "--bind-address", bind_address,
    "--port", port,
    "--accept-timeout-seconds", accept_timeout,
    "--output", output,
]
if not argv or argv[0] != receiver:
    raise SystemExit(1)
for key, value in zip(expected[1::2], expected[2::2]):
    try:
        index = argv.index(key)
    except ValueError:
        raise SystemExit(1)
    if index + 1 >= len(argv) or argv[index + 1] != value:
        raise SystemExit(1)
PY
      then
        command_ok=1
      fi
    fi

    if [[ "$endpoint_ok" -eq 1 && "$executable_ok" -eq 1 && "$command_ok" -eq 1 ]]; then
      printf 'receiver_ready pid=%s endpoint=%s:%s\n' "$pid" "$bind_address" "$port"
      exit 0
    fi
  fi
  sleep 0.2
done

printf 'STOP: expected receiver listener was not verified within %ss; no Android capture should be granted\n' "$readiness_timeout" >&2
exit 1
