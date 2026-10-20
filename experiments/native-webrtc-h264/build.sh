#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
BUILD="$ROOT/.build"
TOOLS="$BUILD/tools"
DEPS="$BUILD/deps"
LIBDATACHANNEL_SOURCE="$DEPS/libdatachannel"
LIBDATACHANNEL_REF="6b1e2e620f1e37f0eafeee702eaea0043cb305fd"
CMAKE_VERSION="3.31.10"

if ! command -v xcrun >/dev/null 2>&1; then
    printf 'ERROR: xcrun is required to locate the macOS SDK.\n' >&2
    exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
    printf 'ERROR: python3 is required to bootstrap project-local CMake.\n' >&2
    exit 2
fi
if ! command -v git >/dev/null 2>&1; then
    printf 'ERROR: git is required to fetch the pinned libdatachannel source.\n' >&2
    exit 2
fi

mkdir -p "$BUILD" "$TOOLS" "$DEPS"

CMAKE_ROOT="$TOOLS/cmake-$CMAKE_VERSION"
if [[ ! -x "$CMAKE_ROOT/bin/cmake" ]]; then
    mkdir -p "$CMAKE_ROOT"
    python3 -m pip install \
        --disable-pip-version-check \
        --no-input \
        --target "$CMAKE_ROOT" \
        "cmake==$CMAKE_VERSION"
fi

CMAKE_PYTHONPATH="$CMAKE_ROOT"
if [[ -x "$CMAKE_ROOT/cmake/data/bin/cmake" ]]; then
    CMAKE_BIN="$CMAKE_ROOT/cmake/data/bin/cmake"
else
    CMAKE_BIN="$CMAKE_ROOT/bin/cmake"
fi
CMAKE=(env "PYTHONPATH=$CMAKE_PYTHONPATH" "$CMAKE_BIN")

if [[ ! -d "$LIBDATACHANNEL_SOURCE/.git" ]]; then
    git clone --filter=blob:none --no-checkout \
        https://github.com/paullouisageneau/libdatachannel.git \
        "$LIBDATACHANNEL_SOURCE"
fi
git -C "$LIBDATACHANNEL_SOURCE" fetch --depth 1 origin "$LIBDATACHANNEL_REF"
git -C "$LIBDATACHANNEL_SOURCE" checkout --detach "$LIBDATACHANNEL_REF"
git -C "$LIBDATACHANNEL_SOURCE" submodule update --init --recursive --depth 1

ACTUAL_REF="$(git -C "$LIBDATACHANNEL_SOURCE" rev-parse HEAD)"
if [[ "$ACTUAL_REF" != "$LIBDATACHANNEL_REF" ]]; then
    printf 'ERROR: libdatachannel revision mismatch: %s\n' "$ACTUAL_REF" >&2
    exit 2
fi

if command -v brew >/dev/null 2>&1; then
    OPENSSL_ROOT_DIR="$(brew --prefix openssl@3 2>/dev/null || true)"
else
    OPENSSL_ROOT_DIR=""
fi
if [[ -z "$OPENSSL_ROOT_DIR" || ! -d "$OPENSSL_ROOT_DIR" ]]; then
    printf 'ERROR: Homebrew OpenSSL 3 is required; set OPENSSL_ROOT_DIR to a local OpenSSL 3 prefix.\n' >&2
    exit 2
fi

SDK_PATH="$(xcrun --sdk macosx --show-sdk-path)"
ARCH="$(uname -m)"
"${CMAKE[@]}" \
    -S "$ROOT" \
    -B "$BUILD/native" \
    -G "Unix Makefiles" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_ARCHITECTURES="$ARCH" \
    -DCMAKE_OSX_SYSROOT="$SDK_PATH" \
    -DOPENSSL_ROOT_DIR="$OPENSSL_ROOT_DIR" \
    -DLIBDATACHANNEL_SOURCE_DIR="$LIBDATACHANNEL_SOURCE"

"${CMAKE[@]}" --build "$BUILD/native" --parallel 2
bash "$ROOT/record-environment.sh" "$BUILD/environment.txt" "$LIBDATACHANNEL_SOURCE" "$CMAKE_BIN"
printf 'Built probe: %s\n' "$BUILD/native/native-webrtc-h264"
