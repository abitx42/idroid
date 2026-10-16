#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
bash "$ROOT/build.sh"

OUTPUT="$ROOT/results/native-video-presentation.json"
if [[ "${1:-}" == "--output" ]]; then
  OUTPUT="$2"
  shift 2
fi

mkdir -p "$(dirname -- "$OUTPUT")"
exec "$ROOT/.build/native-video-presentation-probe" \
  --environment "$ROOT/.build/environment.txt" \
  --output "$OUTPUT" \
  "$@"
