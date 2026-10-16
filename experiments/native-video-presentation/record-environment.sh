#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
OUTPUT="${1:-$ROOT/.build/environment.txt}"
mkdir -p "$(dirname -- "$OUTPUT")"

{
  printf 'recorded_at_utc='; date -u '+%Y-%m-%dT%H:%M:%SZ'
  printf 'uname='; uname -a
  if command -v sw_vers >/dev/null 2>&1; then sw_vers; else printf 'sw_vers=NOT_AVAILABLE\n'; fi
  if command -v xcode-select >/dev/null 2>&1; then
    printf 'developer_directory='; xcode-select -p 2>&1 || true
  else
    printf 'developer_directory=NOT_AVAILABLE\n'
  fi
  if command -v xcrun >/dev/null 2>&1; then
    printf 'macos_sdk_version='; xcrun --sdk macosx --show-sdk-version 2>&1 || true
    printf 'swiftc_path='; xcrun --sdk macosx --find swiftc 2>&1 || true
    printf 'swiftc_version=\n'; xcrun --sdk macosx swiftc --version 2>&1 || true
  else
    printf 'xcrun=NOT_AVAILABLE\n'
  fi
  printf 'git_revision='; git -C "$ROOT/../.." rev-parse HEAD 2>&1 || true
} >"$OUTPUT"

printf 'Environment record: %s\n' "$OUTPUT"
