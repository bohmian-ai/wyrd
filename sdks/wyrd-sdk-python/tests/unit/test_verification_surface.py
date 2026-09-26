"""Offline boundary checks for the public ``wyrd.verification`` handle."""

from __future__ import annotations

import pytest
from wyrd import WyrdError
from wyrd.verification import Verification

UNREACHABLE = "http://127.0.0.1:9"


def test_malformed_arguments_are_refused_before_any_request() -> None:
    """Non-UUID IDs and off-contract requests raise validation naming the argument."""
    verification = Verification(server_url=UNREACHABLE, credential="wyrd_unused")
    cases = [
        ("binding_id", lambda: verification.get_binding("not-a-uuid")),
        ("run_id", lambda: verification.get_run("not-a-uuid")),
        ("request", lambda: verification.start_run({"target": {"kind": "binding"}})),
    ]
    for field, call in cases:
        with pytest.raises(WyrdError) as caught:
            call()
        assert caught.value.code == "WYRD_SPEC_400_VALIDATION"
        assert caught.value.details is not None
        assert caught.value.details["field"] == field
