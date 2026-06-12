"""Public eval package import surface."""

from __future__ import annotations

from wyrd.eval import AgentTurnResponse, ConversationTurn, RunSummary, run_eval


def test_eval_typing_exports_are_importable_and_usable():
    turn: ConversationTurn = {"role": "User", "content": "hello"}
    response: AgentTurnResponse = {"response": "hi", "records": []}
    summary: RunSummary = {"run_id": "run-1", "server_url": "http://127.0.0.1"}

    assert turn["role"] == "User"
    assert response["response"] == "hi"
    assert summary["run_id"] == "run-1"
    assert callable(run_eval)
