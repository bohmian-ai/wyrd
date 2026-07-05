"""Protocol-client error surfaces."""

from __future__ import annotations

import pytest
from wyrd import WyrdError
from wyrd.eval import run_eval

from .mock_server import ProtocolMock, serve


def test_protocol_client_malformed_directive_raises_wyrd_error():
    mock = ProtocolMock(script=[{"kind": "not_a_real_kind"}])
    base_url, server, _ = serve(mock)
    try:
        with pytest.raises(WyrdError) as excinfo:
            run_eval(
                server_url=base_url,
                eval_ref="team/eval-card@1.0.0",
                agent_fn=lambda message, history: "irrelevant",
                access_token="test-token",
            )
    finally:
        server.shutdown()

    assert excinfo.value.code == "WYRD_SPEC_502_UPSTREAM_FAILURE"


def test_protocol_client_propagates_agent_fn_exception():
    mock = ProtocolMock(
        script=[
            {
                "kind": "agent_turn",
                "scenario_id": "scn-1",
                "turn": 0,
                "message": "hi",
                "history": [],
            },
        ]
    )
    base_url, server, _ = serve(mock)
    try:

        def agent_fn(message, history):
            raise RuntimeError("boom")

        with pytest.raises(WyrdError) as excinfo:
            run_eval(
                server_url=base_url,
                eval_ref="team/eval-card@1.0.0",
                agent_fn=agent_fn,
                access_token="test-token",
            )
    finally:
        server.shutdown()

    assert excinfo.value.code == "WYRD_AGENT_499_CALLBACK_ABORTED"
    assert "boom" in str(excinfo.value)


def test_protocol_client_requires_simulated_user_fn_in_client_mode():
    with pytest.raises(WyrdError) as excinfo:
        run_eval(
            server_url="http://127.0.0.1:1",
            eval_ref="team/eval-card@1.0.0",
            agent_fn=lambda message, history: "x",
            access_token="test-token",
            simulated_user="client",
        )

    assert excinfo.value.code == "WYRD_SPEC_400_VALIDATION"
