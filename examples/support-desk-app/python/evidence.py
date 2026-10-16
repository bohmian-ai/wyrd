"""Print everything Wyrd recorded for one Run, joined in one Bifrost query:
the ticket, the request span, each gateway call, and each verdict.

    uv run python evidence.py <run_id>
"""

from __future__ import annotations

import sys
from typing import Any

from wyrd.bifrost import Bifrost
from wyrd.client import WyrdClient

# ANCHOR: sql
SQL = """
SELECT t.run_id, t.question, t.answer, s.trace_id, g.call_id, r.execution_id, r.verdict
FROM vala.datasets.tickets t
JOIN vala.traces.spans s ON s.run_id = t.run_id AND s.name = 'support-desk.ask'
JOIN vala.gateway.calls g ON g.run_id = t.run_id
JOIN vala.verification.results r ON r.run_id = t.run_id
WHERE t.run_id = $1
"""
# ANCHOR_END: sql


def evidence(run_id: str, client: WyrdClient | None = None) -> list[dict[str, Any]]:
    return Bifrost(client=client).sql(SQL, [run_id]).to_arrow().to_pylist()


if __name__ == "__main__":
    for row in evidence(sys.argv[1]):
        print(row)
