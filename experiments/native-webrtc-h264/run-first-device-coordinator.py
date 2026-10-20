#!/usr/bin/env python3
"""Coordinate one first-device receiver trial without exposing pairing input.

The script resolves every repository path from its own location, not from the
caller's working directory. It does not generate or print a pairing code: the
operator enters the same code already entered in the Android app at a hidden
terminal prompt.
"""

from __future__ import annotations

import argparse
import getpass
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


SCRIPT_DIR = Path(__file__).resolve().parent
REPOSITORY_ROOT = SCRIPT_DIR.parents[1]
NATIVE_ROOT = REPOSITORY_ROOT / "experiments" / "native-webrtc-h264"
RECEIVER = NATIVE_ROOT / ".build" / "native" / "android-webrtc-receiver"
RECORD_ENVIRONMENT = NATIVE_ROOT / "record-environment.sh"
LIBDATACHANNEL_SOURCE = NATIVE_ROOT / ".build" / "deps" / "libdatachannel"
CMAKE = NATIVE_ROOT / ".build" / "tools" / "cmake-3.31.10" / "cmake" / "data" / "bin" / "cmake"

BIND_ADDRESS = "10.120.27.133"
PORT = 18765


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--run-dir",
        type=Path,
        default=None,
        help="fresh, nonexistent evidence directory; default creates one under results/",
    )
    parser.add_argument("--duration-seconds", type=float, default=30.0)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="validate paths and harmless in-memory input without writing or starting the receiver",
    )
    return parser.parse_args()


def listener_for(pid: int) -> str:
    result = subprocess.run(
        [
            "lsof",
            "-nP",
            "-a",
            "-p",
            str(pid),
            "-iTCP:" + str(PORT),
            "-sTCP:LISTEN",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    return result.stdout


def stop_child(process: subprocess.Popen[bytes] | None) -> None:
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def validate_pairing_code(code: str) -> bytes:
    """Accept only the printable ASCII form used by the test runbook.

    Do not trim or normalize user input: changing a code would change its HMAC
    secret. Reject whitespace and Unicode explicitly so visually similar input
    cannot produce a surprising authentication failure.
    """
    encoded = code.encode("utf-8")
    if not 8 <= len(encoded) <= 256:
        raise ValueError("Pairing code rejected: length must be 8–256 bytes")
    if any(ord(character) < 0x21 or ord(character) > 0x7E for character in code):
        raise ValueError("Pairing code rejected: use printable ASCII without whitespace")
    return encoded


def main() -> int:
    args = parse_args()
    if args.run_dir is None:
        run_dir = NATIVE_ROOT / "results" / "first-device-dry-run"
    else:
        run_dir = (REPOSITORY_ROOT / args.run_dir).resolve() if not args.run_dir.is_absolute() else args.run_dir.resolve()
    environment_path = run_dir / "environment.txt"
    report_path = run_dir / "trial-1.json"
    process: subprocess.Popen[bytes] | None = None
    fifo_path: Path | None = None
    fifo = None

    if not RECEIVER.is_file() or not os.access(RECEIVER, os.X_OK):
        raise SystemExit(f"Receiver binary is missing or not executable: {RECEIVER}")
    if not RECORD_ENVIRONMENT.is_file() or not LIBDATACHANNEL_SOURCE.is_dir() or not CMAKE.is_file():
        raise SystemExit("Pinned native build dependencies are incomplete; no receiver was started")
    if args.duration_seconds <= 0 or args.duration_seconds > 3600:
        raise SystemExit("duration must be in (0, 3600]")

    if args.dry_run:
        try:
            validate_pairing_code("iDroid-dry-run-code")
        except ValueError as error:
            raise SystemExit(str(error)) from None
        print("dry_run=pass")
        print(f"repository_root={REPOSITORY_ROOT}")
        print(f"receiver={RECEIVER}")
        print(f"run_dir={run_dir}")
        print(f"report={report_path}")
        print("receiver_started=no")
        return 0

    if args.run_dir is None:
        results_root = NATIVE_ROOT / "results"
        results_root.mkdir(parents=True, exist_ok=True)
        run_dir = Path(tempfile.mkdtemp(prefix="first-device-", dir=results_root))
        environment_path = run_dir / "environment.txt"
        report_path = run_dir / "trial-1.json"
    else:
        if run_dir.exists():
            raise SystemExit(f"Refusing to reuse existing run directory: {run_dir}")
        run_dir.mkdir(parents=True)
    for path in (environment_path, report_path):
        if path.exists():
            raise SystemExit(f"Refusing to overwrite existing path: {path}")

    try:
        subprocess.run(
            [
                "bash",
                str(RECORD_ENVIRONMENT),
                str(environment_path),
                str(LIBDATACHANNEL_SOURCE),
                str(CMAKE),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        fifo_dir = Path(tempfile.mkdtemp(prefix="idroid-pairing-"))
        fifo_path = fifo_dir / "pairing-input"
        os.mkfifo(fifo_path, 0o600)

        # O_RDWR prevents the receiver's stdin setup from blocking before the
        # hidden operator prompt is visible. This descriptor stays open until
        # after the listener proves that the receiver consumed the one line.
        fifo = os.fdopen(os.open(fifo_path, os.O_RDWR), "r+b", buffering=0)
        process = subprocess.Popen(
            [
                str(RECEIVER),
                "--bind-address",
                BIND_ADDRESS,
                "--port",
                str(PORT),
                "--pairing-code-stdin",
                "--duration-seconds",
                str(args.duration_seconds),
                "--environment",
                str(environment_path),
                "--output",
                str(report_path),
            ],
            stdin=fifo,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            close_fds=True,
        )

        code = getpass.getpass("Enter the same printable-ASCII pairing code already entered on the phone: ")
        try:
            encoded = validate_pairing_code(code)
        except ValueError as error:
            raise SystemExit(str(error)) from None
        if process.poll() is not None:
            raise SystemExit("Receiver exited before pairing input was submitted")

        # Exactly one newline-terminated write. The code is not in argv,
        # stdout, stderr, or the structured report.
        fifo.write(encoded + b"\n")
        fifo.flush()
        del code, encoded

        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise SystemExit(f"Receiver exited before listener verification: {process.returncode}")
            listener = listener_for(process.pid)
            if f"{BIND_ADDRESS}:{PORT}" in listener:
                print("READY-TO-TAP")
                print(f"receiver={RECEIVER}")
                print(f"report={report_path}")
                break
            time.sleep(0.1)
        else:
            raise SystemExit("Receiver listener was not verified within 10 seconds")

        # Keep the coordinator alive while the operator initiates Android
        # capture and the receiver writes its report.
        process.wait()
        print(f"receiver_exit_code={process.returncode}")
        print(f"report_exists={'yes' if report_path.exists() else 'no'}")
        return 0
    finally:
        stop_child(process)
        if fifo is not None:
            fifo.close()
        if fifo_path is not None:
            fifo_path.unlink(missing_ok=True)
            shutil.rmtree(fifo_path.parent, ignore_errors=True)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit("Coordinator interrupted; receiver child was stopped if it was started")
