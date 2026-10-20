#!/usr/bin/env bash
# Host-only signaling tests. Never installs/launches an APK or connects a phone.
set -euo pipefail
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
GRADLE="$ROOT/.build/gradle-8.7"
"$GRADLE/bin/gradle" -p "$ROOT" --offline --no-daemon :app:compileDebugUnitTestKotlin
python3 - "$ROOT" "${GRADLE_USER_HOME:-$HOME/.gradle}" <<'PY'
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import zipfile

root, cache = map(Path, sys.argv[1:])
artifacts = sorted((cache / "caches/modules-2/files-2.1/io.github.webrtc-sdk/android/125.6422.07").glob("*/*.aar"))
if len(artifacts) != 1:
    sys.exit("Expected one cached locked WebRTC AAR; build the APK first")
runtime = Path(tempfile.mkdtemp(prefix="jvm-diagnostics-", dir=root / ".build"))
with zipfile.ZipFile(artifacts[0]) as archive:
    (runtime / "classes.jar").write_bytes(archive.read("classes.jar"))
classpath = os.pathsep.join(map(str, [
    root / "app/build/tmp/kotlin-classes/debug",
    root / "app/build/tmp/kotlin-classes/debugUnitTest",
    root / ".build/gradle-8.7/lib/kotlin-stdlib-1.9.22.jar",
    runtime / "classes.jar",
]))
subprocess.run(["java", "-cp", classpath, "com.idroid.verticalslice.SignalingDiagnosticsTest"], check=True)
PY
