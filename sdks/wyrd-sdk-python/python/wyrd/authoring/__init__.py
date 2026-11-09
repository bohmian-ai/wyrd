"""In-memory Card authoring in the Card YAML's own vocabulary.

Construct a Card with ``Kind(name, version, **fields)``, edit it with
same-named field setters and ``add_*`` methods, then declare it::

    service = Service("finance-release", "1.0.0")
    service.add_component(Component("reviewer", agent).verified_by([verifier]))
    receipt = Cards().declare(service)

Editing is local and never validates; ``validate()`` and declaration run the
shared Rust validation.
"""

from .._wyrd.authoring import (
    Action,
    Agent,
    Artifact,
    ArtifactFile,
    Audit,
    Binding,
    CardDefinition,
    CardRef,
    Check,
    CodeSource,
    Component,
    Data,
    Experiment,
    InputSlot,
    Mcp,
    Model,
    Operator,
    Prompt,
    Requirement,
    Service,
    Step,
    Trigger,
    Verifier,
)

__all__ = [
    "Action",
    "Agent",
    "Artifact",
    "ArtifactFile",
    "Audit",
    "Binding",
    "CardDefinition",
    "CardRef",
    "Check",
    "CodeSource",
    "Component",
    "Data",
    "Experiment",
    "InputSlot",
    "Mcp",
    "Model",
    "Operator",
    "Prompt",
    "Requirement",
    "Service",
    "Step",
    "Trigger",
    "Verifier",
]
