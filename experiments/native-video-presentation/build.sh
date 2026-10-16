#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
BUILD="$ROOT/.build"
mkdir -p "$BUILD"

if ! command -v xcrun >/dev/null 2>&1; then
  printf 'ERROR: xcrun is required to locate the macOS SDK and swiftc.\n' >&2
  exit 2
fi

SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
SWIFTC="$(xcrun --sdk macosx --find swiftc)"

"$SWIFTC" \
  -O \
  -parse-as-library \
  -sdk "$SDK_PATH" \
  -framework AppKit \
  -framework AVFoundation \
  -framework CoreMedia \
  -framework CoreVideo \
  -framework QuartzCore \
  "$ROOT/Sources/main.swift" \
  -o "$BUILD/native-video-presentation-probe"

bash "$ROOT/record-environment.sh" "$BUILD/environment.txt"
printf 'Built probe: %s\n' "$BUILD/native-video-presentation-probe"
