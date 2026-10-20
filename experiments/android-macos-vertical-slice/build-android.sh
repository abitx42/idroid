#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"
GRADLE_VERSION="8.7"
GRADLE_ROOT="$ROOT/.build/gradle-$GRADLE_VERSION"

if [[ -z "$SDK_ROOT" ]]; then
    for candidate in "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
        if [[ -d "$candidate/platforms" ]]; then
            SDK_ROOT="$candidate"
            break
        fi
    done
fi

if [[ -z "$SDK_ROOT" ]]; then
    printf 'ERROR: set ANDROID_SDK_ROOT (or ANDROID_HOME) to an Android SDK containing platforms/\n' >&2
    exit 2
fi

if ! command -v curl >/dev/null 2>&1 || ! command -v unzip >/dev/null 2>&1; then
    printf 'ERROR: curl and unzip are required to bootstrap the pinned Gradle distribution\n' >&2
    exit 2
fi

if [[ ! -x "$GRADLE_ROOT/bin/gradle" ]]; then
    mkdir -p "$ROOT/.build"
    ARCHIVE="$ROOT/.build/gradle-$GRADLE_VERSION-bin.zip"
    curl --fail --location --silent --show-error \
        "https://services.gradle.org/distributions/gradle-$GRADLE_VERSION-bin.zip" \
        --output "$ARCHIVE"
    unzip -q -o "$ARCHIVE" -d "$ROOT/.build"
fi

if [[ ! -d "$SDK_ROOT/platforms" ]]; then
    printf 'ERROR: Android SDK root not found: %s\n' "$SDK_ROOT" >&2
    exit 2
fi

printf 'sdk.dir=%s\n' "$SDK_ROOT" > "$ROOT/local.properties"
GRADLE_ARGS=(--no-daemon assembleDebug)
if [[ "${IDROID_ANDROID_OFFLINE:-0}" == "1" ]]; then
    GRADLE_ARGS+=(--offline)
fi
"$GRADLE_ROOT/bin/gradle" -p "$ROOT" "${GRADLE_ARGS[@]}"
printf 'APK: %s\n' "$ROOT/app/build/outputs/apk/debug/app-debug.apk"
