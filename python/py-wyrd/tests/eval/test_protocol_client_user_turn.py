"""Client-delegated simulated-user turn round trip."""

from __future__ import annotations

from wyrd.eval import run_eval

from .mock_server import ProtocolMock, serve


def test_protocol_client_delegated_user_turn_round_trips():
    mock = ProtocolMock(
        script=[
            {
                "kind": "user_turn_needed",
                "scenario_id": "scn-1",
                "turn": 1,
                "history": [{"role": "agent", "content": "hi"}],
            },
            {
                "kind": "agent_turn",
                "scenario_id": "scn-1",
                "turn": 1,
                "message": "follow up",
                "history": [
                    {"role": "agent", "content": "hi"},
                    {"role": "user", "content": "follow up"},
                ],
            },
            {"kind": "scenario_complete", "scenario_id": "scn-1"},
            {"kind": "run_complete"},
        ]
    )
    base_url, server, _ = serve(mock)
    try:

        def agent_fn(message, history):
            return "ack"

        def simulated_user_fn(history):
            assert history[-1]["role"] == "agent"
            return "follow up"

        run_eval(
            server_url=base_url,
            eval_ref="team/eval-card@1.0.0",
            agent_fn=agent_fn,
            simulated_user_fn=simulated_user_fn,
            simulated_user="client",
        )
    finally:
        server.shutdown()

    assert mock.received_user_turns == [
        {"scenario_id": "scn-1", "turn": 1, "message": "follow up"}
    ]
    assert mock.received_agent_turns == [
        {"scenario_id": "scn-1", "turn": 1, "response": "ack"}
    ]
