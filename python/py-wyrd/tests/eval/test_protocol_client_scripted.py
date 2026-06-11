"""Scripted agent-turn protocol walk."""

from __future__ import annotations

from wyrd.eval import run_eval

from .mock_server import TEST_LEASE, TEST_RUN_ID, ProtocolMock, serve


def test_protocol_client_scripted_run_completes_and_records_submissions():
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
    base_url, server, _ = serve(mock)
    try:
        agent_calls: list[tuple[str, list[dict[str, str]]]] = []

        def agent_fn(message, history):
            agent_calls.append((message, list(history)))
            return {"response": "world", "records": []}

        summary = run_eval(
            server_url=base_url,
            eval_ref="team/eval-card@1.0.0",
            agent_fn=agent_fn,
        )
    finally:
        server.shutdown()

    assert summary["run_id"] == TEST_RUN_ID
    assert agent_calls == [("hello", [])]
    assert mock.received_agent_turns == [
        {"scenario_id": "scn-1", "turn": 0, "response": "world"}
    ]
    assert mock.received_open == {
        "eval_ref": {
            "kind": "Eval",
            "name": "eval-card",
            "version": "1.0.0",
            "space": "team",
        },
        "simulated_user": "server",
    }
    expected = f"Bearer {TEST_LEASE}"
    assert mock.received_auth_headers
    for path, auth in mock.received_auth_headers:
        assert auth == expected, f"{path} missing/wrong Authorization: {auth!r}"
