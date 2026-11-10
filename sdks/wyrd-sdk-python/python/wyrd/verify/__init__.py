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
    "CodeSource",
    "ComparisonOperator",
    "Component",
    "Data",
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
    "Trigger",
    "VerificationResult",
    "Verifier",
]
