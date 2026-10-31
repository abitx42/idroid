#!/usr/bin/env python3
"""Host-only regression tests. No Android device or raw logs required."""
import importlib.util
from pathlib import Path
import secrets
import unittest

spec = importlib.util.spec_from_file_location("capture_diagnostics", Path(__file__).with_name("capture-diagnostics.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class RedactionTests(unittest.TestCase):
    def test_known_diagnostics_and_states(self):
        for line in (
            "diagnostic=USER_STOP_REQUESTED stage=session category=user_stop",
            "diagnostic=TCP_CONNECTING stage=tcp_connect category=started timeout_ms=5000",
            "diagnostic=ICE_GATHERING_TIMEOUT stage=ice_gather category=timeout timeout_ms=5000",
            "stage=ice_connection category=state state=CONNECTED",
            "diagnostic=CAPTURE_CONSENT_GRANTED",
            "timestamp_ms=1735689600123 diagnostic=SCREEN_FRAME_CAPTURED stage=screen_capture status=ok",
            "timestamp_ms=1735689600123 diagnostic=SCREEN_FRAME_TIMEOUT stage=screen_capture status=failed failure=SCREEN_FRAME_TIMEOUT",
            "timestamp_ms=1735689600123 diagnostic=START_BUTTON_PRESSED stage=session status=ok",
            "timestamp_ms=1735689600123 diagnostic=SIGNALING_ERROR stage=tcp status=failed failure=TCP_CONNECT_TIMEOUT",
        ):
            self.assertEqual(module.redact(line), line)

    def test_raw_payload_and_unapproved_fields_never_persist(self):
        secret = secrets.token_hex(24)
        for line in (
            secret, "OFFER\tsession\tpayload\tauth", "v=0",
            f"diagnostic=USER_STOP_REQUESTED pairing={secret}",
            f"diagnostic={secret}", f"stage={secret} category=state state=NEW",
            "diagnostic=USER_STOP_REQUESTED diagnostic=AUTHENTICATION_FAILED",
            "diagnostic=USER_STOP_REQUESTED\x00", "x" * 300,
            "timestamp_ms=1 diagnostic=SIGNALING_ERROR stage=tcp status=failed failure=secret",
            "timestamp_ms=1 diagnostic=SIGNALING_ERROR stage=tcp status=ok failure=TCP_CONNECT_TIMEOUT",
            "timestamp_ms=1 diagnostic=SIGNALING_ERROR stage=tcp status=failed sdp=payload",
            "timestamp_ms=12345678901234 diagnostic=START_BUTTON_PRESSED stage=session status=ok",
            "timestamp_ms=" + "1" * 300 + " diagnostic=START_BUTTON_PRESSED stage=session status=ok",
        ):
            # Do not let unittest render a rejected input on failure.
            self.assertTrue(module.redact(line) is None, "unapproved content accepted")


if __name__ == "__main__":
    unittest.main()
