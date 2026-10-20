#!/usr/bin/env python3
"""Check collector status and liveness without touching ADB or the network."""

import argparse
import os
from pathlib import Path
import sys
import time


READY_STATUSES = {"COLLECTING_NO_EVENTS", "COLLECTING_EVENTS"}
WAITING_STATUS = "STARTING"
FAILURE_STATUSES = {
    "STARTUP_FAILED",
    "COLLECTION_FAILED",
    "INTERRUPTED_BEFORE_START",
    "SHUTDOWN_FAILED",
    "STOPPED",
}


def read_status(path):
    try:
        value = path.read_text(encoding="ascii")
    except (OSError, UnicodeError):
        return None
    if value.endswith("\n"):
        value = value[:-1]
    if "\n" in value or value not in READY_STATUSES | {WAITING_STATUS} | FAILURE_STATUSES:
        return None
    return value


def is_alive(pid):
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except (ProcessLookupError, PermissionError, ValueError, OverflowError):
        return False
    return True


def check_readiness(path, pid):
    """Return 0 ready, 1 failed/stale, or 2 still starting."""
    first = read_status(path)
    if first == WAITING_STATUS and is_alive(pid):
        return 2
    if first not in READY_STATUSES or not is_alive(pid):
        return 1

    # Recheck both values together to reduce the status/liveness race before
    # the caller authorizes the next physical-device step.
    time.sleep(0.02)
    second = read_status(path)
    if second != first or second not in READY_STATUSES or not is_alive(pid):
        return 1
    print("collector_ready")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("status", type=Path)
    parser.add_argument("pid", type=int)
    args = parser.parse_args()
    return check_readiness(args.status, args.pid)


if __name__ == "__main__":
    raise SystemExit(main())
