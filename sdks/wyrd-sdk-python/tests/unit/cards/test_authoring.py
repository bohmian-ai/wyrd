"""Offline Card definitions through public ``wyrd.verify``: construction, edits,
value capture, Python lifetimes, schema conveniences and every native family.

Nothing here reaches a server; validation is the shared Rust validation.
"""

import gc
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.cards import CardKind
from wyrd.cards import CardRef as RegisteredRef
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
    ComparisonOperator,
    Component,
    Data,
    ExecutionRoute,
    Experiment,
    InputSlot,
    Mcp,
    Model,
    Operator,
    Prompt,
    Requirement,
    ResponseFormat,
    Service,
    Step,
    Trigger,
    Verifier,
)

POLICY = Path(__file__).resolve().parents[5] / "fixtures/cards/card_authoring"
SHARING_POLICY = POLICY / "company-sharing-policy.pdf"


def ref(kind: str, name: str, version: str = "1.0.0") -> dict:
    return {"kind": kind, "name": name, "version": version}


def judge(name: str = "judge") -> Agent:
    return Agent(
        name,
        "1.0.0",
        prompt=Prompt.openai_chat(
            "gpt-4o", messages="Judge {{report}}", response_format=ResponseFormat.verdict()
        ),
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        tool_names=["fs.read"],
    )


def checked(id: str, depends_on: list[str] | None = None) -> Step:
    step = Step(id, Action.code(CodeSource.inline("def f(**_):\n    return {}\n"), "f"))
    for prerequisite in depends_on or []:
        step.add_depends_on(prerequisite)
    return step.add_requirement(
        Requirement(f"{id}-ok", "Fine", Check.assert_(f"steps.{id}.output.ok", "equals", True))
    )


def invalid(error: pytest.ExceptionInfo[WyrdError]) -> str:
    assert error.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"
    return " ".join(diagnostic["message"] for diagnostic in error.value.details["diagnostics"])


def test_definitions_edit_offline() -> None:
    verifier = Verifier("report-policy", "1.0.0", inputs={"report": {"type": "string"}})
    assert verifier.add_input("purpose", str) is verifier
    assert verifier.description("Report policy") is verifier

    step = checked("scan")
    verifier.add_step(step)
    step.add_depends_on("later").timeout_seconds(5)
    assert verifier.spec()["steps"][0] == {
        "id": "scan",
        "action": {
            "kind": "code",
            "source": {"inline": "def f(**_):\n    return {}\n"},
            "entrypoint": "f",
        },
        "requirements": [
            {
                "id": "scan-ok",
                "statement": "Fine",
                "check": {
                    "kind": "assert",
                    "path": "steps.scan.output.ok",
                    "operator": "equals",
                    "expected": True,
                },
            }
        ],
    }, "a child is captured as it was when passed"

    def build() -> Service:
        service = Service("finance-release", "1.0.0")
        component = Component("reviewer", judge("reviewer"))
        component.verified_by([verifier])
        return service.add_component(component)

    service = build()
    gc.collect()
    assert service.spec()["components"] == [
        {
            "alias": "reviewer",
            "ref": {"sibling": ref("Agent", "reviewer")},
            "verified_by": [{"verifier": {"sibling": ref("Verifier", "report-policy")}}],
        }
    ], "the parent keeps its children after their Python references are gone"
    service.validate()

    forward = Verifier("forward", "1.0.0", inputs={"report": str})
    forward.add_step(
        Step(
            "first",
            Action.code(CodeSource.inline("x"), "f").inputs({"later": "steps.second.output"}),
        )
    )
    forward.add_requirement(
        Requirement("done", "Done", Check.assert_("steps.first.output.ok", "equals", True))
    )
    with pytest.raises(WyrdError) as refused:
        forward.validate()
    assert "steps.second" in invalid(refused), "a forward reference is refused only at validation"
    forward.add_step(checked("second"))
    forward.validate()

    ordered = Verifier("ordered", "1.0.0", inputs={"report": str})
    ordered.steps([checked("a"), checked("b"), checked("c", ["a", "b"])])
    assert ordered.plan() == {
        "stages": [{"index": 0, "tasks": ["a", "b"]}, {"index": 1, "tasks": ["c"]}],
        "task_count": 3,
    }

    duplicated = Verifier("duplicated", "1.0.0")
    duplicated.add_input("report", str).add_input("report", int)
    duplicated.add_step(checked("scan")).add_step(checked("scan"))
    with pytest.raises(WyrdError) as refused:
        duplicated.validate()
    message = invalid(refused)
    assert "inputs.report: duplicate entry" in message
    assert duplicated.spec()["inputs"] == {"report": {"type": "json", "schema": {"type": "string"}}}
    assert [step["id"] for step in duplicated.spec()["steps"]] == ["scan", "scan"]

    replaced = Verifier("duplicated", "1.0.0", inputs={"report": str})
    replaced.inputs({"purpose": str})
    assert list(replaced.spec()["inputs"]) == ["purpose"], (
        "a whole-field setter replaces earlier additions"
    )

    with pytest.raises(TypeError, match="unexpected keyword argument 'stepz'"):
        Verifier("typo", "1.0.0", stepz=[])
    with pytest.raises(TypeError, match="takes a list"):
        Agent("listed", "1.0.0").tool_names("fs.read")
    assert not hasattr(Service("fixed", "1.0.0"), "name"), "name is constructor-only"
    assert not hasattr(Step("fixed", Action.extract("inputs.d")), "id")


def test_plain_schemas_do_not_require_pydantic() -> None:
    script = textwrap.dedent(
        """
        import sys
        sys.modules["pydantic"] = None
        from wyrd.verify import Check, InputSlot, Requirement, Verifier

        schema = {"type": "object", "properties": {"text": {"type": "string"}}}
        verifier = Verifier(
            "plain",
            "1.0.0",
            inputs={"text": str, "count": int, "ratio": float, "flag": bool, "nothing": type(None)},
        )
        verifier.add_input("report", schema)
        verifier.add_input("notes", {"type": ["string", "null"]})
        verifier.add_input("canonical", {"type": "json", "schema": {"type": "string"}})
        schema["properties"]["text"]["type"] = "integer"
        verifier.add_requirement(
            Requirement("has-text", "Has text", Check.assert_("inputs.text", "is_non_empty", None))
        )
        verifier.validate()
        inputs = verifier.spec()["inputs"]
        assert inputs["text"] == {"type": "json", "schema": {"type": "string"}}, inputs
        assert inputs["count"]["schema"] == {"type": "integer"}
        assert inputs["ratio"]["schema"] == {"type": "number"}
        assert inputs["flag"]["schema"] == {"type": "boolean"}
        assert inputs["nothing"]["schema"] == {"type": "null"}
        assert inputs["report"]["schema"]["properties"]["text"] == {"type": "string"}
        assert inputs["notes"] == {"type": "json", "schema": {"type": ["string", "null"]}}
        assert inputs["canonical"] == {"type": "json", "schema": {"type": "string"}}
        assert "pydantic" not in [name for name, module in sys.modules.items() if module is not None]
        try:
            Verifier("model", "1.0.0", inputs={"bad": object})
        except TypeError as error:
            assert "has no JSON Schema" in str(error)
        else:
            raise AssertionError("an unsupported class is a TypeError")
        """
    )
    completed = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True)
    assert completed.returncode == 0, completed.stderr


def test_slot_level_required_is_refused() -> None:
    for slot in [
        {"type": "json", "schema": {"type": "string"}, "required": False},
        {"type": "trace", "required": True},
    ]:
        verifier = Verifier("optional", "1.0.0", inputs={"note": slot})
        with pytest.raises(WyrdError) as refused:
            verifier.validate()
        message = invalid(refused)
        assert "inputs.note" in message and "unknown field `required`" in message, message
    assert not hasattr(InputSlot, "required")
    assert not hasattr(Action, "add_input")
    assert not hasattr(Check, "add_input")


class Report(BaseModel):
    text: str
    notes: str | None = None


def test_pydantic_models_supply_input_schemas() -> None:
    verifier = Verifier("modelled", "1.0.0", inputs={"report": Report})
    verifier.add_input("explicit", InputSlot.json(Report))
    inputs = verifier.spec()["inputs"]
    assert inputs["report"] == {"type": "json", "schema": Report.model_json_schema()}
    assert inputs["explicit"] == inputs["report"]
    assert "text" in inputs["report"]["schema"]["required"]
    assert "notes" not in inputs["report"]["schema"]["required"], "omission follows the model"
    assert {"type": "null"} in inputs["report"]["schema"]["properties"]["notes"]["anyOf"]


def test_schema_callback_may_edit_its_receiver() -> None:
    verifier = Verifier("reentrant", "1.0.0")

    class Edits:
        @classmethod
        def model_json_schema(cls) -> dict:
            verifier.description("set while converting")
            return {"type": "string"}

    assert verifier.inputs({"report": Edits}) is verifier
    assert verifier.spec()["inputs"] == {"report": {"type": "json", "schema": {"type": "string"}}}
    assert verifier.add_input("purpose", str) is verifier
    assert list(verifier.spec()["inputs"]) == ["report", "purpose"]


def test_boolean_schemas_are_json_schemas() -> None:
    accepting = Verifier("accepting", "1.0.0", inputs={"any": True, "none": False})
    accepting.inputs({"any": True, "none": False})
    accepting.add_input("open", True).add_input("closed", False)
    accepting.add_input("explicit-open", InputSlot.json(True))
    accepting.add_input("explicit-closed", InputSlot.json(False))
    assert accepting.spec()["inputs"] == {
        "any": {"type": "json", "schema": True},
        "none": {"type": "json", "schema": False},
        "open": {"type": "json", "schema": True},
        "closed": {"type": "json", "schema": False},
        "explicit-open": {"type": "json", "schema": True},
        "explicit-closed": {"type": "json", "schema": False},
    }


def test_every_native_family_constructs_canonical_specs() -> None:
    pager = {"kind": "http", "method": "post", "url": "https://hooks.example.com/release"}
    singles = [
        (
            Data("authored", "1.0.0")
            .interface({"kind": "Parquet", "meta": {"compression": "Snappy"}})
            .schema({"columns": [{"name": "latency", "dtype": "float64"}]})
            .stats(
                {
                    "row_count": 1,
                    "col_count": 1,
                    "byte_count": 1,
                    "sha256": "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906",
                }
            ),
            "Data",
        ),
        (
            Model(
                "authored",
                "1.0.0",
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
            ),
            "Model",
        ),
        (
            Experiment("authored", "1.0.0", experiment_type="ab").add_target_ref(
                CardRef.model("ranker", "1.0.0")
            ),
            "Experiment",
        ),
        (Mcp("authored", "1.0.0", server_name="files", transport="stdio").add_scope("read"), "Mcp"),
        (Audit("authored", "1.0.0", description="Quarterly review"), "Audit"),
        (Trigger("authored", "1.0.0", kind="schedule", cron="0 0 * * *"), "Trigger"),
        (Operator("authored", "1.0.0").action(pager), "Operator"),
        (
            Artifact("authored", "1.0.0", artifact_kind="document")
            .add_metadata("owner", {"type": "str", "value": "finance"})
            .add_label("team", "finance")
            .add_annotation("reviewed", "yes"),
            "Artifact",
        ),
        (judge(), "Agent"),
    ]
    for card, kind in singles:
        reference = card.card_ref()
        assert reference is not None and reference.kind == getattr(CardKind, kind)
        card.validate()
    assert singles[3][0].spec() == {
        "server_name": "files",
        "transport": "stdio",
        "scopes": ["read"],
    }
    assert singles[6][0].spec() == pager, "an Operator's action is flattened into its spec"
    assert singles[2][0].spec()["target_refs"] == [ref("Model", "ranker")]

    policy = CardRef.artifact("company-sharing-policy", "1.0.0").file("extracted/policy.json")
    local = Artifact.from_path("company-sharing-policy", "2.0.0", SHARING_POLICY).extract(True)
    assert isinstance(policy, ArtifactFile)
    local.validate()

    call = Action.agent(CardRef.agent("judge", "1.0.0"))
    call.bind_input("report", "inputs.report").add_workspace(
        "/inputs/report.json", "inputs.report"
    ).add_workspace("/inputs/policy.json", policy)
    check_call = (
        Check.agent(judge())
        .bind_input("report", "inputs.report")
        .workspace({"/inputs/policy.json": local.file("x.pdf")})
    )
    operators = [
        "equals",
        {"kind": "in_range", "min": 0.0, "max": 1.0, "inclusive": True},
        {"kind": "matches_regex", "pattern": "^r"},
    ]
    checks = [
        Check.code(CodeSource.inline("def f():\n    return {}\n"), "f", response_format="verdict"),
        check_call,
    ]
    for operator in operators:
        checks += [
            Check.assert_("inputs.report", operator, 1),
            Check.each("inputs.report.items[*]", operator, 1),
            Check.trace("inputs.trace", "$.spans[0].name", operator, 1),
        ]
    verifier = Verifier(
        "inventory",
        "1.0.0",
        inputs={
            "report": InputSlot.json({"type": "object"}),
            "document": InputSlot.media(["pdf"], max_bytes=1024),
            "trace": InputSlot.trace(),
        },
        steps=[
            Step("extract", Action.extract("inputs.document")),
            Step(
                "judge",
                call,
                when={
                    "path": "steps.extract.status",
                    "operator": "equals",
                    "expected": "succeeded",
                },
            ),
            Step("summarize", Action.code(policy, "summarize").runtime("python")),
            {"id": "canonical", "action": {"kind": "extract", "from": "inputs.document"}},
        ],
        requirements=[
            Requirement(f"r{index}", "Compared", check) for index, check in enumerate(checks)
        ],
        decide={"pass_rate": {"threshold": 0.5}},
        context_capture="hash",
    )
    spec = verifier.spec()
    assert spec["steps"][1]["action"]["workspace"]["/inputs/policy.json"] == {
        "artifact": {
            "card": ref("Artifact", "company-sharing-policy"),
            "path": "extracted/policy.json",
        }
    }
    assert (
        spec["steps"][1]["action"]["inputs"]
        == spec["requirements"][1]["check"]["inputs"]
        == {"report": "inputs.report"}
    )
    assert spec["requirements"][1]["check"]["agent"] == {"sibling": ref("Agent", "judge")}
    assert spec["requirements"][1]["check"]["workspace"]["/inputs/policy.json"]["artifact"][
        "card"
    ] == {"sibling": ref("Artifact", "company-sharing-policy", "2.0.0")}, (
        "a local Artifact stands as its sibling until it is published first"
    )
    assert spec["requirements"][3]["check"] == {
        "kind": "assert",
        "each": "inputs.report.items[*]",
        "operator": "equals",
        "expected": 1,
    }
    assert spec["steps"][3] == {
        "id": "canonical",
        "action": {"kind": "extract", "from": "inputs.document"},
    }
    verifier.validate()

    group = Verifier(
        "conversation",
        "1.0.0",
        inputs={
            "turns": InputSlot.source("verifiers.turn-check.inputs.turn"),
            "answered": "verifiers.turn-check.requirements.answered.outcome",
        },
        order_by="inputs.turns.turn",
        requirements=[
            Requirement(
                "answered", "Answered", Check.each("inputs.answered[*]", "equals", "passed")
            )
        ],
    )
    assert group.spec()["inputs"]["turns"] == "verifiers.turn-check.inputs.turn"
    group.validate()

    drift_spec = {
        "method": "Spc",
        "signal": {
            "kind": "Distribution",
            "baseline_ref": ref("Data", "latency-baseline"),
            "features": ["latency"],
        },
        "condition": {"kind": "Statistical"},
        "profile": {"kind": "Spc", "sample_size": 5},
    }
    drift = Verifier("latency-drift", "1.0.0", inputs={"latency": InputSlot.samples(["latency"])})
    drift.add_requirement(
        Requirement("stable", "Stable", Check.drift("inputs.latency", drift_spec))
    )
    assert drift.spec()["requirements"][0]["check"] == {
        "kind": "drift",
        "on": "inputs.latency",
        **drift_spec,
    }
    drift.validate()

    binding = Binding(
        Verifier("checked", "1.0.0"),
        runs_on=Trigger("daily", "1.0.0", kind="schedule", cron="0 0 * * *"),
    )
    binding.add_on_failure(Operator("page", "1.0.0", action=pager)).add_on_failure(pager)
    binding.add_on_verdict_change(CardRef.operator("notify", "1.0.0"))
    bound = judge("bound").add_verified_by(binding)
    assert bound.spec()["verified_by"] == [
        {
            "verifier": {"sibling": ref("Verifier", "checked")},
            "runs_on": {"sibling": ref("Trigger", "daily")},
            "on_failure": [{"sibling": ref("Operator", "page")}, pager],
            "on_verdict_change": [ref("Operator", "notify")],
        }
    ]

    registered = RegisteredRef(CardKind.Agent, "judge", "1.0.0")
    component = Component("judge", registered, source={"path": "services/judge", "kind": "python"})
    component.add_config("region", {"type": "str", "value": "eu"}).add_verified_by(
        CardRef.verifier("checked", "1.0.0")
    )
    service = Service(
        "authored",
        "1.0.0",
        description="Release service",
        components=[component, {"alias": "raw", "ref": ref("Agent", "raw")}],
        verified_by=["./not-a-file.yaml"],
    ).verified_by([CardRef.verifier("release", "1.0.0")])
    assert service.spec() == {
        "description": "Release service",
        "components": [
            {
                "alias": "judge",
                "ref": ref("Agent", "judge"),
                "source": {"path": "services/judge", "kind": "python"},
                "config": {"region": {"type": "str", "value": "eu"}},
                "verified_by": [{"verifier": ref("Verifier", "checked")}],
            },
            {"alias": "raw", "ref": ref("Agent", "raw")},
        ],
        "verified_by": [{"verifier": ref("Verifier", "release")}],
    }
    service.validate()

    file_source = CodeSource.file(POLICY / "missing.py")
    pending = Verifier("pending", "1.0.0", inputs={"report": str})
    pending.add_requirement(Requirement("coded", "Coded", Check.code(file_source, "f")))
    assert pending.spec()["requirements"][0]["check"]["source"] is None, (
        "a code file is read only at validation"
    )
    with pytest.raises(WyrdError) as refused:
        pending.validate()
    assert "missing.py" in str(refused.value.details)


def test_canonical_mappings_carry_native_values() -> None:
    verifier = Verifier("checked", "1.0.0", inputs={"report": str}).add_step(checked("scan"))
    canonical = judge("bound").verified_by([{"verifier": verifier}])
    fluent = judge("bound").verified_by([Binding(verifier)])
    assert canonical.spec() == fluent.spec()
    assert canonical.spec()["verified_by"] == [
        {"verifier": {"sibling": ref("Verifier", "checked")}}
    ]
    canonical.validate()
    other = Verifier("checked", "1.0.0", inputs={"report": int}).add_step(checked("scan"))
    conflicting = Component("other", judge("other").verified_by([Binding(other)]))

    def probe(agent: Agent) -> Service:
        return Service("probe", "1.0.0", components=[Component("bound", agent), conflicting])

    with pytest.raises(WyrdError) as refused:
        probe(canonical).validate()
    assert refused.value.code == "WYRD_SPEC_400_DUPLICATE_SUBMISSION", (
        "the mapping captured its Verifier as the fluent binding does"
    )
    # A plain sibling mapping captures nothing, so nothing conflicts.
    probe(judge("bound").verified_by(fluent.spec()["verified_by"])).validate()

    reviewer = judge()
    service = Service("canonical", "1.0.0", components=[{"alias": "judge", "ref": reviewer}])
    assert (
        service.spec()
        == Service("canonical", "1.0.0", components=[Component("judge", reviewer)]).spec()
        == {"components": [{"alias": "judge", "ref": {"sibling": ref("Agent", "judge")}}]}
    )
    service.validate()
    # A later edit to the child changes nothing already captured.
    reviewer.tool_names(["fs.write"])
    service.validate()
    with pytest.raises(WyrdError) as refused:
        service.add_component(Component("again", reviewer)).validate()
    assert refused.value.code == "WYRD_SPEC_400_DUPLICATE_SUBMISSION"

    replaced = Service(
        "replaced",
        "1.0.0",
        components=[{"alias": "judge", "ref": judge()}],
        verified_by=[{"verifier": verifier}],
    )
    # Replacing a field drops what its old value captured.
    replaced.components([{"alias": "judge", "ref": reviewer}])
    replaced.validate()
    with pytest.raises(WyrdError) as refused:
        replaced.add_component(conflicting).validate()
    assert refused.value.code == "WYRD_SPEC_400_DUPLICATE_SUBMISSION", (
        "another live field keeps its own capture"
    )
    with pytest.raises(WyrdError) as refused:
        Service("twice", "1.0.0").add_component({"alias": "judge", "ref": judge()}).add_component(
            {"alias": "judge", "ref": judge()}
        ).validate()
    assert "judge" in invalid(refused), "an incremental duplicate still refuses"

    drift_spec = {
        "method": "Spc",
        "signal": {"kind": "Distribution", "features": ["latency"]},
        "condition": {"kind": "Statistical"},
        "profile": {"kind": "Spc", "sample_size": 5},
    }
    for baseline in [
        CardRef.data("latency-baseline", "1.0.0"),
        RegisteredRef(CardKind.Data, "latency-baseline", "1.0.0"),
    ]:
        drift = Verifier(
            "latency-drift", "1.0.0", inputs={"latency": InputSlot.samples(["latency"])}
        )
        signal = {**drift_spec["signal"], "baseline_ref": baseline}
        drift.add_requirement(
            Requirement(
                "stable", "Stable", Check.drift("inputs.latency", {**drift_spec, "signal": signal})
            )
        )
        assert drift.spec()["requirements"][0]["check"]["signal"]["baseline_ref"] == ref(
            "Data", "latency-baseline"
        )
        drift.validate()

    local = Artifact.from_path("company-sharing-policy", "2.0.0", SHARING_POLICY)
    source = CodeSource.file(POLICY / "missing.py")
    nested = Step(
        "summarize",
        Action.code(CodeSource.inline("def f(**_):\n    return {}\n"), "f").add_workspace(
            "/inputs/policy.json", local.file("extracted/policy.json")
        ),
    )
    nested_verifier = Verifier("nested", "1.0.0", inputs={"report": str})
    nested_verifier.steps([nested])
    as_mapping = Verifier("nested", "1.0.0", inputs={"report": str})
    as_mapping.steps(
        [
            {
                "id": "summarize",
                "action": {
                    "kind": "code",
                    "source": CodeSource.inline("def f(**_):\n    return {}\n"),
                    "entrypoint": "f",
                    "workspace": {"/inputs/policy.json": local.file("extracted/policy.json")},
                },
            }
        ]
    )
    assert as_mapping.spec() == nested_verifier.spec()
    pending = Verifier("pending", "1.0.0", inputs={"report": str}).add_requirement(
        {
            "id": "coded",
            "statement": "Coded",
            "check": {"kind": "code", "source": source, "entrypoint": "f"},
        }
    )
    assert pending.spec()["requirements"][0]["check"]["source"] is None
    with pytest.raises(WyrdError) as refused:
        pending.validate()
    assert "missing.py" in str(refused.value.details), "a nested code file is read at validation"

    with pytest.raises(WyrdError) as refused:
        Service("keyed", "1.0.0").add_component({"alias": "judge", "ref": judge(), 1: "x"})  # ty: ignore[invalid-argument-type]
    assert refused.value.code == "WYRD_SPEC_400_VALIDATION"
    with pytest.raises(WyrdError) as refused:
        Service("keyed", "1.0.0").add_component({"alias": "judge", "config": {"x": float("nan")}})
    assert refused.value.code == "WYRD_SPEC_400_VALIDATION"


def test_circular_and_deep_containers_refuse_without_crashing() -> None:
    script = textwrap.dedent(
        """
        from wyrd import WyrdError
        from wyrd.verify import Action, Agent, Component, Service

        def refused(build):
            try:
                build()
            except WyrdError as error:
                assert error.code == "WYRD_SPEC_400_VALIDATION", error.code
                return True
            return False

        cyclic_dict = {}
        cyclic_dict["self"] = cyclic_dict
        cyclic_list = []
        cyclic_list.append(cyclic_list)
        indirect = {}
        indirect["items"] = [indirect]
        agent = Agent("judge", "1.0.0")
        for cycle in [cyclic_dict, cyclic_list, indirect]:
            assert refused(lambda: Action.agent(cycle))
            assert refused(lambda: Service("svc", "1.0.0").add_component({"alias": "judge", "ref": agent, "config": cycle}))
            assert refused(lambda: Component("judge", agent).verified_by([{"verifier": cycle}]))
        assert refused(lambda: Service("p", description=cyclic_dict))

        def nest(depth, container):
            value = "x"
            for _ in range(depth):
                value = container(value)
            return value

        for container in [lambda v: [v], lambda v: (v,), lambda v: {"k": v}]:
            for depth in [126, 127, 128, 4000]:
                value = nest(depth, container)
                control = refused(lambda: Service("p", description=value))
                assert control == (depth > 127), depth
                assert refused(lambda: Action.agent(value)) == control, depth
                component = {"alias": "judge", "ref": agent, "config": nest(depth - 1, container)}
                assert refused(lambda: Service("p", description={**component, "ref": "x"})) == control, depth
                assert refused(lambda: Service("svc", "1.0.0").add_component(component)) == control, depth
                binding = {"verifier": nest(depth - 1, container)}
                assert refused(lambda: Component("judge", agent).verified_by([binding])) == control, depth

        shared = {"threshold": 1}
        service = Service("svc", "1.0.0").add_component({"alias": "judge", "ref": agent, "config": {"a": shared, "b": [shared, shared]}})
        assert service.spec()["components"][0]["config"] == {"a": shared, "b": [shared, shared]}
        before = service.spec()
        assert refused(lambda: service.components([{"alias": "judge", "ref": agent, "config": cyclic_dict}]))
        assert service.spec() == before, "a refused conversion leaves the field as it was"
        incomplete = Service("svc", "1.0.0").add_component({"alias": "judge"})
        try:
            incomplete.validate()
        except WyrdError:
            pass
        else:
            raise AssertionError("an incomplete component is refused at validation")
        """
    )
    completed = subprocess.run(
        [sys.executable, "-c", script], capture_output=True, text=True, timeout=120
    )
    assert completed.returncode == 0, (completed.returncode, completed.stderr)


def test_canonical_mapping_callbacks_may_edit_their_receiver() -> None:
    verifier = Verifier("reentrant", "1.0.0")

    class Edits:
        def model_dump_json(self) -> str:
            verifier.description("set while converting")
            return '"inputs.report"'

    step = {"id": "read", "action": {"kind": "extract", "from": Edits()}}
    assert verifier.steps([step]) is verifier  # ty: ignore[invalid-argument-type]
    assert verifier.spec()["steps"] == [
        {"id": "read", "action": {"kind": "extract", "from": "inputs.report"}}
    ]


def test_unversioned_definitions_take_the_original_prompt_and_canonical_values() -> None:
    class Item(BaseModel):
        sku: str

    class Release(BaseModel):
        report: str
        recipient_authorized: bool
        items: list[Item]

    class Inputs(BaseModel):
        release: Release

    prompt = Prompt.openai_chat(
        "gpt-4o", messages="Judge {{release}}", response_format=ResponseFormat.verdict()
    )
    judge = Agent(
        "release-judge",
        prompt=prompt,
        execution_route=ExecutionRoute.wyrd_gateway(deployment="openai"),
        run_config=AgentRunConfig(max_iterations=3),
        tool_names=["fs.read"],
    )
    assert judge.card_ref() is None
    assert judge.spec()["execution_route"] == {"kind": "wyrd_gateway", "deployment": "openai"}
    assert judge.spec()["run_config"] == {"max_iterations": 3}, "unset fields stay absent"
    assert Agent("native", execution_route=ExecutionRoute.Native).spec()["execution_route"] == {
        "kind": "native"
    }
    with pytest.raises(TypeError):
        Agent("typed", tool_names=[1])  # ty: ignore[invalid-argument-type]

    assert Verifier("v", inputs=Inputs).spec() == Verifier("v", inputs={"release": Release}).spec()
    slot = Verifier("v", inputs=Inputs).spec()["inputs"]["release"]
    assert slot["schema"]["$defs"]["Item"]["properties"]["sku"] == {
        "title": "Sku",
        "type": "string",
    }

    checks = {
        "equals": Check.assert_(
            "inputs.release.recipient_authorized", ComparisonOperator.Equals, True
        ),
        "range": Check.assert_(
            "inputs.release.report",
            ComparisonOperator.in_range(min=0.0, max=1.0, inclusive=True),
            None,
        ),
        "regex": Check.each(
            "inputs.release.items[*].sku", ComparisonOperator.matches_regex(pattern="^S-"), None
        ),
    }
    verifier = Verifier(
        "release-controls",
        inputs=Inputs,
        requirements=[Requirement(name, name, check) for name, check in checks.items()]
        + [
            Requirement(
                "judged", "Judged", Check.agent(judge, inputs={"release": "inputs.release"})
            )
        ],
    )
    authored = [requirement["check"] for requirement in verifier.spec()["requirements"]]
    assert [check.get("operator") for check in authored[:3]] == [
        "equals",
        {"kind": "in_range", "min": 0.0, "max": 1.0, "inclusive": True},
        {"kind": "matches_regex", "pattern": "^S-"},
    ]
    assert authored[3]["agent"] == {"sibling": {"kind": "Agent", "name": "release-judge"}}
    Service(
        "finance-release", components=[Component("judge", judge, verified_by=[verifier])]
    ).validate()
