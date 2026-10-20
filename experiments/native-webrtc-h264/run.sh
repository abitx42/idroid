#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
bash "$ROOT/build.sh"

exec "$ROOT/.build/native/native-webrtc-h264" "$@"
