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

``ChoiceTransform``, ``ProbabilityTransform``, ``ScoreTransform``, and their
``VerdictTransform`` union are Pydantic models, imported on first use, so
this module loads without the ``pydantic`` extra.
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
    "ChoiceTransform",
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
    "ProbabilityTransform",
    "Prompt",
    "Recorded",
    "Requirement",
    "ResponseFormat",
    "ScoreTransform",
    "Service",
    "Step",
    "Table",
    "Trigger",
    "VerdictTransform",
    "VerificationResult",
    "Verifier",
]

_TRANSFORMS = frozenset(
    {"ChoiceTransform", "ProbabilityTransform", "ScoreTransform", "VerdictTransform"}
)


def __getattr__(name: str) -> Any:
    """Import a Pydantic transform model on first use.

    Raises:
        ImportError: The ``pydantic`` extra is not installed.
        AttributeError: ``name`` is no member of this module.
    """
    if name in _TRANSFORMS:
        from . import _transforms

        return getattr(_transforms, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
