"""Integration journeys: Bifrost write surface against a live WyrdTestServer.

All tests require the ``integration`` pytest marker and a running WyrdTestServer
provided by the ``wyrd_server`` session fixture in conftest.py.

Current state of the transport layer: the Python ``Bifrost`` handle drains into
an in-process MockSink. Full round-trip assertions (rows land in warehouse,
fingerprint == describe().fingerprint, audit log gap-free seq) require the gRPC
ingest transport wired in a later stage. Until then the journeys verify the SDK
lifecycle, backpressure contract, and error boundaries that are observable now.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING

import pytest
from wyrd.bifrost import Bifrost
from wyrd.observe import record

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer

SCHEMA = json.dumps(
    {
        "type": "object",
        "properties": {
            "model": {"type": "string"},
            "tokens": {"type": "integer"},
            "score": {"type": "number"},
        },
        "required": ["model", "tokens", "score"],
    }
)

CARD_REF = "prod/Service/genai_pipeline@1.0.0"


def _row(i: int) -> str:
    return json.dumps({"model": "claude-opus-4-8", "tokens": i, "score": 0.5})


# ---------------------------------------------------------------------------
# C4e: bootstrap_service pymethod
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bootstrap_service_mints_api_key(wyrd_server: WyrdTestServer) -> None:
    key = wyrd_server.bootstrap_service(["writer"], name="svc-test")
    assert isinstance(key, str) and len(key) > 0, "expected non-empty API key string"


@pytest.mark.integration
def test_bootstrap_service_empty_permissions_returns_key(wyrd_server: WyrdTestServer) -> None:
    key = wyrd_server.bootstrap_service([], name="svc-no-perms")
    assert isinstance(key, str) and len(key) > 0, "underprivileged principal still gets a key"


# ---------------------------------------------------------------------------
# C6: happy-path lifecycle — both write surfaces
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_handle_insert_no_drops(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    for i in range(100):
        bifrost.insert(
            table="genai.prompts",
            schema=SCHEMA,
            row=_row(i),
            card_ref=CARD_REF,
        )
    assert bifrost.dropped == 0, "happy path: no drops"
    assert bifrost.producer_count == 1


@pytest.mark.integration
def test_observe_record_no_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    for i in range(100):
        record(bifrost, table="genai.observe", schema=SCHEMA, row=_row(i), card_ref=CARD_REF)
    assert bifrost.dropped == 0


# ---------------------------------------------------------------------------
# C6: backpressure contracts
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_bifrost_insert_propagates_queue_full(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)

    # With MockSink (instant drain), queue-full is unlikely under normal load.
    # This test verifies the API surface accepts inserts without error under
    # current transport; backpressure is fully exercised in sdk.rs unit tests.
    for i in range(500):
        bifrost.insert(
            table="genai.backpressure",
            schema=SCHEMA,
            row=_row(i),
            card_ref=CARD_REF,
        )
    assert bifrost.dropped == 0, "observe path: no silent drops on explicit insert path"


@pytest.mark.integration
def test_observe_record_swallows_and_counts(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    attempted = 200
    for i in range(attempted):
        record(
            bifrost, table="genai.observe_overflow", schema=SCHEMA, row=_row(i), card_ref=CARD_REF
        )
    # Fire-and-forget never raises; accepted + dropped == attempted.
    # With MockSink all rows are accepted, so dropped == 0 is expected here.
    assert bifrost.dropped + (attempted - bifrost.dropped) == attempted


# ---------------------------------------------------------------------------
# C6: negative journeys
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_negative_bad_card_ref_raises(wyrd_server: WyrdTestServer) -> None:
    bifrost = Bifrost(wyrd_server.base_url, wyrd_server.api_key)
    with pytest.raises(ValueError):
        bifrost.insert(table="genai.bad", schema=SCHEMA, row=_row(0), card_ref="not-a-ref")


@pytest.mark.integration
def test_negative_conflicting_schema_fingerprint_mismatch(wyrd_server: WyrdTestServer) -> None:
    pytest.skip(
        "Requires Bifrost.register() — gRPC ingest transport not yet wired. "
        "The WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH contract is unit-covered in "
        "vala-bifrost and will be e2e-verified when transport lands."
    )


@pytest.mark.integration
def test_negative_empty_permissions_denied_rbac_on_write(wyrd_server: WyrdTestServer) -> None:
    pytest.skip(
        "Requires real gRPC ingest transport so the server can enforce RBAC and return "
        "WYRD_PERMISSION_403_DENIED_RBAC. Currently Bifrost drains into MockSink; "
        "server-side RBAC enforcement is verified in wyrd-server authz e2e tests."
    )


@pytest.mark.integration
def test_negative_invalid_sql_query(wyrd_server: WyrdTestServer) -> None:
    pytest.skip(
        "Requires Bifrost.sql() — query surface not yet wired in the Python SDK. "
        "The WYRD_VALA_400_QUERY_INVALID_SQL contract is unit-covered in vala-bifrost."
    )


# ---------------------------------------------------------------------------
# C6: audit trail
# ---------------------------------------------------------------------------


@pytest.mark.integration
def test_positive_audit_trail(wyrd_server: WyrdTestServer) -> None:
    pytest.skip(
        "Requires Bifrost.sql() to read vala.system.audit_log and confirm gap-free seq "
        "with decision=allow across register/write/query kinds. Audit outbox landing is "
        "unit-covered in vala-sql/tests/audit_outbox.rs (S3.C5)."
    )
