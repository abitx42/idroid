#!/usr/bin/env python3
"""Host-only tests for collector startup/status handling."""

from pathlib import Path
import importlib.util
import os
import stat
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parent
COLLECTOR = ROOT / "capture-diagnostics.py"
READINESS = ROOT / "collector-readiness.py"
VALID_LINE = "timestamp_ms=1735689600123 diagnostic=START_BUTTON_PRESSED stage=session status=ok"
VALID_STATUSES = {
    "STARTING",
    "COLLECTING_NO_EVENTS",
    "COLLECTING_EVENTS",
    "STARTUP_FAILED",
    "COLLECTION_FAILED",
    "STOPPED",
}


FAKE_ADB = '''#!/usr/bin/env python3
import os
import sys
import time

mode = os.environ["FAKE_ADB_MODE"]
pid_path = os.environ.get("FAKE_ADB_PID")
attempt_path = os.environ.get("FAKE_ADB_ATTEMPT")
if pid_path and not os.path.exists(pid_path):
    with open(pid_path, "x", encoding="ascii") as output:
        output.write(str(os.getpid()))
if mode == "fail":
    sys.stderr.write("unrestricted fake stderr\\n")
    raise SystemExit(17)
if mode == "events":
    print("timestamp_ms=1735689600123 diagnostic=START_BUTTON_PRESSED stage=session status=ok", flush=True)
    print("unrestricted raw fixture", flush=True)
if mode == "flaky":
    if not attempt_path or not os.path.exists(attempt_path):
        if attempt_path:
            with open(attempt_path, "w", encoding="ascii") as output:
                output.write("first\\n")
        time.sleep(0.7)
        raise SystemExit(17)
    print("timestamp_ms=1735689600123 diagnostic=SCREEN_FRAME_CAPTURED stage=screen_capture status=ok", flush=True)
while True:
    time.sleep(1)
'''


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


collector_module = load_module(COLLECTOR, "capture_diagnostics")
readiness_module = load_module(READINESS, "collector_readiness")


def wait_for_status(path, expected, process):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if path.exists():
            value = path.read_text(encoding="ascii").strip()
            if value == expected:
                return
        if process.poll() is not None and expected not in {"STARTUP_FAILED", "COLLECTION_FAILED"}:
            break
        time.sleep(0.02)
    raise AssertionError("collector status was not observed")


class CollectorReadinessTests(unittest.TestCase):
    def setUp(self):
        self.directory = Path(tempfile.mkdtemp(prefix="idroid-collector-test-"))
        self.fake_adb = self.directory / "fake-adb.py"
        self.fake_adb.write_text(FAKE_ADB, encoding="utf-8")
        self.fake_adb.chmod(self.fake_adb.stat().st_mode | stat.S_IXUSR)

    def start(self, mode):
        output = self.directory / f"{mode}.android.log"
        status = self.directory / f"{mode}.status"
        environment = os.environ.copy()
        environment["FAKE_ADB_MODE"] = mode
        environment["FAKE_ADB_PID"] = str(self.directory / f"{mode}.pid")
        environment["FAKE_ADB_ATTEMPT"] = str(self.directory / f"{mode}.attempt")
        process = subprocess.Popen(
            [sys.executable, str(COLLECTOR), "--adb", str(self.fake_adb),
             "--serial", "synthetic", "--output", str(output), "--status", str(status)],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return process, output, status

    def test_successful_startup_reports_no_events_then_events(self):
        process, output, status = self.start("events")
        try:
            wait_for_status(status, "COLLECTING_EVENTS", process)
            self.assertEqual(output.read_text(encoding="utf-8"), VALID_LINE + "\n")
            self.assertTrue(set(status.read_text(encoding="ascii").split()) <= VALID_STATUSES)
        finally:
            process.terminate()
            process.communicate(timeout=5)

    def test_immediate_subprocess_failure_is_startup_failure(self):
        process, output, status = self.start("fail")
        stdout, stderr = process.communicate(timeout=5)
        self.assertNotEqual(process.returncode, 0)
        self.assertEqual(status.read_text(encoding="ascii").strip(), "STARTUP_FAILED")
        self.assertEqual(output.read_text(encoding="utf-8"), "")
        self.assertEqual(stdout, "")
        self.assertNotIn("unrestricted fake stderr", stderr)

    def test_transient_collection_failure_is_restarted(self):
        process, output, status = self.start("flaky")
        try:
            wait_for_status(status, "COLLECTING_EVENTS", process)
            self.assertEqual(
                output.read_text(encoding="utf-8"),
                "timestamp_ms=1735689600123 diagnostic=SCREEN_FRAME_CAPTURED stage=screen_capture status=ok\n",
            )
        finally:
            process.terminate()
            process.communicate(timeout=5)

    def test_empty_but_running_collection_is_not_failure(self):
        process, output, status = self.start("empty")
        try:
            wait_for_status(status, "COLLECTING_NO_EVENTS", process)
            self.assertEqual(output.read_text(encoding="utf-8"), "")
            self.assertIsNone(process.poll())
        finally:
            process.terminate()
            process.communicate(timeout=5)

    def test_shutdown_terminates_and_reaps_child(self):
        process, _output, status = self.start("empty")
        wait_for_status(status, "COLLECTING_NO_EVENTS", process)
        child_pid_path = self.directory / "empty.pid"
        deadline = time.monotonic() + 5
        while not child_pid_path.exists() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(child_pid_path.exists())
        process.terminate()
        process.communicate(timeout=5)
        self.assertEqual(process.returncode, 0)
        self.assertEqual(status.read_text(encoding="ascii").strip(), "STOPPED")
        child_pid = int(child_pid_path.read_text(encoding="ascii"))
        with self.assertRaises(ProcessLookupError):
            os.kill(child_pid, 0)

    def test_readiness_rejects_process_that_exits_after_publishing_status(self):
        status = self.directory / "stale.status"
        status.write_text("COLLECTING_NO_EVENTS\n", encoding="ascii")
        process = subprocess.Popen([sys.executable, "-c", "pass"])
        process.wait(timeout=5)
        self.assertNotEqual(readiness_module.check_readiness(status, process.pid), 0)

    def test_consent_boundary_recheck_rejects_collector_that_exited(self):
        status = self.directory / "boundary.status"
        status.write_text("COLLECTING_NO_EVENTS\n", encoding="ascii")
        process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        try:
            self.assertEqual(readiness_module.check_readiness(status, process.pid), 0)
            process.terminate()
            process.wait(timeout=5)
            self.assertNotEqual(readiness_module.check_readiness(status, process.pid), 0)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)

    def test_interrupt_before_child_creation_has_distinct_failure_status(self):
        self.assertEqual(collector_module.shutdown_status(None), "INTERRUPTED_BEFORE_START")

    def test_malformed_status_is_rejected(self):
        status = self.directory / "malformed.status"
        status.write_text("COLLECTING_NO_EVENTS\nextra\n", encoding="ascii")
        process = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        try:
            self.assertNotEqual(readiness_module.check_readiness(status, process.pid), 0)
        finally:
            process.terminate()
            process.wait(timeout=5)

    def test_nonpositive_pid_is_rejected_without_process_group_probe(self):
        status = self.directory / "invalid-pid.status"
        status.write_text("COLLECTING_NO_EVENTS\n", encoding="ascii")
        self.assertNotEqual(readiness_module.check_readiness(status, 0), 0)
        self.assertNotEqual(readiness_module.check_readiness(status, -1), 0)

    def test_shutdown_failure_cannot_report_stopped(self):
        class BrokenChild:
            def poll(self):
                return None

            def terminate(self):
                raise OSError("synthetic termination failure")

        self.assertEqual(collector_module.shutdown_status(BrokenChild()), "SHUTDOWN_FAILED")


if __name__ == "__main__":
    unittest.main()
