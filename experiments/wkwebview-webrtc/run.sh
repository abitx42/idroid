#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
bash "$ROOT/build.sh"

OUTPUT="$ROOT/results/wkwebview-loopback.json"
if [[ "${1:-}" == "--output" ]]; then
  OUTPUT="$2"
  shift 2
fi

mkdir -p "$(dirname -- "$OUTPUT")"
exec "$ROOT/.build/wkwebview-webrtc-probe" \
  --html "$ROOT/Web/index.html" \
  --environment "$ROOT/.build/environment.txt" \
  --output "$OUTPUT" \
  "$@"
