#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
APP="$ROOT/apps/mac-native"

PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
  cargo build --locked --offline --release -p idroid-core-ffi --manifest-path "$ROOT/Cargo.toml"

IDROID_CORE_FFI_DIR="$ROOT/target/release" \
  swift build --package-path "$APP" -c release

APP_BUNDLE="$APP/.build/release/iDroid.app"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

cp "$APP/.build/release/idroid-mac-native" "$APP_BUNDLE/Contents/MacOS/idroid-mac-native"
cp "$APP/Info.plist" "$APP_BUNDLE/Contents/Info.plist"

if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - --entitlements "$APP/entitlements.plist" "$APP_BUNDLE"
fi

printf 'Binary: %s\n' "$APP/.build/release/idroid-mac-native"
printf 'Bundle: %s\n' "$APP_BUNDLE"
