"""Card definitions, the evidence Verifiers accept, and the executions that judge it.

Construct a Card with ``Kind(name, version=None, **fields)``; an omitted
version is assigned at registration. Edit it with same-named field setters and
``add_*`` methods, then declare it::

    service = Service("finance-release")
    service.add_component(Component("reviewer", agent).verified_by([verifier]))
    state = WyrdState.declare(service)

Editing is local and never validates; ``validate()`` and declaration run the
shared Rust validation.

``File`` and ``ArtifactRef`` name media evidence for ``Observe.verify`` and
``Observe.submit``. ``submit`` returns an ``Execution`` for Run evidence, which
any process can rebuild from its ID and wait on::

    execution = state.run("agent").observe.submit("policy-check", answer=text)
    result = wyrd.verify.Execution.connect(execution.id).wait(60)

Window samples return ``Recorded``: the Verifier's schedule judges them later.
"""

from typing import Any

from .._wyrd.cards.prompt import Prompt, ResponseFormat
from .._wyrd.verify import (
    Action,
    Agent,
    AgentRunConfig,
    Artifact,
    ArtifactFile,
    ArtifactRef,
    Audit,
    Binding,
    CardRef,
    Check,
    CodeSource,
    ComparisonOperator,
    Component,
    Data,
    Evidence,
    Execution,
    ExecutionRoute,
    Experiment,
    File,
    InputSlot,
    Mcp,
    Model,
    Operator,
    Recorded,
    Requirement,
    Service,
    Step,
    Table,
    Trigger,
    VerificationResult,
    Verifier,
)

# Mirrors wyrd-client's `SLOT_KEYWORD`: the canonical slot a marked field declares.
_SLOT_KEYWORD = "x-wyrd-slot"


class Trace(str):
    """A trace slot's value: the trace ID of the work a Verifier judges.

    A Pydantic model field, or a Verifier ``inputs`` mapping value, of this
    type declares a ``trace`` slot; every other field declares a ``json``
    slot. A model field validates as a plain ``str`` and serializes as the
    bare ID, so one model both declares a Verifier's inputs and supplies them.
    """

    @classmethod
    def __get_pydantic_core_schema__(cls, source: Any, handler: Any) -> Any:
        return handler.generate_schema(str)

    @classmethod
    def __get_pydantic_json_schema__(cls, core_schema: Any, handler: Any) -> dict[str, Any]:
        return {**handler(core_schema), _SLOT_KEYWORD: {"type": "trace"}}

    @classmethod
    def model_json_schema(cls) -> dict[str, Any]:
        """The marked string schema a mapping value ``Trace`` stands for."""
        return {"type": "string", _SLOT_KEYWORD: {"type": "trace"}}


__all__ = [
    "Action",
    "Agent",
    "AgentRunConfig",
    "Artifact",
    "ArtifactFile",
    "ArtifactRef",
    "Audit",
    "Binding",
    "CardRef",
    "Check",
    "CodeSource",
    "ComparisonOperator",
    "Component",
    "Data",
    "Evidence",
    "Execution",
    "ExecutionRoute",
    "Experiment",
    "File",
    "InputSlot",
    "Mcp",
    "Model",
    "Operator",
    "Prompt",
    "Recorded",
    "Requirement",
    "ResponseFormat",
    "Service",
    "Step",
    "Table",
    "Trace",
    "Trigger",
    "VerificationResult",
    "Verifier",
]
