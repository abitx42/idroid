#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
bash "$ROOT/build.sh"

ARGS=("$@")
has_output=0
has_environment=0
for arg in "$@"; do
  case "$arg" in
    --output) has_output=1 ;;
    --environment) has_environment=1 ;;
  esac
done
if [[ "$has_output" -eq 0 ]]; then
  ARGS+=(--output "$ROOT/results/android-remote.json")
fi
if [[ "$has_environment" -eq 0 ]]; then
  ARGS+=(--environment "$ROOT/.build/environment.txt")
fi

exec "$ROOT/.build/native/android-webrtc-receiver" "${ARGS[@]}"
