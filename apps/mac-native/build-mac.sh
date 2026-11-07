#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
APP="$ROOT/apps/mac-native"

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
  cargo build --locked --offline --release -p idroid-core-ffi --manifest-path "$ROOT/Cargo.toml"

IDROID_CORE_FFI_DIR="$ROOT/target/release" \
  swift build --package-path "$APP" -c release

printf 'Binary: %s\n' "$APP/.build/release/idroid-mac-native"
