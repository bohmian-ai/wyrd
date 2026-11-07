"""A hosted Agent publishes its private scratch for a later step, and the publications read back.

The ``report-pipeline`` drafter writes a report, a tree, and JSON in its
scratch and publishes them by name; every refused publication or write
carries its typed rule. The consumer reads exactly the accepted content
through its mounts and the collection manifest, and the public readback
lists, reads, and plans downloads of the same content. A principal granted
only ``fs.write`` writes scratch while every other call is denied, so
nothing is published.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import pytest
from wyrd import verify
from wyrd.cards import Cards
from wyrd.client import WyrdClient
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer

from .support import ASSESSMENT, REPORT, Receiver, client_of, declare, pull, read_text, tool_results
from .test_local_development import configure_gateway
from .test_verifier_building_blocks import grants, requirement, step

pytestmark = pytest.mark.integration

INVALID = "WYRD_SPEC_400_VALIDATION"

DENIED = "WYRD_PERMISSION_403_DENIED_RBAC"


@pytest.fixture(scope="module")
def publish_desk_bundle(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path_factory: pytest.TempPathFactory
) -> Path:
    """The ``publish-desk`` Service, registered and downloaded, with ``openai`` deployed."""
    configure_gateway(WyrdClient())
    declare(cards, "cards/hosted_agent_tools/publish-desk.yaml")
    return pull(cards, "publish-desk", tmp_path_factory.mktemp("publish-desk"))


def verify_as(
    server: WyrdTestServer,
    bundle: Path,
    name: str,
    tool_scope: str | dict[str, object],
    question: str,
) -> verify.VerificationResult:
    """Verify ``question`` with ``report-pipeline`` now, as a new key ``name`` granted ``tool_scope`` tools."""
    key = server.scoped_api_key(name, grants(tool_scope))
    state = WyrdState.from_path(bundle, WyrdClient(credential=key))
    return state.run().observe.verify("report-pipeline", question)


def requests(receiver: Receiver, prompt: str) -> list[dict[str, Any]]:
    """Every Chat Completions body whose user prompt is exactly ``prompt``, in arrival order."""
    return [
        delivery.body
        for delivery in receiver.to("/v1/chat/completions")
        if any(
            each.get("role") == "user" and each.get("content") == prompt
            for each in delivery.body.get("messages", [])
        )
    ]


def results(receiver: Receiver, prompt: str) -> dict[str, Any]:
    """Every tool result the Agent prompted with ``prompt`` returned, by call ID, from its last request."""
    return tool_results(requests(receiver, prompt)[-1]["messages"])


def offered(body: dict[str, Any]) -> list[str]:
    """The function names the model was offered in ``body``, sorted."""
    return sorted(tool["function"]["name"] for tool in body.get("tools", []))


def refusal(result: dict[str, Any]) -> tuple[object, object]:
    """The stable code and repair rule of a refused tool result."""
    return result.get("code"), result.get("details", {}).get("rule")


def publications(execution: verify.Execution) -> list[Any]:
    """Every publication of ``execution``, read through the paged listing."""
    listed, cursor = [], None
    while True:
        page = execution.publications(limit=2, cursor=cursor)
        listed += page["items"]
        cursor = page["next_cursor"]
        if cursor is None:
            return listed


DIGESTS = {
    "report": "Ir06sS2VoUyv9Zu1ER4D6fjj0mKQiWHzAgYl/q7kE3M=",
    "tree": "9E5k5185SOn3P436lHIcTOjLtPJlxHkMcCstQc+/J1M=",
}
"""The base64 SHA-256 a download plan reports for each publication's planned file."""


def test_agent_publishes_file_directory_and_json(
    wyrd_server: WyrdTestServer, receiver: Receiver, publish_desk_bundle: Path
) -> None:
    result = verify_as(wyrd_server, publish_desk_bundle, "publish_every_tool", "all", "FULL-RUN")

    assert requirement(result, "consumed")["outcome"] == "passed", result.steps
    assert offered(requests(receiver, "DRAFT: FULL-RUN")[0]) == [
        "code_run",
        "fs_read",
        "fs_write",
        "publish",
    ]
    assert offered(requests(receiver, "CONSUME: FULL-RUN")[0]) == ["fs_list", "fs_read"]
    drafter = results(receiver, "DRAFT: FULL-RUN")
    for call, name, kind in [
        ("p-report", "report", "file"),
        ("p-tree", "tree", "directory"),
        ("p-assessment", "assessment", "json"),
        ("p-nothing", "nothing", "json"),
    ]:
        assert drafter[call] == {"name": name, "kind": kind}, call
    assert read_text(drafter["r1"]) == REPORT.rstrip()
    assert drafter["w6"] == {"path": "/work/report.md", "bytes": 24}
    for call, rule in [
        ("x-conflict", "publication_name"),
        ("x-both", "publication_source"),
        ("x-neither", "publication_source"),
        ("x-missing", "workspace_missing"),
        ("x-inputs", "publication_path"),
        ("x-escape", "publication_path"),
        ("x-write-inputs", "write_path"),
        ("x-write-escape", "write_path"),
    ]:
        assert refusal(drafter[call]) == (INVALID, rule), call
    assert (drafter["x-name"]["code"], drafter["x-name"]["details"]["field"]) == (INVALID, "name")
    assert drafter["x-forged"]["code"] == "SKALD_TOOL_422_INPUT", "the host owns the producer"

    consumer = results(receiver, "CONSUME: FULL-RUN")
    assert read_text(consumer["c-report"]) == REPORT.rstrip(), "later scratch never republishes"
    assert read_text(consumer["c-entry"]) == REPORT.rstrip()
    assert [each["path"] for each in consumer["c-tree"]["files"]] == [
        "/inputs/tree/a.md",
        "/inputs/tree/nested dir/é b.md",
    ]
    assert json.loads(read_text(consumer["c-assessment"])) == ASSESSMENT
    assert read_text(consumer["c-nothing"]) == "null"
    manifest = json.loads(read_text(consumer["c-manifest"]))
    assert sorted((entry["name"], entry["kind"]) for entry in manifest["entries"]) == [
        ("assessment", "json"),
        ("nothing", "json"),
        ("report", "file"),
        ("tree", "directory"),
    ]

    execution = verify.Execution.connect(result.execution_id, client_of(wyrd_server))
    listed = publications(execution)
    assert sorted(
        (each["producer"]["step_id"], each["name"], each["producer"]["attempt"], each["kind"])
        for each in listed
    ) == [
        ("draft", "assessment", 1, "json"),
        ("draft", "nothing", 1, "json"),
        ("draft", "report", 1, "file"),
        ("draft", "tree", 1, "directory"),
    ], "refused calls and the final answer publish nothing"
    draft: verify.Producer = {"step_id": "draft", "attempt": 1}
    tree = execution.publication(draft, "tree")["manifest"]
    assert [each["path"] for each in tree["files"]] == ["a.md", "nested dir/é b.md"]
    assert tree["directories"] == ["empty", "nested dir"]
    assessment = execution.publication(draft, "assessment")
    assert (assessment["kind"], assessment["value"]) == ("json", ASSESSMENT)
    nothing = execution.publication(draft, "nothing")
    assert (nothing["kind"], nothing["value"]) == ("json", None)
    for name, path, content in [
        ("report", None, REPORT.encode()),
        ("tree", "nested dir/é b.md", b"beta"),
    ]:
        planned = execution.download(draft, name, path)
        assert (planned["size_bytes"], planned["sha256"]) == (len(content), DIGESTS[name]), name


def test_tool_calls_need_their_own_grant(
    wyrd_server: WyrdTestServer, receiver: Receiver, publish_desk_bundle: Path
) -> None:
    result = verify_as(
        wyrd_server,
        publish_desk_bundle,
        "publish_writes_only",
        {"tool": "fs.write"},
        "NARROWED-RUN",
    )

    calls = step(result, "draft").get("tool_calls", [])
    assert calls
    for call in calls:
        denied = DENIED in call.get("error", "")
        if call["tool"] == "fs.write":
            assert not denied, call
        else:
            assert denied, call
            assert ("output_sha256" in call, call["returned_bytes"]) == (False, 0), call
    denied = results(receiver, "DRAFT: NARROWED-RUN")
    assert "low" not in json.dumps(denied["p-assessment"]), "a denied publish echoes no content"
    execution = verify.Execution.connect(result.execution_id, client_of(wyrd_server))
    assert publications(execution) == []
    consume = step(result, "consume")
    assert consume["status"] == "failed"
    assert "published no" in consume.get("reason", ""), consume
    assert requests(receiver, "CONSUME: NARROWED-RUN") == [], "the consumer never reached the model"
