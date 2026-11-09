"""The ``card_authoring`` story, authored in Python through ``wyrd.authoring``.

Service ``finance-release`` composes Agents ``report-reviewer`` and
``release-controls``, each verified by its own two-step Verifier
(``report-policy``, ``release-access``); the report judge reads the extracted
``company-sharing-policy`` Artifact. The graph is built in code and declared
through ``Cards.declare`` or ``WyrdState.declare``, which publish a local
Artifact first and then declare the graph. The session's local provider plays
the policy judge (``conftest.sharing_verdict``).
"""

from __future__ import annotations

import contextlib
import io
import json
import runpy
import shutil
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest
from pydantic import BaseModel, ConfigDict
from wyrd import WyrdError
from wyrd.authoring import (
    Action,
    Agent,
    Artifact,
    ArtifactFile,
    CardRef,
    Check,
    CodeSource,
    Component,
    InputSlot,
    Requirement,
    Service,
    Step,
    Verifier,
)
from wyrd.cards import CardKind, Cards, DeclarationReceipt
from wyrd.cards import CardRef as RegisteredRef
from wyrd.client import WyrdClient
from wyrd.prompt import Prompt, PromptCard
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verify import VerificationResult

from ..conftest import serving
from ..support import FIXTURES, Receiver
from ..test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = FIXTURES.parent / "examples/finance-release"
"""The finance release example, run from an empty directory as a reader runs it."""

STORY = FIXTURES / "cards/card_authoring"
POLICY = STORY / "company-sharing-policy.pdf"
POLICY_FILE = "extracted/company-sharing-policy.pdf.json"
"""The extracted policy file every report judge mounts."""

RECIPIENT = {
    "type": "object",
    "additionalProperties": False,
    "required": ["email", "authorized", "permitted_purposes"],
    "properties": {
        "email": {"type": "string"},
        "authorized": {"type": "boolean"},
        "permitted_purposes": {"type": "array", "items": {"type": "string"}},
    },
}
DESTINATION = {
    "type": "object",
    "additionalProperties": False,
    "required": ["url", "encrypted", "restricted_access", "public_sharing"],
    "properties": {
        "url": {"type": "string"},
        "encrypted": {"type": "boolean"},
        "restricted_access": {"type": "boolean"},
        "public_sharing": {"type": "boolean"},
    },
}
REPORT = {
    "type": "object",
    "additionalProperties": False,
    "required": ["text"],
    "properties": {"text": {"type": "string"}},
}


class Evidence(BaseModel):
    """Release evidence the caller holds as Pydantic models."""

    model_config = ConfigDict(extra="forbid", strict=True)


class Report(Evidence):
    text: str


class Recipient(Evidence):
    email: str
    authorized: bool
    permitted_purposes: list[str]


class Destination(Evidence):
    url: str
    encrypted: bool
    restricted_access: bool
    public_sharing: bool


def permitted() -> dict[str, Any]:
    """An external audit of a clean report to an authorized auditor over a controlled destination."""
    return {
        "report": Report(text="Synthetic customer report: quarterly total $120."),
        "recipient": Recipient(
            email="auditor@example.test",
            authorized=True,
            permitted_purposes=["external-audit"],
        ),
        "purpose": "external-audit",
        "destination": Destination(
            url="https://auditor.example.test/reports",
            encrypted=True,
            restricted_access=True,
            public_sharing=False,
        ),
    }


def policy_judge(name: str, version: str) -> Agent:
    """A policy-judge Agent ``name`` at ``version``, as the story's Agent files declare it."""
    return Agent(
        name,
        version,
        execution_route={"kind": "wyrd_gateway", "deployment": "openai"},
        tool_names=["fs.read"],
        run_config={"max_iterations": 3},
        prompt={
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [
                {
                    "role": "user",
                    "content": (
                        "POLICY JUDGE: read /inputs/policy.json with fs.read. Determine whether"
                        " the proposed release satisfies the company policy. Cite original"
                        " request and policy evidence. Report: {{report}} Recipient:"
                        " {{recipient}} Purpose: {{purpose}} Destination: {{destination}}"
                    ),
                }
            ],
            "response_format": "verdict",
        },
    )


def fact_step(
    id: str,
    entrypoint: str,
    inputs: list[str],
    requirement: tuple[str, str, str, bool],
) -> Step:
    """A code step running ``entrypoint`` over ``inputs``, judged by one equality requirement."""
    requirement_id, statement, field, expected = requirement
    action = Action.code(CodeSource.file(STORY / "release_checks.py"), entrypoint)
    action.inputs({name: f"inputs.{name}" for name in inputs})
    check = Check.assert_(f"steps.{id}.output.{field}", "equals", expected)
    return Step(id, action, requirements=[Requirement(requirement_id, statement, check)])


def release_inputs() -> dict[str, Any]:
    """The recipient, purpose, and destination slots: two JSON Schemas and a plain ``str``."""
    return {"recipient": RECIPIENT, "purpose": str, "destination": DESTINATION}


def report_policy(version: str, judge: Agent, policy: ArtifactFile) -> Verifier:
    """``report-policy`` at ``version``: the identifier scan with the judge reading ``policy``, then context."""
    verifier = Verifier(
        "report-policy",
        version,
        description="The report is fit to share under the company sharing policy.",
        inputs=release_inputs(),
    )
    verifier.add_input("report", REPORT)
    call = Check.agent(judge).inputs(
        {name: f"inputs.{name}" for name in ("report", "recipient", "purpose", "destination")}
    )
    call.add_workspace("/inputs/policy.json", policy)
    scan = fact_step(
        "policy",
        "report_facts",
        ["report"],
        (
            "no-bank-account-identifier",
            "The report contains no prohibited bank identifier.",
            "contains_bank_identifier",
            False,
        ),
    )
    scan.add_requirement(
        Requirement(
            "sharing-policy-satisfied",
            "The release satisfies the company sharing policy.",
            call,
        )
    )
    context = fact_step(
        "policy-context",
        "policy_context",
        ["report", "purpose"],
        ("usable-report", "The proposed release contains report text.", "has_report", True),
    )
    return verifier.steps([scan, context])


def release_access(version: str, description: str | None = None) -> Verifier:
    """``release-access`` at ``version``: independent recipient and destination steps, optional ``notes``."""
    verifier = Verifier(
        "release-access",
        version,
        description=description or "The recipient and destination may receive the report.",
        inputs=release_inputs(),
    )
    verifier.add_input("notes", InputSlot.json({"type": "string"}).required(False))
    verifier.add_step(
        fact_step(
            "recipient",
            "recipient_facts",
            ["recipient", "purpose"],
            (
                "recipient-authorized",
                "The recipient is authorized for this purpose.",
                "authorized",
                True,
            ),
        )
    ).add_step(
        fact_step(
            "destination",
            "destination_facts",
            ["destination"],
            (
                "destination-controls-satisfied",
                "The destination has appropriate access controls.",
                "appropriate",
                True,
            ),
        )
    )
    return verifier


def finance_release(version: str, policy: ArtifactFile, access: Verifier) -> Service:
    """``finance-release`` with every Card at ``version``, its reviewer judged against ``policy``."""
    reviewer = policy_judge("report-reviewer", version)
    service = Service("finance-release", version, service_type="api")
    service.add_component(
        Component("report-reviewer", reviewer).verified_by(
            [report_policy(version, reviewer, policy)]
        )
    )
    service.add_component(
        Component("release-controls", policy_judge("release-controls", version)).verified_by(
            [access]
        )
    )
    return service


def local_policy(version: str, source: Path = POLICY) -> Artifact:
    """The local policy Artifact ``version``, published from ``source`` with its documents extracted."""
    return Artifact.from_path("company-sharing-policy", version, source).extract(True)


def registered_policy(version: str) -> ArtifactFile:
    """The registered policy Artifact ``version``'s extracted file, used without uploading."""
    return CardRef.artifact("company-sharing-policy", version).file(POLICY_FILE)


def baseline(policy: ArtifactFile) -> Service:
    """The baseline story graph over ``policy``."""
    return finance_release("1.0.0", policy, release_access("1.0.0"))


def verify_both(
    state: WyrdState, request: dict[str, Any]
) -> tuple[VerificationResult | WyrdError, VerificationResult | WyrdError]:
    """Verify ``request`` against both components, each under its own subject; a refusal is returned."""

    def attempt(
        alias: str, verifier: str, slots: tuple[str, ...]
    ) -> VerificationResult | WyrdError:
        inputs = {slot: request[slot] for slot in slots if slot in request}
        try:
            return state.run(alias).observe.verify(verifier, **inputs)
        except WyrdError as refused:
            return refused

    return (
        attempt(
            "report-reviewer", "report-policy", ("report", "recipient", "purpose", "destination")
        ),
        attempt(
            "release-controls", "release-access", ("recipient", "purpose", "destination", "notes")
        ),
    )


def judged(result: VerificationResult | WyrdError) -> VerificationResult:
    """``result`` as a judged result; a refusal fails the test with its code."""
    assert isinstance(result, VerificationResult), result
    return result


def outcome(result: VerificationResult, id: str) -> str:
    """The outcome of requirement ``id`` in ``result``."""
    [found] = [each for each in result.requirements if each["id"] == id]
    return found["outcome"]


def releases(report: VerificationResult, access: VerificationResult) -> bool:
    """Whether the simulated release may proceed: only both passed verdicts permit it."""
    return report.verdict == access.verdict == "passed"


def judge_calls(receiver: Receiver) -> int:
    """The number of policy-judge requests the local provider has received."""
    return sum(
        "POLICY JUDGE" in json.dumps(each.body) for each in receiver.to("/v1/chat/completions")
    )


def resolved(receipt: DeclarationReceipt) -> dict[str, tuple[RegisteredRef, str]]:
    """Each declared Card of ``receipt`` by name, with its resolved identity and spec hash."""
    return {each.card_ref.name: (each.card_ref, each.spec_hash) for each in receipt.outcomes}


def declared_kinds(receipt: DeclarationReceipt) -> list[CardKind]:
    """The kinds ``receipt`` declared."""
    return [each.card_ref.kind for each in receipt.outcomes]


def policy_published(cards: Cards, version: str) -> bool:
    """Whether policy ``version`` is registered and holds its extracted policy file."""
    try:
        files = cards.list_artifacts(RegisteredRef("Artifact", "company-sharing-policy", version))
    except WyrdError:
        return False
    return any(each["relative_path"] == POLICY_FILE for each in files)


def mounted_policy(cards: Cards, version: str) -> str:
    """The policy version the registered ``report-policy`` ``version`` mounts."""
    spec = cards.get(RegisteredRef("Verifier", "report-policy", version)).spec
    check = spec["steps"][0]["requirements"][1]["check"]
    return check["workspace"]["/inputs/policy.json"]["artifact"]["card"]["version"]


def code(raised: pytest.ExceptionInfo[WyrdError]) -> str:
    """The stable code ``raised`` carries."""
    return raised.value.code


def test_multi_component_service_verifies_each_subject(
    wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver
) -> None:
    """Each authored component is verified under its own subject by its own two-step Verifier.

    The permitted release passes both and releases; each prohibited variant
    fails only the Verifier that owns it; missing evidence and an uncertain
    provider are inconclusive and hold; a malformed report and the other
    component's Verifier are refused; an unchanged rerun declares nothing new.
    """
    configure_gateway(WyrdClient())
    service = baseline(local_policy("1.0.0").file(POLICY_FILE))
    state = WyrdState.declare(service)
    asked = judge_calls(receiver)
    try:
        report, access = map(judged, verify_both(state, permitted()))
        for result, alias, verifier, steps in (
            (report, "report-reviewer", "report-policy", {"policy", "policy-context"}),
            (access, "release-controls", "release-access", {"recipient", "destination"}),
        ):
            assert result.subject == state.card_ref(alias), result.subject
            assert result.verifier.name == verifier
            assert {step["id"] for step in result.steps} == steps, result.steps
            assert all(step["status"] == "succeeded" for step in result.steps), result.steps
        assert releases(report, access), (report.requirements, access.requirements)
        assert judge_calls(receiver) - asked == 2, "one policy read, then the verdict"
        [policy] = [
            each for each in report.requirements if each["id"] == "sharing-policy-satisfied"
        ]
        assert policy["outcome"] == "passed", policy
        citations = policy.get("citations", [])
        assert {"slot": "report", "locator": {"json_pointer": "/text"}} in citations, citations
        assert any(
            each["slot"].startswith("attachment_") and "lines" in each["locator"]
            for each in citations
        ), "the judge cites the policy line it read"

        prohibited = permitted() | {
            "report": {"text": "Synthetic report. Bank identifier: BANK-ACCOUNT-DEMO-001."}
        }
        unauthorized = permitted()
        unauthorized["recipient"] = unauthorized["recipient"].model_copy(
            update={"authorized": False}
        )
        public = permitted()
        public["destination"] = public["destination"].model_copy(update={"public_sharing": True})
        for request, verdicts, failed in (
            (prohibited, ("failed", "passed"), "no-bank-account-identifier"),
            (unauthorized, ("passed", "failed"), "recipient-authorized"),
            (public, ("passed", "failed"), "destination-controls-satisfied"),
        ):
            report, access = map(judged, verify_both(state, request))
            assert (report.verdict, access.verdict) == verdicts, failed
            owner = report if verdicts[0] == "failed" else access
            assert outcome(owner, failed) == "failed", owner.requirements
            assert not releases(report, access), "a failure holds the release"

        missing = permitted()
        del missing["report"]
        report = judged(verify_both(state, missing)[0])
        assert report.verdict == "inconclusive", report.requirements
        for id in ("no-bank-account-identifier", "usable-report"):
            assert outcome(report, id) == "inconclusive", report.requirements
        assert all(each["outcome"] != "failed" for each in report.requirements)

        uncertain = permitted() | {
            "report": Report(text="Synthetic report flagged PROVIDER-UNCERTAIN.")
        }
        report, access = map(judged, verify_both(state, uncertain))
        assert report.verdict == "inconclusive", report.requirements
        assert outcome(report, "sharing-policy-satisfied") == "inconclusive"
        for id in ("no-bank-account-identifier", "usable-report"):
            assert outcome(report, id) == "passed", report.requirements
        assert not releases(report, access), "uncertainty holds the release"

        refused = verify_both(state, permitted() | {"report": {"text": 123}})[0]
        assert isinstance(refused, WyrdError), refused
        assert refused.code == "WYRD_VERIFICATION_400_INPUT_INVALID", refused

        for alias, verifier in (
            ("report-reviewer", "release-access"),
            ("release-controls", "report-policy"),
        ):
            request = permitted()
            with pytest.raises(WyrdError) as unbound:
                state.run(alias).observe.verify(
                    verifier,
                    recipient=request["recipient"],
                    purpose=request["purpose"],
                    destination=request["destination"],
                )
            assert code(unbound) == "WYRD_SDK_404_UNKNOWN_VERIFIER"
    finally:
        state.shutdown()

    rerun = cards.declare(service)
    assert rerun.root.name == "finance-release"
    assert all(each.outcome == "idempotent_noop" for each in rerun.outcomes), rerun.outcomes
    again = WyrdState.declare(service)
    try:
        for alias in ("report-reviewer", "release-controls"):
            assert again.card_ref(alias) == state.card_ref(alias)
    finally:
        again.shutdown()


def test_local_artifacts_and_declaration_parity(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path
) -> None:
    """Local Artifacts publish first through both entry points and match the file graph.

    A registered reference uploads nothing; a new local version leaves the
    old graph on the old policy; a graph refused after its policy published
    keeps that policy for reuse; changed bytes or content under a published
    version, an unreadable source, an unregistered reference, and a reader
    are refused with their stable codes; path and holder declaration still
    work.
    """
    configure_gateway(WyrdClient())
    authored = baseline(local_policy("1.0.0").file(POLICY_FILE))
    receipt = cards.declare(authored)
    assert receipt.root.name == "finance-release"
    assert CardKind.Artifact not in declared_kinds(receipt), receipt.outcomes
    assert policy_published(cards, "1.0.0")
    assert mounted_policy(cards, "1.0.0") == "1.0.0"

    files = cards.declare_from_path(STORY / "finance-release.yaml")
    assert files.root == receipt.root
    assert resolved(files) == resolved(receipt), "the authored graph is the file graph"
    assert all(each.outcome == "idempotent_noop" for each in files.outcomes), files.outcomes
    assert resolved(cards.declare(baseline(registered_policy("1.0.0")))) == resolved(files)

    from_code = WyrdState.declare(baseline(registered_policy("1.0.0")))
    from_file = WyrdState.declare(STORY / "finance-release.yaml")
    try:
        for alias in ("report-reviewer", "release-controls"):
            assert from_code.card_ref(alias) == from_file.card_ref(alias)
        for _ in range(2):
            report, access = map(judged, verify_both(from_code, permitted()))
            assert releases(report, access), (report.requirements, access.requirements)
            assert report.verifier.version == "1.0.0"
    finally:
        from_file.shutdown()

    with pytest.raises(WyrdError) as unregistered:
        cards.declare(finance_release("9.0.0", registered_policy("9.0.0"), release_access("9.0.0")))
    assert code(unregistered) == "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
    assert not policy_published(cards, "9.0.0")

    revised = tmp_path / "company-sharing-policy.pdf"
    revised.write_bytes(POLICY.read_bytes() + b"% revision 2\n")
    revised_state = WyrdState.declare(
        finance_release(
            "2.0.0", local_policy("2.0.0", revised).file(POLICY_FILE), release_access("2.0.0")
        )
    )
    try:
        assert policy_published(cards, "2.0.0")
        assert mounted_policy(cards, "2.0.0") == "2.0.0"
        assert mounted_policy(cards, "1.0.0") == "1.0.0"
        assert judged(verify_both(revised_state, permitted())[0]).verifier.version == "2.0.0"
        old = judged(verify_both(from_code, permitted())[0])
        assert old.verifier.version == "1.0.0", "the old state keeps the old policy"
    finally:
        revised_state.shutdown()
        from_code.shutdown()

    with pytest.raises(WyrdError) as drifted:
        cards.declare(baseline(local_policy("1.0.0", revised).file(POLICY_FILE)))
    assert code(drifted) == "WYRD_REGISTRY_409_SPEC_DRIFT"
    changed = finance_release(
        "1.0.0", registered_policy("1.0.0"), release_access("1.0.0", "Changed after publication.")
    )
    with pytest.raises(WyrdError) as drifted:
        cards.declare(changed)
    assert code(drifted) == "WYRD_REGISTRY_409_SPEC_DRIFT"
    with pytest.raises(WyrdError) as drifted:
        WyrdState.declare(changed)
    assert code(drifted) == "WYRD_REGISTRY_409_SPEC_DRIFT"

    with pytest.raises(WyrdError) as unreadable:
        cards.declare(
            finance_release(
                "4.0.0",
                local_policy("4.0.0", tmp_path / "missing.pdf").file(POLICY_FILE),
                release_access("4.0.0"),
            )
        )
    assert code(unreadable) == "WYRD_LOADER_400_INVALID_ENVELOPE"
    with pytest.raises(WyrdError):
        cards.get(RegisteredRef("Service", "finance-release", "4.0.0"))

    judge = policy_judge("report-reviewer", "3.0.0")
    orphaned = Service("finance-release", "3.0.0")
    orphaned.add_component(
        Component("report-reviewer", judge).verified_by(
            [report_policy("3.0.0", judge, local_policy("3.0.0").file(POLICY_FILE))]
        )
    )
    orphaned.add_component(
        Component("release-controls", CardRef.agent("unregistered-controls", "1.0.0")).verified_by(
            [release_access("3.0.0")]
        )
    )
    with pytest.raises(WyrdError) as refused:
        cards.declare(orphaned)
    assert code(refused) == "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY"
    assert policy_published(cards, "3.0.0"), "the published policy outlives the refused graph"
    recovered = cards.declare(
        finance_release("3.0.0", registered_policy("3.0.0"), release_access("3.0.0"))
    )
    assert CardKind.Artifact not in declared_kinds(recovered), recovered.outcomes
    assert mounted_policy(cards, "3.0.0") == "3.0.0"

    root = cards.declare(local_policy("5.0.0"))
    assert root.root.name == "company-sharing-policy" and root.root.version == "5.0.0"
    assert root.root.uid is not None
    assert declared_kinds(root) == [CardKind.Artifact]

    reader = WyrdClient(credential=wyrd_server.scoped_api_key("card_reader", ["cards:read"]))
    with pytest.raises(WyrdError) as denied:
        Cards(reader).declare(authored)
    assert code(denied) == "WYRD_PERMISSION_403_DENIED_RBAC"
    with pytest.raises(WyrdError) as denied:
        WyrdState.declare(authored, reader)
    assert code(denied) == "WYRD_PERMISSION_403_DENIED_RBAC"

    holder = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
        name="authoring-holder",
        version="0.1.0",
    )
    assert cards.declare(holder).root.uid == holder.uid


@pytest.fixture
def finance_reader(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Iterator[Path]:
    """An empty directory holding only the example's files, reaching a fresh server as its administrator.

    The server is its own so the example's ``finance-release`` 1.0.0 meets no
    story graph of the same identity.
    """
    reader = tmp_path / "finance-release"
    reader.mkdir()
    for name in ("finance_release.py", "company-sharing-policy.pdf"):
        shutil.copy(EXAMPLE / name, reader)
    monkeypatch.chdir(reader)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    with (
        serving() as model,
        WyrdTestServer(
            verification_runtime=True, provider_base_url=model.url, mutate_env=False
        ) as server,
    ):
        monkeypatch.setenv("WYRD_SERVER_URL", server.base_url)
        monkeypatch.setenv("WYRD_GRPC_URL", server.grpc_url)
        monkeypatch.setenv("WYRD_API_KEY", server.tenant_admin_key())
        configure_gateway(WyrdClient())
        yield reader


def test_finance_release_example(finance_reader: Path) -> None:
    """The example passes the permitted release and fails the one naming a bank account."""
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        runpy.run_path(str(finance_reader / "finance_release.py"), run_name="__main__")
    assert printed.getvalue() == (
        "Quarterly total $120.\n"
        "  release-check: passed\n"
        "Quarterly total $120. Paid from BANK-ACCOUNT-DEMO-001.\n"
        "  release-check: failed\n"
        "    not passed: The report names no bank account.\n"
    )
