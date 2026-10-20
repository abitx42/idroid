#!/usr/bin/env bash
set -euo pipefail

ROOT="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
BINARY="$ROOT/.build/native/android-webrtc-receiver"
BIND_ADDRESS="${IDROID_RECEIVER_TEST_BIND_ADDRESS:-}"
PORT="${IDROID_RECEIVER_TEST_PORT:-18765}"

if [[ -z "$BIND_ADDRESS" ]]; then
    printf 'ERROR: set IDROID_RECEIVER_TEST_BIND_ADDRESS to a local RFC1918 address\n' >&2
    exit 2
fi
if [[ ! -x "$BINARY" ]]; then
    printf 'ERROR: build the receiver first with bash build.sh\n' >&2
    exit 2
fi

"$ROOT/.build/native/receiver-diagnostics-test"

python3 - "$ROOT" "$BINARY" "$BIND_ADDRESS" "$PORT" <<'PY'
import json
import base64
import fcntl
import hashlib
import hmac
import os
import pathlib
import select
import secrets
import signal
import socket
import subprocess
import sys
import termios
import time

root = pathlib.Path(sys.argv[1])
binary = pathlib.Path(sys.argv[2])
bind_address = sys.argv[3]
port = int(sys.argv[4])
results = root / "results"
results.mkdir(parents=True, exist_ok=True)
environment = root / ".build/environment.txt"
pairing = "negative-test-" + secrets.token_hex(16)
stamp = time.strftime("%Y%m%d-%H%M%S") + "-" + secrets.token_hex(4)


def command_base(report, duration="30", pairing_stdin=False, accept_timeout=None):
    return [
        str(binary),
        "--bind-address", bind_address,
        *(["--pairing-code-stdin"] if pairing_stdin else ["--pairing-code", pairing]),
        "--port", str(port),
        "--duration-seconds", duration,
        *(["--accept-timeout-seconds", str(accept_timeout)] if accept_timeout is not None else []),
        "--output", str(report),
        "--environment", str(environment),
    ]


def assert_cli_failure(arguments, _expected, stdin_data=None):
    try:
        process = subprocess.run(
            [str(binary), *arguments],
            input=stdin_data,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=5,
        )
    except subprocess.TimeoutExpired:
        raise SystemExit("invalid-argument test timed out") from None
    # Native diagnostics are intentionally fixed-text; validate rejection and
    # non-disclosure rather than requiring unrestricted parser details.
    if process.returncode != 2:
        raise SystemExit("invalid-argument test failed")
    if pairing in process.stdout or pairing in process.stderr:
        raise SystemExit("pairing material appeared in invalid-argument output")
    if stdin_data and stdin_data.rstrip("\r\n") and (
        stdin_data.rstrip("\r\n") in process.stdout or stdin_data.rstrip("\r\n") in process.stderr
    ):
        raise SystemExit("stdin material appeared in invalid-argument output")


def assert_outcome(data, stage, code):
    if data.get("outcome") != {"stage": stage, "code": code}:
        raise SystemExit("receiver returned an unexpected outcome classification")


def connect_and_send(process, payload):
    chunks = payload if isinstance(payload, (list, tuple)) else [payload]
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise SystemExit("receiver exited before accepting the test connection")
        try:
            with socket.create_connection((bind_address, port), timeout=0.5) as client:
                for index, chunk in enumerate(chunks):
                    client.sendall(chunk)
                    if index + 1 < len(chunks):
                        time.sleep(0.2)
            return
        except OSError:
            time.sleep(0.1)
    raise SystemExit("receiver did not accept the test connection")


def finish(process, timeout=10):
    try:
        return process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
        raise SystemExit("receiver did not cleanly exit after the negative test")


def run_case(name, payload, expected_status, expected_error, authenticated=False, pairing_stdin=False, pairing_input=None):
    report = results / f"android-receiver-negative-{stamp}-{name}.json"
    process = subprocess.Popen(
        command_base(report, pairing_stdin=pairing_stdin),
        stdin=subprocess.PIPE if pairing_stdin else subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if pairing_stdin:
        if pairing in process.args:
            raise SystemExit("stdin pairing material appeared in process arguments")
        process.stdin.write(pairing + "\n" if pairing_input is None else pairing_input)
        process.stdin.close()
        process.stdin = None
    connect_and_send(process, payload)
    stdout, stderr = finish(process)
    if process.returncode != 0 or not report.exists():
        raise SystemExit(f"{name} case did not produce a clean report")
    data = json.loads(report.read_text())
    if data.get("status") != expected_status or data.get("statusError") != expected_error:
        raise SystemExit(f"{name} case returned an unexpected status")
    diagnostics = data.get("diagnostics", {})
    if not diagnostics.get("tcpAccepted") or diagnostics.get("offerAuthenticated") != authenticated:
        raise SystemExit(f"{name} case has incorrect non-sensitive stage diagnostics")
    if diagnostics.get("acceptTimeoutSeconds") != 10.0 or diagnostics.get("offerReadTimeoutSeconds") != 10.0:
        raise SystemExit(f"{name} case reported incorrect signaling timeout configuration")
    expected_outcomes = {
        "malformed signaling offer": ("signaling", "offer_malformed"),
        "signaling message exceeded bound": ("signaling", "offer_oversized"),
        "signaling authentication failed": ("signaling", "authentication_failed"),
        "invalid base64 signaling payload": ("signaling", "invalid_base64"),
        "SDP negotiation failed": ("sdp", "remote_offer_failed"),
        "invalid signaling session ID": ("signaling", "invalid_session"),
        "peer disconnected before signaling offer": ("tcp", "offer_disconnected"),
    }
    assert_outcome(data, *expected_outcomes[expected_error])
    report_text = report.read_text()
    if '"pairingCode"' in report_text or pairing in report_text or pairing in stdout or pairing in stderr:
        raise SystemExit(f"pairing material leaked in {name} case")
    print(f"PASS {name}: {expected_status}")


def run_stdin_tty_case(interrupt=False):
    report = results / f"android-receiver-negative-{stamp}-stdin-tty-{'interrupt' if interrupt else 'short'}.json"
    master, slave = os.openpty()
    original = termios.tcgetattr(slave)

    def acquire_terminal():
        os.setsid()
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    process = subprocess.Popen(
        command_base(report, pairing_stdin=True), stdin=slave,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        preexec_fn=acquire_terminal,
    )
    try:
        deadline = time.monotonic() + 5
        while termios.tcgetattr(master)[3] & termios.ECHO:
            if process.poll() is not None or time.monotonic() >= deadline:
                raise SystemExit("stdin reader did not disable terminal echo")
            time.sleep(0.01)
        # Exit during option validation so the isolated controlling terminal
        # need not launch AppKit/WindowServer to prove no-echo input handling.
        tty_input = "short"
        if interrupt:
            process.send_signal(signal.SIGINT)
        else:
            os.write(master, (tty_input + "\n").encode())
        echoed = bytearray()
        deadline = time.monotonic() + 5
        # macOS waits for PTY output to drain while restoring termios. Drain
        # its final newline concurrently, as a real terminal would.
        while process.poll() is None and time.monotonic() < deadline:
            if select.select([master], [], [], 0.05)[0]:
                chunk = os.read(master, 4096)
                if not chunk:
                    break
                echoed.extend(chunk)
        stdout, stderr = finish(process, timeout=1)
        while select.select([master], [], [], 0)[0]:
            chunk = os.read(master, 4096)
            if not chunk:
                break
            echoed.extend(chunk)
        restored = termios.tcgetattr(master)
        if (restored[3] & termios.ECHO) != (original[3] & termios.ECHO):
            raise SystemExit("stdin reader did not restore terminal echo")
        if tty_input.encode() in echoed or tty_input in stdout or tty_input in stderr or pairing in process.args:
            raise SystemExit("pairing material leaked in terminal stdin case")
        if interrupt:
            if process.returncode != -signal.SIGINT or report.exists():
                raise SystemExit("terminal stdin interrupt did not stop before startup")
            print("PASS stdin-terminal-interrupt-restores-echo")
        else:
            if process.returncode != 2 or report.exists():
                raise SystemExit("terminal stdin case did not reject invalid input before startup")
            print("PASS stdin-terminal-no-echo-and-restore")
    finally:
        os.close(master)
        os.close(slave)
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


def offer_payload(session_id, encoded_sdp):
    value = f"OFFER|{session_id}|{encoded_sdp}".encode("utf-8")
    auth = hmac.new(pairing.encode("utf-8"), value, hashlib.sha256).hexdigest()
    return f"OFFER\t{session_id}\t{encoded_sdp}\t{auth}\n".encode("utf-8")


def run_second_client_case():
    report = results / f"android-receiver-negative-{stamp}-second-client.json"
    process = subprocess.Popen(
        command_base(report), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    first = None
    try:
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise SystemExit("receiver exited before the first client connected")
            try:
                first = socket.create_connection((bind_address, port), timeout=0.5)
                break
            except OSError:
                time.sleep(0.1)
        if first is None:
            raise SystemExit("receiver did not accept the first client")
        try:
            second = socket.create_connection((bind_address, port), timeout=1.0)
        except OSError:
            second = None
        if second is not None:
            with second:
                second.sendall(b"NOT_AN_OFFER\n")
                second.settimeout(0.5)
                try:
                    if second.recv(1):
                        raise SystemExit("second client received a signaling response")
                except (socket.timeout, ConnectionResetError):
                    pass
    finally:
        if first is not None:
            first.close()
    stdout, stderr = finish(process)
    if process.returncode != 0 or not report.exists():
        raise SystemExit("second-client case did not cleanly terminate")
    data = json.loads(report.read_text())
    if data.get("status") != "FAIL" or data.get("statusError") != "peer disconnected before signaling offer":
        raise SystemExit("second-client case returned an unexpected status")
    if '"pairingCode"' in report.read_text() or pairing in stdout or pairing in stderr:
        raise SystemExit("pairing material leaked in second-client case")
    assert_outcome(data, "tcp", "offer_disconnected")
    print("PASS second-client-rejected")


def run_idle_timeout_case():
    report = results / f"android-receiver-negative-{stamp}-idle-timeout.json"
    process = subprocess.Popen(
        command_base(report, duration="0.1"), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
    )
    client = None
    try:
        deadline = time.monotonic() + 8
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise SystemExit("receiver exited before the idle client connected")
            try:
                client = socket.create_connection((bind_address, port), timeout=0.5)
                break
            except OSError:
                time.sleep(0.1)
        if client is None:
            raise SystemExit("receiver did not accept the idle client")
        time.sleep(11)
    finally:
        if client is not None:
            client.close()
    stdout, stderr = finish(process, timeout=5)
    if process.returncode != 0 or not report.exists():
        raise SystemExit("idle-timeout case did not cleanly terminate")
    data = json.loads(report.read_text())
    if data.get("status") != "TIMEOUT" or data.get("statusError") != "signaling offer read timed out":
        raise SystemExit("idle-timeout case returned an unexpected status")
    assert_outcome(data, "signaling", "offer_timeout")
    diagnostics = data.get("diagnostics", {})
    if diagnostics.get("acceptTimeoutSeconds") != 10.0 or diagnostics.get("offerReadTimeoutSeconds") != 10.0:
        raise SystemExit("idle-timeout case reported incorrect signaling timeout configuration")
    if '"pairingCode"' in report.read_text() or pairing in stdout or pairing in stderr:
        raise SystemExit("pairing material leaked in idle-timeout case")
    print("PASS idle-client-timeout: TIMEOUT")


def run_configured_accept_timeout_case():
    report = results / f"android-receiver-negative-{stamp}-accept-timeout-45.json"
    process = subprocess.Popen(
        command_base(report, duration="30", accept_timeout="45"),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    try:
        # The default is 10 seconds. Waiting past that boundary before the
        # malformed local probe proves the configured 45-second accept window.
        time.sleep(11)
        if process.poll() is not None:
            raise SystemExit("configured accept-timeout case expired at the default deadline")
        connect_and_send(process, b"NOT_AN_OFFER\n")
    finally:
        if process.poll() is None:
            stdout, stderr = finish(process)
        else:
            stdout, stderr = process.communicate()
    if process.returncode != 0 or not report.exists():
        raise SystemExit("configured accept-timeout case did not produce a clean report")
    data = json.loads(report.read_text())
    if data.get("status") != "FAIL" or data.get("statusError") != "malformed signaling offer":
        raise SystemExit("configured accept-timeout case returned an unexpected status")
    diagnostics = data.get("diagnostics", {})
    if diagnostics.get("acceptTimeoutSeconds") != 45.0 or diagnostics.get("offerReadTimeoutSeconds") != 10.0:
        raise SystemExit("configured accept-timeout case reported incorrect timeout values")
    report_text = report.read_text()
    if '"pairingCode"' in report_text or pairing in report_text or pairing in stdout or pairing in stderr:
        raise SystemExit("pairing material leaked in configured accept-timeout case")
    assert_outcome(data, "signaling", "offer_malformed")
    print("PASS configured-accept-timeout: 45 seconds (offer read remains 10 seconds)")


help_process = subprocess.run(
    [str(binary), "--help"], stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    text=True, check=False,
)
if (help_process.returncode != 0 or "--bind-address PRIVATE_IPV4" not in help_process.stdout or
        "--pairing-code-stdin" not in help_process.stdout or "--accept-timeout-seconds N" not in help_process.stdout):
    raise SystemExit("help output does not document the required bind address")
print("PASS help")

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
    probe.settimeout(0.2)
    if probe.connect_ex((bind_address, port)) == 0:
        raise SystemExit(f"refusing to disturb an existing listener on {bind_address}:{port}")
print(f"PASS port-free-check: {port}")

assert_cli_failure(
    ["--bind-address", bind_address, "--pairing-code", "short", "--port", str(port)],
    "--pairing-code",
)
print("PASS short-pairing-code-rejected")
assert_cli_failure(
    ["--pairing-code", pairing, "--port", str(port)],
    "--bind-address",
)
print("PASS missing-bind-address-rejected")
assert_cli_failure(
    ["--bind-address", "0.0.0.0", "--pairing-code", pairing, "--port", str(port)],
    "RFC1918",
)
print("PASS wildcard-bind-address-rejected")
assert_cli_failure(
    ["--bind-address", "127.0.0.1", "--pairing-code", pairing, "--port", str(port)],
    "RFC1918",
)
print("PASS loopback-bind-address-rejected")
for value in ["", "0", "-1", "3600.1", "nan", "inf", "45junk"]:
    assert_cli_failure(
        ["--bind-address", bind_address, "--pairing-code", pairing, "--port", str(port),
         "--accept-timeout-seconds", value],
        "--accept-timeout-seconds",
    )
    print(f"PASS invalid-accept-timeout-{value or 'empty'}-rejected")
assert_cli_failure(
    ["--bind-address", bind_address, "--pairing-code", pairing, "--port", str(port),
     "--accept-timeout-seconds"],
    "missing value for --accept-timeout-seconds",
)
print("PASS missing-accept-timeout-rejected")

stdin_arguments = ["--bind-address", bind_address, "--pairing-code-stdin", "--port", str(port)]
for name, value in [
    ("empty-eof", ""), ("empty-line", "\n"), ("short", "short\n"),
    ("oversized", pairing + "x" * (257 - len(pairing)) + "\n"),
    ("oversized-long", pairing + "x" * 8192 + "\n"),
    ("embedded-nul", pairing + "\0suffix\n"),
]:
    assert_cli_failure(stdin_arguments, "--pairing-code-stdin", stdin_data=value)
    print(f"PASS stdin-{name}-rejected")
assert_cli_failure(
    [*stdin_arguments, "--pairing-code", pairing], "choose only one", stdin_data="",
)
print("PASS conflicting-pairing-options-rejected")
assert_cli_failure(
    [*stdin_arguments, "--pairing-code-stdin"], "only be specified once", stdin_data="",
)
print("PASS duplicate-stdin-option-rejected")

run_case("malformed", b"NOT_AN_OFFER\n", "FAIL", "malformed signaling offer")
run_case("partial-read", [b"NOT_", b"AN_OFFER\n"], "FAIL", "malformed signaling offer")
run_case("at-bound", b"x" * 262143 + b"\n", "FAIL", "malformed signaling offer")
run_case("oversized", b"x" * 262145 + b"\n", "FAIL", "signaling message exceeded bound")
run_case("bad-auth", b"OFFER\tsession\tZQ\tbad\n", "FAIL", "signaling authentication failed")
run_case("bad-sdp", offer_payload("session-sdp", "!"), "FAIL", "invalid base64 signaling payload", authenticated=True)
run_case("sdp-negotiation", offer_payload("session-sdp", "bm90IGFuIFNEUA"), "FAIL", "SDP negotiation failed", authenticated=True)
for name, sdp in [
    ("sdp-exception-redacted", f"v=0\r\na=fingerprint:sha-256 {pairing}\r\n"),
    ("sdp-warning-redacted", f"v=0\r\na=fingerprint:{pairing} AA\r\n"),
]:
    encoded = base64.urlsafe_b64encode(sdp.encode()).decode().rstrip("=")
    run_case(name, offer_payload("session-sdp", encoded), "FAIL", "SDP negotiation failed", authenticated=True)
run_case("invalid-session", offer_payload("", "ZQ"), "FAIL", "invalid signaling session ID", authenticated=True)
run_case("peer-disconnect", b"", "FAIL", "peer disconnected before signaling offer")
run_case("stdin-pipe", b"NOT_AN_OFFER\n", "FAIL", "malformed signaling offer", pairing_stdin=True)
run_case("stdin-max-length", b"NOT_AN_OFFER\n", "FAIL", "malformed signaling offer", pairing_stdin=True,
         pairing_input=pairing + "x" * (256 - len(pairing)) + "\n")
run_case("stdin-eof", b"NOT_AN_OFFER\n", "FAIL", "malformed signaling offer", pairing_stdin=True, pairing_input=pairing)
run_stdin_tty_case()
run_stdin_tty_case(interrupt=True)
run_second_client_case()
run_idle_timeout_case()
run_configured_accept_timeout_case()

timeout_report = results / f"android-receiver-negative-{stamp}-timeout.json"
process = subprocess.Popen(
    command_base(timeout_report, duration="0.1"),
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
)
stdout, stderr = finish(process, timeout=22)
if process.returncode != 0 or not timeout_report.exists():
    raise SystemExit("timeout case did not cleanly terminate")
timeout_data = json.loads(timeout_report.read_text())
if timeout_data.get("status") != "TIMEOUT":
    raise SystemExit("timeout case did not report TIMEOUT")
assert_outcome(timeout_data, "tcp", "accept_timeout")
if timeout_data.get("diagnostics", {}).get("tcpAccepted"):
    raise SystemExit("no-client timeout incorrectly reports a TCP connection")
if timeout_data.get("connection", {}).get("peerStateAtReport") != "new":
    raise SystemExit("timeout report lost the observed state during cleanup")
if timeout_data.get("diagnostics", {}).get("acceptTimeoutSeconds") != 10.0 or timeout_data.get("diagnostics", {}).get("offerReadTimeoutSeconds") != 10.0:
    raise SystemExit("timeout report did not preserve the default signaling timeout values")
if '"pairingCode"' in timeout_report.read_text() or pairing in timeout_report.read_text() or pairing in stdout or pairing in stderr:
    raise SystemExit("pairing material leaked in timeout case")
print("PASS timeout-cleanup: TIMEOUT")
PY
