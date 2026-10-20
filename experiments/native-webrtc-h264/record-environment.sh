#!/usr/bin/env bash
set -euo pipefail

OUTPUT="${1:?output path is required}"
LIBDATACHANNEL_SOURCE="${2:?libdatachannel source path is required}"
CMAKE_BIN="${3:?cmake path is required}"
mkdir -p "$(dirname -- "$OUTPUT")"

{
    printf 'recorded_at_utc='; date -u '+%Y-%m-%dT%H:%M:%SZ'
    uname -a
    sw_vers 2>/dev/null || true
    printf 'developer_directory='; xcode-select -p 2>/dev/null || true
    printf 'macos_sdk_version='; xcrun --sdk macosx --show-sdk-version 2>/dev/null || true
    printf 'clang_version='; xcrun --sdk macosx clang++ --version 2>/dev/null | python3 -c 'import sys; print(sys.stdin.readline().strip())'
    printf 'cmake_version='; "$CMAKE_BIN" --version 2>/dev/null | python3 -c 'import sys; print(sys.stdin.readline().strip())'
    printf 'libdatachannel_revision='; git -C "$LIBDATACHANNEL_SOURCE" rev-parse HEAD
    printf 'libdatachannel_submodules=\n'; git -C "$LIBDATACHANNEL_SOURCE" submodule status
} > "$OUTPUT"

printf 'Environment record: %s\n' "$OUTPUT"
