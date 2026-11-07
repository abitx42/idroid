#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)"
NDK="${ANDROID_NDK_ROOT:-${ANDROID_NDK_HOME:-}}"
if [[ -z "$NDK" ]]; then
    for candidate in \
        "$ANDROID_SDK_ROOT/ndk/28.2.13676358" \
        "$HOME/Library/Android/sdk/ndk/28.2.13676358" \
        "/opt/homebrew/share/android-commandlinetools/ndk/28.2.13676358"; do
        if [[ -d "$candidate/toolchains/llvm/prebuilt" ]]; then
            NDK="$candidate"
            break
        fi
    done
fi
if [[ -z "$NDK" ]]; then
    printf '%s\n' 'ERROR: set ANDROID_NDK_ROOT to an installed Android NDK' >&2
    exit 2
fi

TARGET=aarch64-linux-android
PREBUILT=""
for candidate in "$NDK/toolchains/llvm/prebuilt"/*; do
    if [[ -x "$candidate/bin/${TARGET}35-clang" ]]; then
        PREBUILT="$candidate"
        break
    fi
done
CLANG="$PREBUILT/bin/${TARGET}35-clang"
if [[ ! -x "$CLANG" ]]; then
    printf 'ERROR: Android NDK compiler not found: %s\n' "$CLANG" >&2
    exit 2
fi

PATH="/opt/homebrew/opt/rustup/bin:$PATH"
rustup target list --installed | grep -qx "$TARGET" || rustup target add "$TARGET"
AR="$PREBUILT/bin/llvm-ar"
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG" \
CC_aarch64_linux_android="$CLANG" AR_aarch64_linux_android="$AR" \
    cargo build --locked --offline --release \
    --manifest-path "$ROOT/Cargo.toml" -p idroid-core-ffi --target "$TARGET"

GRADLE="$ROOT/experiments/android-macos-vertical-slice/.build/gradle-8.7/bin/gradle"
if [[ ! -x "$GRADLE" ]]; then
    ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}" \
        bash "$ROOT/experiments/android-macos-vertical-slice/build-android.sh"
fi
GRADLE="$ROOT/experiments/android-macos-vertical-slice/.build/gradle-8.7/bin/gradle"
if [[ ! -x "$GRADLE" ]]; then
    printf '%s\n' 'Gradle 8.7 bootstrap did not complete.' >&2
    exit 2
fi
SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-$HOME/Library/Android/sdk}}"
printf 'sdk.dir=%s\n' "$SDK_ROOT" \
    > "$ROOT/experiments/android-macos-vertical-slice/local.properties"
"$GRADLE" -p "$ROOT/experiments/android-macos-vertical-slice" --no-daemon \
    -PidroidNdkPath="$NDK" \
    -PidroidCoreFfiLibDir="$ROOT/target/$TARGET/release" :app:assembleDebug
