"""The ``card_authoring`` story, defined in Python through ``wyrd.verify``.

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
import sys
from collections.abc import Iterator
from pathlib import Path
from typing import Any

import pytest
from pydantic import BaseModel, ConfigDict
from wyrd import WyrdError
from wyrd.cards import CardKind, Cards, DeclarationReceipt
from wyrd.cards import CardRef as RegisteredRef
from wyrd.client import WyrdClient
from wyrd.prompt import Prompt, PromptCard
from wyrd.state import WyrdState
from wyrd.testing import WyrdTestServer
from wyrd.verify import (
    Action,
    Agent,
    AgentRunConfig,
    Artifact,
    ArtifactFile,
    Audit,
    Binding,
    CardRef,
    Check,
    CodeSource,
    Component,
    Data,
    ExecutionRoute,
    Experiment,
    InputSlot,
    Mcp,
    Model,
    Operator,
    Requirement,
    Service,
    Step,
    Trigger,
    VerificationResult,
    Verifier,
)

from ..conftest import serving
from ..support import FIXTURES, Receiver
from ..test_local_development import configure_gateway

pytestmark = pytest.mark.integration

EXAMPLE = FIXTURES.parent / "examples/finance-release"
"""The finance release example, run from an empty directory as a reader runs it."""

SERVICE_EXAMPLE = EXAMPLE / "service"
"""The finance release split into component modules, composed by its ``main.py``."""

STORY = FIXTURES / "cards/card_authoring"
OMITTED = STORY / "omitted"
"""A Service graph whose files name no version."""
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
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        tool_names=["fs.read"],
        run_config=AgentRunConfig(max_iterations=3),
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
    for name in inputs:
        action.bind_input(name, f"inputs.{name}")
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
        {name: f"inputs.{name}" for name in ("report", "recipient", "purpose")}
    )
    call.bind_input("destination", "inputs.destination").add_workspace(
        "/inputs/policy.json", policy
    )
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
    """``release-access`` at ``version``: independent recipient and destination steps, unused ``notes``."""
    verifier = Verifier(
        "release-access",
        version,
        description=description or "The recipient and destination may receive the report.",
        inputs=release_inputs(),
    )
    verifier.add_input("notes", {"type": "string"})
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


def summarized_access(version: str) -> Verifier:
    """``release-access`` at ``version`` with a ``summary`` step consuming both fact steps' outputs."""
    summary = Action.code(CodeSource.file(STORY / "release_checks.py"), "summarize_findings")
    summary.inputs(
        {"recipient": "steps.recipient.output", "destination": "steps.destination.output"}
    )
    check = Check.assert_("steps.summary.output.releasable", "equals", True)
    return release_access(version).add_step(
        Step(
            "summary",
            summary,
            depends_on=["recipient", "destination"],
            requirements=[
                Requirement(
                    "release-permitted",
                    "Both the recipient and the destination permit the release.",
                    check,
                )
            ],
        )
    )


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


def canonical_release(
    version: str, policy: ArtifactFile, access: Verifier, reviewer: Agent | None = None
) -> Service:
    """``finance_release`` authored as canonical component and binding mappings holding its Cards."""
    reviewer = reviewer or policy_judge("report-reviewer", version)
    controls = policy_judge("release-controls", version)
    return Service(
        "finance-release",
        version,
        service_type="api",
        components=[
            {
                "alias": "report-reviewer",
                "ref": reviewer,
                "verified_by": [{"verifier": report_policy(version, reviewer, policy)}],
            },
            {"alias": "release-controls", "ref": controls, "verified_by": [{"verifier": access}]},
        ],
    )


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


def policy_files(cards: Cards, version: str) -> set[str]:
    """The files registered policy ``version`` holds; none when it is unregistered."""
    try:
        files = cards.list_artifacts(RegisteredRef("Artifact", "company-sharing-policy", version))
    except WyrdError:
        return set()
    return {str(each["relative_path"]) for each in files}


def policy_published(cards: Cards, version: str) -> bool:
    """Whether policy ``version`` is registered and holds its extracted policy file."""
    return POLICY_FILE in policy_files(cards, version)


def cites_policy(report: VerificationResult) -> bool:
    """Whether ``report``'s passed policy verdict cites a line of the mounted policy."""
    [policy] = [each for each in report.requirements if each["id"] == "sharing-policy-satisfied"]
    return policy["outcome"] == "passed" and any(
        each["slot"].startswith("attachment_") and "lines" in each["locator"]
        for each in policy.get("citations", [])
    )


def mounted_policy(cards: Cards, version: str) -> str:
    """The policy version the registered ``report-policy`` ``version`` mounts."""
    spec = cards.get(RegisteredRef("Verifier", "report-policy", version)).spec
    check = spec["steps"][0]["requirements"][1]["check"]
    return check["workspace"]["/inputs/policy.json"]["artifact"]["card"]["version"]


def code(raised: pytest.ExceptionInfo[WyrdError]) -> str:
    """The stable code ``raised`` carries."""
    return raised.value.code


def test_multi_component_service_verifies_each_subject(
    wyrd_server: WyrdTestServer, cards: Cards, receiver: Receiver, monkeypatch: pytest.MonkeyPatch
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

    summarized = WyrdState.declare(
        finance_release("1.1.0", registered_policy("1.0.0"), summarized_access("1.1.0"))
    )
    try:
        registered = cards.get(RegisteredRef("Verifier", "release-access", "1.1.0")).spec
        assert registered["steps"][2]["depends_on"] == ["recipient", "destination"]
        public = permitted()
        public["destination"] = public["destination"].model_copy(update={"public_sharing": True})
        for request, releasable, verdict in (
            (permitted(), True, "passed"),
            (public, False, "failed"),
        ):
            access = judged(verify_both(summarized, request)[1])
            ran = [step["id"] for step in access.steps]
            assert set(ran) == {"recipient", "destination", "summary"}, ran
            assert ran[-1] == "summary", "the summary runs after both prerequisites"
            [summary] = [step for step in access.steps if step["id"] == "summary"]
            assert summary["status"] == "succeeded", summary
            assert summary["output"] == {
                "releasable": releasable,
                "recipient": {"authorized": True},
                "destination": {"appropriate": releasable},
            }, "the summary consumes both prerequisite outputs"
            assert outcome(access, "release-permitted") == verdict, access.requirements
            assert access.verdict == verdict
    finally:
        summarized.shutdown()

    verify_modular_service(monkeypatch)


def verify_modular_service(monkeypatch: pytest.MonkeyPatch) -> None:
    """The modular finance Service runs as its reader runs it, and each component verifies alone.

    ``main.py`` declares the imported definitions and passes both
    components. Declaring the same definitions again reuses their versions;
    each component's Verifier then runs its two assertions and its judge
    under that component's subject, refuses the other component's Verifier,
    and fails a report carrying an unfinished marker.
    """
    monkeypatch.syspath_prepend(str(SERVICE_EXAMPLE))
    monkeypatch.setattr(sys, "modules", dict(sys.modules))
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed):
        example = runpy.run_path(str(SERVICE_EXAMPLE / "main.py"), run_name="__main__")
    assert printed.getvalue() == "Report quality: passed\nRelease controls: passed\n"

    state = WyrdState.declare(
        Service(
            "finance-release",
            components=[
                Component(
                    "report-writer",
                    example["report_writer"],
                    verified_by=[example["report_quality"]],
                ),
                Component(
                    "release-reviewer",
                    example["release_reviewer"],
                    verified_by=[example["release_controls"]],
                ),
            ],
        )
    )
    try:
        report = example["Report"](
            text="Quarterly revenue was $120.", source="Current-quarter revenue: $120."
        )
        release = example["Release"](
            report=report.text,
            recipient_authorized=True,
            destination_private=True,
            purpose="Provide the quarterly revenue report to the appointed financial auditor.",
        )
        for alias, verifier, evidence, requirements, other in (
            (
                "report-writer",
                "report-quality",
                {"report": report},
                ["no-bank-account", "no-unfinished-text", "matches-source"],
                "release-controls",
            ),
            (
                "release-reviewer",
                "release-controls",
                {"release": release},
                ["authorized-recipient", "private-destination", "appropriate-purpose"],
                "report-quality",
            ),
        ):
            with state.run(alias) as run:
                result = run.observe.verify(verifier, **evidence)
                with pytest.raises(WyrdError) as unbound:
                    run.observe.verify(other, **evidence)
            assert result.subject == state.card_ref(alias), result.subject
            assert [(each["id"], each["outcome"]) for each in result.requirements] == [
                (id, "passed") for id in requirements
            ], result.requirements
            assert code(unbound) == "WYRD_SDK_404_UNKNOWN_VERIFIER"
        with state.run("report-writer") as run:
            unfinished = run.observe.verify(
                "report-quality", report=report.model_copy(update={"text": "TODO: totals."})
            )
        assert unfinished.verdict == "failed"
        assert outcome(unfinished, "no-unfinished-text") == "failed", unfinished.requirements
    finally:
        state.shutdown()


def draft_judge(version: str | None, asks: str) -> Agent:
    """``draft-judge`` at ``version`` asking ``asks``, as ``omitted/draft-judge.yaml`` declares it."""
    return Agent(
        "draft-judge",
        version,
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        prompt={
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": asks}],
            "response_format": "verdict",
        },
    )


def draft_writer(version: str | None, asks: str) -> Agent:
    """``draft-writer`` at ``version`` asking ``asks``, as ``omitted/draft-writer.yaml`` declares it."""
    return Agent(
        "draft-writer",
        version,
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        prompt={
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": asks}],
        },
    )


def draft_checks(judge: Agent) -> Verifier:
    """``draft-checks`` without a version, as ``omitted/draft-checks.yaml`` declares it, judged by ``judge``."""
    return Verifier(
        "draft-checks",
        inputs={
            "draft": {
                "type": "object",
                "required": ["text"],
                "properties": {"text": {"type": "string"}},
            }
        },
        requirements=[
            Requirement(
                "has-text",
                "The draft has text.",
                Check.assert_("inputs.draft.text", "is_non_empty", None),
            ),
            Requirement(
                "judged",
                "A judge accepts the draft.",
                Check.agent(judge, inputs={"draft": "inputs.draft"}),
            ),
        ],
    )


def drafting(judge: Agent) -> Service:
    """``drafting`` without a version, as ``omitted/drafting.yaml`` declares it, judged by ``judge``."""
    writer = draft_writer(None, "Draft release notes for {{change}}.")
    return Service(
        "drafting",
        service_type="api",
        components=[Component("writer", writer, verified_by=[draft_checks(judge)])],
    )


def drafting_mixed() -> Service:
    """``drafting-mixed`` as ``omitted/mixed.yaml`` declares it: a pinned writer, versionless checks."""
    writer = Agent(
        "mixed-writer",
        "1.0.0",
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        prompt={
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "Draft a changelog entry for {{change}}."}],
        },
    )
    checks = draft_checks(draft_judge(None, "Judge the draft {{draft}}."))
    return Service(
        "drafting-mixed",
        service_type="api",
        components=[Component("writer", writer, verified_by=[checks])],
    )


def declared(receipt: DeclarationReceipt, name: str) -> dict[str, Any]:
    """The registered reference ``receipt`` resolved for ``name``."""
    [reference] = [each.card_ref for each in receipt.outcomes if each.card_ref.name == name]
    return reference.to_dict()


def persisted(cards: Cards, receipt: DeclarationReceipt, name: str) -> dict[str, Any]:
    """The persisted spec of the Card ``receipt`` resolved for ``name``."""
    reference = declared(receipt, name)
    return cards.get(RegisteredRef(reference["kind"], reference["name"], reference["version"])).spec


def assert_bound(cards: Cards, receipt: DeclarationReceipt) -> None:
    """Every link of the ``drafting`` graph is its target's full registered reference."""
    for name in ("drafting", "draft-writer", "draft-checks", "draft-judge"):
        reference = declared(receipt, name)
        assert reference["version"] and reference["uid"], reference
    component = persisted(cards, receipt, "drafting")["components"][0]
    assert component["ref"] == declared(receipt, "draft-writer")
    assert component["verified_by"][0]["verifier"] == declared(receipt, "draft-checks")
    checks = persisted(cards, receipt, "draft-checks")
    assert checks["requirements"][1]["check"]["agent"] == declared(receipt, "draft-judge")


def test_omitted_versions_resolve_in_composed_definitions(cards: Cards) -> None:
    """A graph naming no version declares the same through its YAML path and its definitions.

    The server selects every version and binds each component, bound
    Verifier, and transitive judge to its full registered reference; a rerun
    reuses them. A mixed graph keeps its pinned writer and selects the rest,
    alike from YAML and definitions. A judge whose content changes without a
    version gets a newly selected version that gives its parents new
    versions while the writer keeps its reference; two explicit versions of
    one Agent are distinct targets; an omitted link naming two submissions
    is refused.
    """
    files = cards.declare_from_path(OMITTED / "drafting.yaml")
    assert_bound(cards, files)
    authored = drafting(draft_judge(None, "Judge the draft {{draft}}."))
    assert resolved(cards.declare(authored)) == resolved(files), "same identities and content"
    assert resolved(cards.declare(authored)) == resolved(files), "a rerun reuses them"

    mixed_files = cards.declare_from_path(OMITTED / "mixed.yaml")
    mixed = cards.declare(drafting_mixed())
    assert resolved(mixed) == resolved(mixed_files), "same identities and content"
    assert declared(mixed, "mixed-writer")["version"] == "1.0.0"
    component = persisted(cards, mixed, "drafting-mixed")["components"][0]
    assert component["ref"] == declared(mixed, "mixed-writer")
    assert component["verified_by"][0]["verifier"] == declared(files, "draft-checks")

    changed = cards.declare(drafting(draft_judge(None, "Judge the draft {{draft}} strictly.")))
    assert_bound(cards, changed)
    for name in ("draft-judge", "draft-checks", "drafting"):
        old, new = declared(files, name), declared(changed, name)
        assert old["version"] != new["version"] and old["uid"] != new["uid"], (old, new)
        assert resolved(files)[name][1] != resolved(changed)[name][1], name
    assert declared(changed, "draft-writer") == declared(files, "draft-writer")

    def pair(second: str | None) -> Service:
        return Service(
            "draft-pair",
            components=[
                Component("first", draft_writer("3.0.0", "First {{change}}.")),
                Component("second", draft_writer(second, "Second {{change}}.")),
            ],
        )

    both = persisted(cards, cards.declare(pair("4.0.0")), "draft-pair")
    assert [each["ref"]["version"] for each in both["components"]] == ["3.0.0", "4.0.0"]
    with pytest.raises(WyrdError) as ambiguous:
        cards.declare(pair(None))
    assert code(ambiguous) == "WYRD_LOADER_400_INVALID_ENVELOPE"
    [diagnostic] = ambiguous.value.details["diagnostics"]
    assert diagnostic["code"] == "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY", diagnostic


def wire_kinds(receipt: DeclarationReceipt) -> set[str]:
    """The wire names of the kinds ``receipt`` declared."""
    return {str(each.card_ref.to_dict()["kind"]) for each in receipt.outcomes}


def test_definitions_register_all_kinds(cards: Cards) -> None:
    """Definitions of every Card kind register through ``Cards.declare``.

    The spec-only kinds declare alone; a Service's Agent holds an inline
    original Prompt, registered as a Prompt Card the Agent references
    exactly, and binds a Verifier run by a Trigger that pages an Operator.
    """
    model = cards.declare(
        Model(
            "authored-model",
            interface={
                "kind": "Custom",
                "meta": {
                    "framework_version": "0.1.0",
                    "loader_module": "assistant",
                    "loader_class": "LatencyModel",
                    "extra": {},
                },
            },
            task_type="Other",
            signature={
                "inputs": [{"name": "latency", "dtype": "float64"}],
                "outputs": [{"name": "score", "dtype": "float64"}],
            },
        )
    )
    kinds = wire_kinds(model)
    for definition in (
        Data(
            "authored-data",
            interface={"kind": "Parquet", "meta": {"compression": "Snappy"}},
            schema={"columns": [{"name": "latency", "dtype": "float64"}]},
            stats={
                "row_count": 1,
                "col_count": 1,
                "byte_count": 1,
                "sha256": "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906",
            },
        ),
        Experiment("authored-experiment", experiment_type="ab").add_target_ref(
            CardRef.model(model.root.name, model.root.version)
        ),
        Mcp("authored-mcp", server_name="files", transport="stdio"),
        Audit("authored-audit", description="Quarterly review"),
        Artifact("authored-artifact", artifact_kind="document", content_type="application/pdf"),
    ):
        kinds |= wire_kinds(cards.declare(definition))

    checks = Verifier(
        "authored-checks",
        inputs={"latency": InputSlot.samples(["latency"])},
        requirements=[
            Requirement(
                "within-budget",
                "Mean latency stays at or below 100 ms.",
                Check.drift(
                    "inputs.latency",
                    {
                        "method": "Custom",
                        "signal": {"kind": "Metric", "name": "latency"},
                        "condition": {"kind": "Statistical"},
                        "profile": {
                            "kind": "Custom",
                            "metric_name": "latency",
                            "baseline_value": 0.0,
                            "alert_threshold": 100.0,
                        },
                    },
                ),
            )
        ],
    )
    binding = Binding(
        checks, runs_on=Trigger("authored-trigger", kind="schedule", cron="0 0 * * *")
    )
    binding.add_on_failure(
        Operator(
            "authored-operator",
            action={"kind": "http", "method": "post", "url": "https://hooks.example.com/release"},
        )
    )
    writer = Agent(
        "authored-agent",
        prompt=Prompt.openai_chat("gpt-4o", messages="Draft release notes for {{change}}."),
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
    )
    receipt = cards.declare(
        Service(
            "authored-service",
            service_type="api",
            components=[Component("writer", writer, verified_by=[binding])],
        )
    )
    kinds |= wire_kinds(receipt)
    [prompt] = [each.card_ref for each in receipt.outcomes if each.card_ref.kind == CardKind.Prompt]
    assert persisted(cards, receipt, "authored-agent")["prompt"] == prompt.to_dict()
    every = {"Data", "Model", "Artifact", "Experiment", "Prompt", "Agent", "Mcp", "Service"}
    assert kinds == every | {"Audit", "Verifier", "Trigger", "Operator"}


def test_local_artifacts_and_declaration_parity(
    wyrd_server: WyrdTestServer, cards: Cards, tmp_path: Path
) -> None:
    """Local Artifacts publish first through both entry points and match the file graph.

    A registered reference uploads nothing; a new local version leaves the
    old graph on the old policy; a graph refused after its policy published
    keeps that policy for reuse; changed bytes or content under a published
    version, an unreadable source, an unregistered reference, and a reader
    are refused with their stable codes; path and holder declaration still
    work; a local Artifact naming no version is linked by the reference its
    publication selected.
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
    canonical = canonical_release(
        "1.0.0", local_policy("1.0.0").file(POLICY_FILE), release_access("1.0.0")
    )
    assert resolved(cards.declare(canonical)) == resolved(files), (
        "canonical mappings declare the file graph"
    )

    from_code = WyrdState.declare(
        canonical_release("1.0.0", registered_policy("1.0.0"), release_access("1.0.0"))
    )
    from_file = WyrdState.declare(path=STORY / "finance-release.yaml", client=WyrdClient())
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

    unversioned = Artifact.from_path("unversioned-sharing-policy", POLICY).extract(True)
    cards.declare(finance_release("5.0.0", unversioned.file(POLICY_FILE), release_access("5.0.0")))
    spec = cards.get(RegisteredRef("Verifier", "report-policy", "5.0.0")).spec
    link = spec["steps"][0]["requirements"][1]["check"]["workspace"]["/inputs/policy.json"]
    publication = cards.get(cards.resolve_latest(CardKind.Artifact, "unversioned-sharing-policy"))
    assert link["artifact"]["card"] == publication.card_ref.to_dict(), (
        "the omitted-version policy is linked by its published reference"
    )
    assert publication.card_ref.version and publication.card_ref.uid, publication.card_ref

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

    directory = tmp_path / "policy-directory"
    (directory / "appendix").mkdir(parents=True)
    shutil.copy(POLICY, directory / POLICY.name)
    (directory / "appendix/retention.md").write_text("Retain release records for seven years.\n")
    from_directory = cards.declare(
        canonical_release(
            "6.0.0",
            local_policy("6.0.0", directory).file(POLICY_FILE),
            release_access("6.0.0"),
        )
    )
    assert CardKind.Artifact not in declared_kinds(from_directory), from_directory.outcomes
    assert {POLICY.name, POLICY_FILE, "appendix/retention.md"} <= policy_files(cards, "6.0.0")
    assert mounted_policy(cards, "6.0.0") == "6.0.0"
    directory_state = WyrdState.declare(
        finance_release(
            "7.0.0", local_policy("7.0.0", directory).file(POLICY_FILE), release_access("7.0.0")
        )
    )
    try:
        assert {POLICY.name, POLICY_FILE, "appendix/retention.md"} <= policy_files(cards, "7.0.0")
        assert mounted_policy(cards, "7.0.0") == "7.0.0"
        report, access = map(judged, verify_both(directory_state, permitted()))
        assert report.verifier.version == "7.0.0"
        assert cites_policy(report), report.requirements
        assert releases(report, access), (report.requirements, access.requirements)
    finally:
        directory_state.shutdown()

    with pytest.raises(WyrdError) as missing:
        WyrdState.declare(path=tmp_path / "missing.yaml", client=WyrdClient())
    assert code(missing) == "WYRD_LOADER_400_INVALID_ENVELOPE"
    assert "missing.yaml" in str(missing.value.details), "the keyword reaches the loader"

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
