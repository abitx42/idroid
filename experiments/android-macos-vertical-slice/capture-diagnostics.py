#!/usr/bin/env python3
"""Persist only app-owned diagnostic tokens, never raw logcat/pairing material."""

import argparse
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time


STARTUP_GRACE_SECONDS = 0.5
COLLECTOR_STATUSES = {
    "STARTING",
    "COLLECTING_NO_EVENTS",
    "COLLECTING_EVENTS",
    "STARTUP_FAILED",
    "COLLECTION_FAILED",
    "INTERRUPTED_BEFORE_START",
    "SHUTDOWN_FAILED",
    "STOPPED",
}


DIAGNOSTICS = {
    "CAPTURE_SERVICE_STARTING", "CAPTURE_SERVICE_START_FAILED", "CAPTURE_START_FAILED",
    "CAPTURE_STOPPED_BY_ANDROID", "CAPTURE_SERVICE_DESTROYING", "CAPTURE_SERVICE_DESTROYED",
    "CAPTURE_CLEANUP_FAILED", "USER_STOP_REQUESTED", "NOTIFICATION_CONSENT_DENIED",
    "CAPTURE_CONSENT_DENIED", "CAPTURE_CONSENT_GRANTED", "LOCAL_DESCRIPTION_SET",
    "REMOTE_DESCRIPTION_SET", "SDP_CREATE_FAILED", "SDP_LOCAL_FAILED", "SDP_REMOTE_FAILED",
    "ICE_GATHERING_TIMEOUT", "ICE_CONNECTION_FAILED", "PEER_CONNECTION_FAILED",
    "SIGNALING_FAILED", "INVALID_SIGNALING_INPUT", "OFFER_PREPARATION_FAILED",
    "OFFER_TOO_LARGE", "TCP_CONNECTION_FAILED", "TCP_CONNECT_TIMEOUT", "SIGNALING_WRITE_FAILED",
    "ANSWER_READ_FAILED", "ANSWER_READ_TIMEOUT", "MALFORMED_SIGNALING", "AUTHENTICATION_FAILED",
    "TCP_CONNECTING", "TCP_CONNECTED", "OFFER_SENT", "ANSWER_AUTHENTICATED",
    "START_BUTTON_PRESSED", "PROJECTION_CONSENT_GRANTED", "CAPTURE_SERVICE_STARTED",
    "SCREEN_CAPTURE_STARTED", "SDP_OFFER_CREATED", "ICE_GATHERING_STARTED", "ICE_GATHERING_COMPLETE",
    "SIGNALING_ERROR",
}
STAGES = {
    "tcp_connect", "signaling_input", "signaling_prepare", "signaling_send", "signaling_response",
    "signaling_auth", "signaling", "ice_gather", "ice_connection", "peer_connection",
    "sdp_create", "sdp_local", "sdp_remote", "sdp", "tcp", "ice", "session", "projection_consent",
    "screen_capture", "capture_start", "service_start", "cleanup",
}
CATEGORIES = {
    "started", "connected", "waiting", "authenticated", "failed", "timeout", "malformed",
    "invalid", "too_large", "io_failed", "set", "user_stop", "projection_stop", "state",
    "ok", "stopped", "authenticated",
}
STATUSES = {"ok", "failed", "stopped"}
FAILURES = {
    "NOTIFICATION_PERMISSION_REQUIRED", "NOTIFICATION_PERMISSION_DENIED",
    "PROJECTION_CONSENT_REQUEST_FAILED", "CAPTURE_CONSENT_DENIED", "SERVICE_START_FAILED",
    "SCREEN_CAPTURE_FAILED", "SDP_OFFER_FAILED", "SDP_LOCAL_DESCRIPTION_FAILED",
    "SDP_REMOTE_DESCRIPTION_FAILED", "ICE_GATHERING_TIMEOUT", "ICE_CONNECTION_FAILED",
    "PEER_CONNECTION_FAILED", "TCP_CONNECTION_FAILED", "TCP_CONNECT_TIMEOUT",
    "SIGNALING_WRITE_FAILED", "ANSWER_READ_FAILED", "ANSWER_READ_TIMEOUT",
    "MALFORMED_SIGNALING", "AUTHENTICATION_FAILED", "INVALID_SIGNALING_INPUT",
    "OFFER_TOO_LARGE", "SIGNALING_FAILED",
}
STATES = {
    "NEW", "GATHERING", "COMPLETE", "CHECKING", "CONNECTED", "COMPLETED", "FAILED",
    "DISCONNECTED", "CLOSED", "CONNECTING", "STABLE", "HAVE_LOCAL_OFFER", "HAVE_REMOTE_OFFER",
    "HAVE_LOCAL_PRANSWER", "HAVE_REMOTE_PRANSWER", "UNKNOWN",
}


def redact(line):
    """Reject the entire line unless all fields belong to closed vocabularies."""
    line = line.strip()
    if len(line) > 512 or not line:
        return None
    fields = {}
    for token in line.split():
        if not re.fullmatch(r"[a-z_]+=[A-Za-z0-9_]+", token):
            return None
        key, value = token.split("=", 1)
        if key in fields:
            return None
        fields[key] = value
    allowed = {"diagnostic": DIAGNOSTICS, "stage": STAGES, "category": CATEGORIES,
               "state": STATES, "timeout_ms": {"5000", "10000"},
               "timestamp_ms": None, "status": STATUSES, "failure": FAILURES}
    if any(
        key not in allowed
        or (allowed[key] is not None and value not in allowed[key])
        or (key == "timestamp_ms" and not re.fullmatch(r"[0-9]{1,13}", value))
        for key, value in fields.items()
    ):
        return None
    if "status" in fields:
        if set(fields) - {"timestamp_ms", "diagnostic", "stage", "status", "failure"}:
            return None
        if not {"timestamp_ms", "diagnostic", "stage", "status"}.issubset(fields):
            return None
        if "failure" in fields and fields["status"] != "failed":
            return None
        return " ".join(f"{key}={value}" for key, value in fields.items())
    if "diagnostic" not in fields and not (
        set(fields) == {"stage", "category", "state"} and fields["category"] == "state"
    ):
        return None
    return " ".join(f"{key}={value}" for key, value in fields.items())


def write_status(path, status):
    """Atomically publish one fixed, non-secret collector status."""
    if status not in COLLECTOR_STATUSES:
        raise ValueError("invalid collector status")
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        mode="w", encoding="ascii", dir=path.parent, prefix=f".{path.name}.", delete=False
    ) as temporary:
        temporary.write(status + "\n")
        temporary.flush()
        os.fsync(temporary.fileno())
        temporary_path = temporary.name
    os.replace(temporary_path, path)


def terminate_and_reap(child):
    """Terminate a created child and return whether wait/reaping succeeded."""
    if child is None:
        return False
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
    return child.poll() is not None


def shutdown_status(child):
    """Return the only status permitted after an interrupt."""
    if child is None:
        return "INTERRUPTED_BEFORE_START"
    try:
        return "STOPPED" if terminate_and_reap(child) else "SHUTDOWN_FAILED"
    except (OSError, subprocess.SubprocessError):
        return "SHUTDOWN_FAILED"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adb", required=True)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--status", required=True)
    args = parser.parse_args()
    output_path = Path(args.output)
    status_path = Path(args.status)
    if output_path.exists() or status_path.exists():
        sys.exit("Collector output/status path already exists; preserve it and choose a fresh run")

    def stop(_signum, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    # Exclusive creation is intentional: never replace an earlier trial's logs.
    child = None
    stopping = False
    try:
        write_status(status_path, "STARTING")
        with open(output_path, "x", encoding="utf-8") as output:
            command = [args.adb, "-s", args.serial, "logcat", "-v", "raw", "-T", "1",
                       "iDroidCapture:I", "*:S"]
            try:
                child = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                         text=True, errors="replace")
            except OSError:
                write_status(status_path, "STARTUP_FAILED")
                sys.exit("ADB log collector failed to start; no raw stderr was retained")

            startup_deadline = time.monotonic() + STARTUP_GRACE_SECONDS
            while time.monotonic() < startup_deadline:
                if child.poll() is not None:
                    write_status(status_path, "STARTUP_FAILED")
                    sys.exit("ADB log collector exited during startup; no raw stderr was retained")
                time.sleep(0.02)
            write_status(status_path, "COLLECTING_NO_EVENTS")

            try:
                for line in child.stdout:
                    safe = redact(line)
                    if safe is not None:
                        output.write(safe + "\n")
                        output.flush()
                        write_status(status_path, "COLLECTING_EVENTS")
                if child.wait() != 0:
                    write_status(status_path, "COLLECTION_FAILED")
                    sys.exit("ADB log collection failed; no raw stderr was retained")
                write_status(status_path, "COLLECTION_FAILED")
                sys.exit("ADB log collector exited unexpectedly; no raw stderr was retained")
            except KeyboardInterrupt:
                stopping = True
    except KeyboardInterrupt:
        stopping = True
    finally:
        if child is not None:
            # Only stop and reap the logcat process created here; do not clear
            # the device log buffer or persist its stderr.
            if not stopping:
                terminate_and_reap(child)
        if stopping:
            write_status(status_path, shutdown_status(child))


if __name__ == "__main__":
    main()
