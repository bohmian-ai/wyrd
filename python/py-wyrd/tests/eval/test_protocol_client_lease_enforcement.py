"""Lease-carrier protocol test."""

from __future__ import annotations

from wyrd.eval import run_eval

from .mock_server import TEST_LEASE, TEST_RUN_ID, ProtocolMock, serve


def test_protocol_client_carries_lease_through_full_walk():
    mock = ProtocolMock(
        script=[
            {
                "kind": "agent_turn",
                "scenario_id": "scn-1",
                "turn": 0,
                "message": "hello",
                "history": [],
            },
            {"kind": "scenario_complete", "scenario_id": "scn-1"},
            {"kind": "run_complete"},
        ]
    )
    mock.enforce_lease = True
    base_url, server, _ = serve(mock)
    try:

        def agent_fn(message, history):
            return {"response": "world", "records": []}

        summary = run_eval(
            server_url=base_url,
            eval_ref="team/eval-card@1.0.0",
            agent_fn=agent_fn,
            access_token="test-token",
        )
    finally:
        server.shutdown()

    assert summary["run_id"] == TEST_RUN_ID
    expected = f"Bearer {TEST_LEASE}"
    for path, auth in mock.received_auth_headers:
        assert auth == expected, f"{path} did not carry the lease: {auth!r}"
