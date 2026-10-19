"""Judge one live agent's application from what its own Wyrd server holds.

Only authorized public reads decide: the declared Service graph pulled as
metadata, Card- and Run-correlated Bifrost rows, and each queued execution's
server-owned status. An agent's prose, files, or exit code never count.

Each agent gets its own deployment and tenant, so every row this observer
reads was written for that agent's application; another agent's evidence is
unreachable through its client. A Run is evidenced by correlated rows, not a
Run registry.

    python -m tests.agent_readiness.observe [--timeout SECONDS]

prints one JSON evidence document and exits 0 only when every milestone holds.
The client resolves from the saved local settings (``WYRD_CONFIG_HOME``).
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import tempfile
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any

import yaml
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.client import WyrdClient
from wyrd.errors import WyrdError
from wyrd.verify import Execution

MILESTONES = ("declared", "telemetry", "custom_rows", "sync_judgment", "async_judgment")
"""Every milestone in workflow order; the first one missing is the failure point."""

JUDGED = frozenset({"passed", "failed"})
"""Verdicts that are real judgments; ``inconclusive`` and a missing verdict are not."""

TABLE_NAME = re.compile(r"^[a-z_][a-z0-9_]*$")
"""A ``vala.datasets`` table segment safe to name in SQL."""

ROW_LIMIT = 200
"""Rows each evidence query projects; a working application needs only one."""


@dataclass
class Judgment:
    """One Verifier execution judging the application, as the server reports it."""

    execution_id: str
    status: str
    verdict: str | None
    run_id: str | None

    def judged(self) -> bool:
        """A completed execution with a real verdict on an application Run."""
        return self.status == "completed" and self.verdict in JUDGED and bool(self.run_id)


@dataclass
class Evidence:
    """What one Service's declared graph has on the server."""

    service: str | None = None
    kinds: list[str] = field(default_factory=list)
    telemetry_runs: list[str] = field(default_factory=list)
    custom_runs: dict[str, list[str]] = field(default_factory=dict)
    sync: list[Judgment] = field(default_factory=list)
    queued: list[Judgment] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def failure_point(self) -> str | None:
        """The first missing milestone, or ``None`` when the whole workflow is evidenced.

        Declared needs a Service with at least one component and one bound
        Verifier in its pulled graph. Telemetry and custom rows need a row
        carrying a Run and a graph Card. A synchronous judgment is a direct
        execution's completed passed or failed result; an asynchronous one is
        a queued execution whose server status is completed with such a
        result. Two direct results never stand in for a queued one.
        """
        components = set(self.kinds) - {"Service", "Verifier"}
        if self.service is None or "Verifier" not in self.kinds or not components:
            return "declared"
        if not self.telemetry_runs:
            return "telemetry"
        if not any(self.custom_runs.values()):
            return "custom_rows"
        if not any(judgment.judged() for judgment in self.sync):
            return "sync_judgment"
        if not any(judgment.judged() for judgment in self.queued):
            return "async_judgment"
        return None

    def progress(self) -> int:
        """How many milestones hold, for choosing the furthest Service."""
        point = self.failure_point()
        return len(MILESTONES) if point is None else MILESTONES.index(point)

    def report(self) -> dict[str, Any]:
        """The JSON document the coordinator records."""
        return {**asdict(self), "failure_point": self.failure_point()}


class Observer:
    """Reads one agent's evidence through its own authorized client."""

    def __init__(self, client: WyrdClient) -> None:
        self.client = client
        self.cards = Cards(client)
        self.bifrost = Bifrost(client=client)

    def observe(self) -> Evidence:
        """Evidence for the declared Service that got furthest, or empty evidence."""
        best = Evidence()
        # ponytail: first page of Services; an agent declaring >200 is a failure anyway.
        for summary in self.cards.list(kind="Service", limit=200).items:
            evidence = self.service(summary.card_ref)
            if evidence.progress() > best.progress() or best.service is None:
                best = evidence
        return best

    def watch(self, timeout: float, interval: float = 5.0) -> Evidence:
        """Observe until every milestone holds or ``timeout`` seconds pass.

        Analytical visibility trails buffered writes, so a missing row is
        re-read rather than judged at once; the last observation is returned.
        """
        deadline = time.monotonic() + timeout
        while True:
            evidence = self.observe()
            if evidence.failure_point() is None or time.monotonic() >= deadline:
                return evidence
            time.sleep(interval)

    def service(self, ref: CardRef) -> Evidence:
        """Pull ``ref``'s graph as metadata and read its correlated evidence."""
        evidence = Evidence(service=str(ref))
        with tempfile.TemporaryDirectory(prefix="wyrd-readiness-graph-") as scratch:
            bundle = Path(scratch) / "graph"
            self.cards.pull(ref, bundle, metadata_only=True)
            manifest = yaml.safe_load((bundle / "metadata.yaml").read_text())
            uids: list[str] = []
            tables: list[str] = []
            for card in manifest["cards"]:
                envelope = yaml.safe_load((bundle / card["card_path"]).read_text())
                evidence.kinds.append(envelope["kind"])
                uids.append(str(card["card_ref"]["uid"]))
                if envelope["kind"] == "Service" and card["card_ref"]["uid"] == ref.uid:
                    tables = [table["name"] for table in envelope["spec"].get("tables", [])]
        evidence.kinds.sort()
        cards = ", ".join(f"${index}" for index in range(1, len(uids) + 1))
        evidence.telemetry_runs = self.runs(
            f"SELECT DISTINCT run_id FROM vala.traces.spans "
            f"WHERE run_id IS NOT NULL AND card_uid IN ({cards}) LIMIT {ROW_LIMIT}",
            uids,
            evidence,
        )
        for table in tables:
            if not TABLE_NAME.match(table):
                evidence.notes.append(f"skipped unexpected table name {table!r}")
                continue
            evidence.custom_runs[table] = self.runs(
                f"SELECT DISTINCT run_id FROM vala.datasets.{table} "
                f"WHERE run_id IS NOT NULL AND card_uid IN ({cards}) LIMIT {ROW_LIMIT}",
                uids,
                evidence,
            )
        evidence.sync = [
            Judgment(row["execution_id"], row["execution_status"], row["verdict"], row["run_id"])
            for row in self.rows(
                "SELECT r.execution_id, r.execution_status, r.verdict, r.run_id "
                "FROM vala.verification.results r "
                "JOIN vala.verification.execution_inputs i ON i.execution_id = r.execution_id "
                f"WHERE NOT i.queue_execution AND r.card_uid IN ({cards}) LIMIT {ROW_LIMIT}",
                uids,
                evidence,
            )
        ]
        queued = self.rows(
            "SELECT DISTINCT execution_id FROM vala.verification.execution_inputs "
            f"WHERE queue_execution AND card_uid IN ({cards}) LIMIT {ROW_LIMIT}",
            uids,
            evidence,
        )
        evidence.queued = [self.execution(row["execution_id"]) for row in queued]
        return evidence

    def execution(self, execution_id: str) -> Judgment:
        """A queued execution's server-owned status; a staged result row alone is not its outcome."""
        try:
            state = Execution(execution_id, self.client).status()
        except WyrdError as error:
            return Judgment(execution_id, error.code, None, None)
        result = state.get("result")
        if result is None:
            return Judgment(execution_id, state["status"], None, None)
        return Judgment(
            execution_id, state["status"], result["outcome"]["verdict"], result.get("run_id")
        )

    def runs(self, sql: str, params: list[str], evidence: Evidence) -> list[str]:
        """The non-null ``run_id`` values ``sql`` returns."""
        return sorted(str(row["run_id"]) for row in self.rows(sql, params, evidence))

    def rows(self, sql: str, params: list[str], evidence: Evidence) -> list[dict[str, Any]]:
        """Run one evidence query; a refused or degraded answer counts as no rows.

        The reason is kept in ``evidence.notes`` so an empty milestone shows why.
        """
        try:
            result = self.bifrost.sql(sql, list(params))
        except WyrdError as error:
            evidence.notes.append(f"{error.code}: {sql.split(' FROM ')[1].split()[0]}")
            return []
        outcome = result.terminal().outcome
        if outcome != "success":
            evidence.notes.append(f"query {outcome}: {sql.split(' FROM ')[1].split()[0]}")
            return []
        return result.to_arrow().to_pylist()


def main() -> int:
    """Print one agent's evidence and exit 0 only when every milestone holds."""
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=180.0)
    evidence = Observer(WyrdClient()).watch(parser.parse_args().timeout)
    json.dump(evidence.report(), sys.stdout)
    return 0 if evidence.failure_point() is None else 1


if __name__ == "__main__":
    raise SystemExit(main())
