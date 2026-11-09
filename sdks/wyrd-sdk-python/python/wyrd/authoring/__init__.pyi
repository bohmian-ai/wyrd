"""In-memory Card authoring in the Card YAML's own vocabulary.

Each named Card is ``Kind(name, version, **fields)``; each sub-value takes the
fields that identify it. Every other method is named for the YAML field it
edits and takes that field's YAML value: references stay strings such as
``inputs.report.text``, operators stay ``"equals"`` or their object form, and
nested values stay canonical mappings. A setter such as
``Service.verified_by`` replaces the whole field; an add method such as
``Service.add_verified_by`` appends one entry. Both edit the receiver and
return it.

Construction and editing never validate, read files, or touch the network.
Passing a child copies its current value, so later edits to the child change
nothing already passed. ``validate()``, ``Cards.declare`` and
``WyrdState.declare`` run the shared Rust validation; declaration publishes
each local Artifact first, then the graph::

    verifier = Verifier("report-policy", "1.0.0", inputs={"report": Report})
    service = Service("finance-release", "1.0.0")
    service.add_component(Component("reviewer", agent).verified_by([verifier]))
    state = WyrdState.declare(service)

An input slot also accepts ``str``, ``int``, ``float``, ``bool`` and ``None``'s
type, a class with ``model_json_schema()`` such as a Pydantic model, or a bare
JSON Schema; Pydantic is never imported for you.
"""

import os
import pathlib
from collections.abc import Iterable, Mapping, Sequence
from typing import Any, Self, TypeAlias, TypedDict, Unpack

from .. import cards
from ..cards import CardKind, JsonScalar

JsonInput: TypeAlias = JsonScalar | Sequence[JsonInput] | Mapping[str, JsonInput]
"""Any JSON value as authored: nested mappings and sequences of JSON values."""

PathLike: TypeAlias = str | os.PathLike[str] | pathlib.Path

AnyCard: TypeAlias = (
    Service
    | Agent
    | Prompt
    | Verifier
    | Artifact
    | Model
    | Data
    | Experiment
    | Mcp
    | Audit
    | Trigger
    | Operator
    | CardDefinition
)
"""Any authored Card; at a reference position it becomes a sibling in the same graph."""

FieldInput: TypeAlias = (
    AnyCard
    | CardRef
    | cards.CardRef
    | ArtifactFile
    | CodeSource
    | Component
    | Binding
    | Step
    | Requirement
    | InputSlot
    | Action
    | Check
    | JsonInput
)
"""A value at a reference position: a Card, an exact reference, an authoring value, or JSON."""

BindingInput: TypeAlias = (
    Binding | Verifier | CardRef | cards.CardRef | str | Mapping[str, JsonInput]
)
"""A binding, or the Verifier, exact reference or YAML path it binds."""

SlotInput: TypeAlias = InputSlot | str | type | Mapping[str, JsonInput]
"""An input slot: a slot, a source reference, a class standing for a schema, or a mapping.

A mapping whose ``type`` is ``json``, ``media``, ``trace`` or ``samples`` is a
canonical slot; any other mapping is the JSON Schema of a ``json`` slot, never an
example value to infer one from.
"""

class CardRef:
    """An exact Card reference, kept as authored until validation.

    At a reference position it stays external, even when the same graph
    defines that identity, and nothing is uploaded for it.
    """

    @property
    def kind(self) -> CardKind:
        """The referenced kind."""
        ...

    @property
    def name(self) -> str:
        """The referenced name, as authored."""
        ...

    @property
    def version(self) -> str:
        """The referenced version, as authored."""
        ...

    def __init__(self, kind: CardKind | str, name: str, version: str) -> None:
        """The exact reference to the ``kind`` Card ``name`` at ``version``.

        Args:
            kind: The kind, as a ``CardKind`` or its wire name.
            name: The Card name.
            version: The exact Card version.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown kind.
        """
        ...

    @staticmethod
    def data(name: str, version: str) -> CardRef:
        """The exact reference to the Data Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def model(name: str, version: str) -> CardRef:
        """The exact reference to the Model Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def artifact(name: str, version: str) -> CardRef:
        """The exact reference to the Artifact Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def experiment(name: str, version: str) -> CardRef:
        """The exact reference to the Experiment Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def prompt(name: str, version: str) -> CardRef:
        """The exact reference to the Prompt Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def agent(name: str, version: str) -> CardRef:
        """The exact reference to the Agent Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def mcp(name: str, version: str) -> CardRef:
        """The exact reference to the Mcp Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def service(name: str, version: str) -> CardRef:
        """The exact reference to the Service Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def audit(name: str, version: str) -> CardRef:
        """The exact reference to the Audit Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def verifier(name: str, version: str) -> CardRef:
        """The exact reference to the Verifier Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def trigger(name: str, version: str) -> CardRef:
        """The exact reference to the Trigger Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    @staticmethod
    def operator(name: str, version: str) -> CardRef:
        """The exact reference to the Operator Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version.
        """
        ...

    def file(self, path: str) -> ArtifactFile:
        """Select the fixed file or directory prefix ``path`` of this registered Artifact.

        Nothing is uploaded.

        Args:
            path: The file or directory prefix within the Artifact.
        """
        ...

class ServiceFields(TypedDict, total=False):
    """The ``Service`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    service_type: JsonInput
    components: Sequence[Component | Mapping[str, JsonInput]]
    verified_by: Sequence[BindingInput]
    tables: Sequence[JsonInput]
    entry_point: JsonInput
    deployment: Mapping[str, JsonInput]
    runtime: JsonInput
    service_config: Mapping[str, JsonInput]
    credential_refs: Sequence[JsonInput]
    content_hash: JsonInput
    lock_hash: JsonInput
    metadata: Mapping[str, JsonInput]

class Service:
    """A Service Card: a running system, its components, and the Verifiers that check it."""

    def __init__(self, name: str, version: str, **fields: Unpack[ServiceFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def service_type(self, value: JsonInput) -> Self:
        """Set ``service_type``, replacing its earlier value; returns this value.

        Args:
            value: The new ``service_type``.
        """
        ...

    def components(self, items: Iterable[Component | Mapping[str, JsonInput]]) -> Self:
        """Set the whole ``components`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``components`` entries.
        """
        ...

    def add_component(self, item: Component | Mapping[str, JsonInput]) -> Self:
        """Append one entry to ``components``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def verified_by(self, items: Iterable[BindingInput]) -> Self:
        """Set the whole ``verified_by`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``verified_by`` entries.
        """
        ...

    def add_verified_by(self, item: BindingInput) -> Self:
        """Append one entry to ``verified_by``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def tables(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``tables`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``tables`` entries.
        """
        ...

    def add_table(self, item: JsonInput) -> Self:
        """Append one entry to ``tables``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def entry_point(self, value: JsonInput) -> Self:
        """Set ``entry_point``, replacing its earlier value; returns this value.

        Args:
            value: The new ``entry_point``.
        """
        ...

    def deployment(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``deployment`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``deployment`` entries.
        """
        ...

    def add_deployment(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``deployment``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def runtime(self, value: JsonInput) -> Self:
        """Set ``runtime``, replacing its earlier value; returns this value.

        Args:
            value: The new ``runtime``.
        """
        ...

    def service_config(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``service_config`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``service_config`` entries.
        """
        ...

    def add_service_config(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``service_config``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def credential_refs(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``credential_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``credential_refs`` entries.
        """
        ...

    def add_credential_ref(self, item: JsonInput) -> Self:
        """Append one entry to ``credential_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def content_hash(self, value: JsonInput) -> Self:
        """Set ``content_hash``, replacing its earlier value; returns this value.

        Args:
            value: The new ``content_hash``.
        """
        ...

    def lock_hash(self, value: JsonInput) -> Self:
        """Set ``lock_hash``, replacing its earlier value; returns this value.

        Args:
            value: The new ``lock_hash``.
        """
        ...

    def metadata(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``metadata`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``metadata`` entries.
        """
        ...

    def add_metadata(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``metadata``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class AgentFields(TypedDict, total=False):
    """The ``Agent`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    prompt: FieldInput
    execution_route: JsonInput
    tool_names: Sequence[JsonInput]
    run_config: JsonInput
    verified_by: Sequence[BindingInput]

class Agent:
    """An Agent Card: a prompt run through an execution route with tools."""

    def __init__(self, name: str, version: str, **fields: Unpack[AgentFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def prompt(self, value: FieldInput) -> Self:
        """Set ``prompt``, replacing its earlier value; returns this value.

        Args:
            value: The new ``prompt``.
        """
        ...

    def execution_route(self, value: JsonInput) -> Self:
        """Set ``execution_route``, replacing its earlier value; returns this value.

        Args:
            value: The new ``execution_route``.
        """
        ...

    def tool_names(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``tool_names`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``tool_names`` entries.
        """
        ...

    def add_tool_name(self, item: JsonInput) -> Self:
        """Append one entry to ``tool_names``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def run_config(self, value: JsonInput) -> Self:
        """Set ``run_config``, replacing its earlier value; returns this value.

        Args:
            value: The new ``run_config``.
        """
        ...

    def verified_by(self, items: Iterable[BindingInput]) -> Self:
        """Set the whole ``verified_by`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``verified_by`` entries.
        """
        ...

    def add_verified_by(self, item: BindingInput) -> Self:
        """Append one entry to ``verified_by``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

class PromptFields(TypedDict, total=False):
    """The ``Prompt`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    provider: JsonInput
    model: JsonInput
    system: JsonInput
    messages: Sequence[JsonInput]
    settings: JsonInput
    response_format: JsonInput
    format: JsonInput
    body: Mapping[str, JsonInput]

class Prompt:
    """A Prompt Card: provider, model, and the authored prompt."""

    def __init__(self, name: str, version: str, **fields: Unpack[PromptFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def provider(self, value: JsonInput) -> Self:
        """Set ``provider``, replacing its earlier value; returns this value.

        Args:
            value: The new ``provider``.
        """
        ...

    def model(self, value: JsonInput) -> Self:
        """Set ``model``, replacing its earlier value; returns this value.

        Args:
            value: The new ``model``.
        """
        ...

    def system(self, value: JsonInput) -> Self:
        """Set ``system``, replacing its earlier value; returns this value.

        Args:
            value: The new ``system``.
        """
        ...

    def messages(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``messages`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``messages`` entries.
        """
        ...

    def add_message(self, item: JsonInput) -> Self:
        """Append one entry to ``messages``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def settings(self, value: JsonInput) -> Self:
        """Set ``settings``, replacing its earlier value; returns this value.

        Args:
            value: The new ``settings``.
        """
        ...

    def response_format(self, value: JsonInput) -> Self:
        """Set ``response_format``, replacing its earlier value; returns this value.

        Args:
            value: The new ``response_format``.
        """
        ...

    def format(self, value: JsonInput) -> Self:
        """Set ``format``, replacing its earlier value; returns this value.

        Args:
            value: The new ``format``.
        """
        ...

    def body(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``body`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``body`` entries.
        """
        ...

    def add_body(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``body``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class VerifierFields(TypedDict, total=False):
    """The ``Verifier`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    inputs: Mapping[str, SlotInput]
    steps: Sequence[Step | Mapping[str, JsonInput]]
    requirements: Sequence[Requirement | Mapping[str, JsonInput]]
    decide: JsonInput
    context_capture: JsonInput
    order_by: JsonInput

class Verifier:
    """A Verifier Card: the evidence it accepts, the steps that compute over it, and the
    requirements that judge it."""

    def __init__(self, name: str, version: str, **fields: Unpack[VerifierFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def plan(self) -> dict[str, Any]:
        """The step stages the server would execute, as ``{stages, task_count}``.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` for a field that does
                not decode, or the shared Verifier validation's refusal.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def inputs(self, entries: Mapping[str, SlotInput]) -> Self:
        """Set the whole ``inputs`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``inputs`` entries.
        """
        ...

    def add_input(self, key: str, value: SlotInput) -> Self:
        """Declare the evidence slot ``key`` in ``inputs``; returns this value.

        Args:
            key: The slot's name.
            value: The slot, or a JSON Schema or class standing for one.
        """
        ...

    def steps(self, items: Iterable[Step | Mapping[str, JsonInput]]) -> Self:
        """Set the whole ``steps`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``steps`` entries.
        """
        ...

    def add_step(self, item: Step | Mapping[str, JsonInput]) -> Self:
        """Append one entry to ``steps``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def requirements(self, items: Iterable[Requirement | Mapping[str, JsonInput]]) -> Self:
        """Set the whole ``requirements`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``requirements`` entries.
        """
        ...

    def add_requirement(self, item: Requirement | Mapping[str, JsonInput]) -> Self:
        """Append one entry to ``requirements``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def decide(self, value: JsonInput) -> Self:
        """Set ``decide``, replacing its earlier value; returns this value.

        Args:
            value: The new ``decide``.
        """
        ...

    def context_capture(self, value: JsonInput) -> Self:
        """Set ``context_capture``, replacing its earlier value; returns this value.

        Args:
            value: The new ``context_capture``.
        """
        ...

    def order_by(self, value: JsonInput) -> Self:
        """Set ``order_by``, replacing its earlier value; returns this value.

        Args:
            value: The new ``order_by``.
        """
        ...

class ArtifactFields(TypedDict, total=False):
    """The ``Artifact`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    artifact_kind: JsonInput
    artifact_uris: Sequence[JsonInput]
    content_type: JsonInput
    size_bytes: JsonInput
    integrity: JsonInput
    schema_ref: FieldInput
    framework_adapter: JsonInput
    external_uri: JsonInput
    metadata: Mapping[str, JsonInput]

class Artifact:
    """An Artifact Card: a local file or directory published before the graph that
    references it, or an Artifact defined only by its spec."""

    def __init__(self, name: str, version: str, **fields: Unpack[ArtifactFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    @staticmethod
    def from_path(
        name: str, version: str, path: PathLike, **fields: Unpack[ArtifactFields]
    ) -> Artifact:
        """The ``file`` Artifact published from ``path`` before its graph.

        Nothing is read until it is published.

        Args:
            name: The Artifact name.
            version: The exact Artifact version.
            path: The local file or directory to publish.
            **fields: Further ``Artifact`` spec fields.
        """
        ...

    def extract(self, extract: bool) -> Self:
        """Set whether publication extracts supported documents; returns this Artifact.

        Args:
            extract: Whether publication extracts supported documents.
        """
        ...

    def file(self, path: str) -> ArtifactFile:
        """Select the fixed file or directory prefix ``path`` of this Artifact.

        Args:
            path: The file or directory prefix within the Artifact.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def artifact_kind(self, value: JsonInput) -> Self:
        """Set ``artifact_kind``, replacing its earlier value; returns this value.

        Args:
            value: The new ``artifact_kind``.
        """
        ...

    def artifact_uris(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``artifact_uris`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``artifact_uris`` entries.
        """
        ...

    def add_artifact_uri(self, item: JsonInput) -> Self:
        """Append one entry to ``artifact_uris``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def content_type(self, value: JsonInput) -> Self:
        """Set ``content_type``, replacing its earlier value; returns this value.

        Args:
            value: The new ``content_type``.
        """
        ...

    def size_bytes(self, value: JsonInput) -> Self:
        """Set ``size_bytes``, replacing its earlier value; returns this value.

        Args:
            value: The new ``size_bytes``.
        """
        ...

    def integrity(self, value: JsonInput) -> Self:
        """Set ``integrity``, replacing its earlier value; returns this value.

        Args:
            value: The new ``integrity``.
        """
        ...

    def schema_ref(self, value: FieldInput) -> Self:
        """Set ``schema_ref``, replacing its earlier value; returns this value.

        Args:
            value: The new ``schema_ref``.
        """
        ...

    def framework_adapter(self, value: JsonInput) -> Self:
        """Set ``framework_adapter``, replacing its earlier value; returns this value.

        Args:
            value: The new ``framework_adapter``.
        """
        ...

    def external_uri(self, value: JsonInput) -> Self:
        """Set ``external_uri``, replacing its earlier value; returns this value.

        Args:
            value: The new ``external_uri``.
        """
        ...

    def metadata(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``metadata`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``metadata`` entries.
        """
        ...

    def add_metadata(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``metadata``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class ModelFields(TypedDict, total=False):
    """The ``Model`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    interface: JsonInput
    task_type: JsonInput
    signature: JsonInput
    sample_input: JsonInput
    card_refs: Sequence[FieldInput]

class Model:
    """A Model Card: a trained model's interface, task, and signature."""

    def __init__(self, name: str, version: str, **fields: Unpack[ModelFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def interface(self, value: JsonInput) -> Self:
        """Set ``interface``, replacing its earlier value; returns this value.

        Args:
            value: The new ``interface``.
        """
        ...

    def task_type(self, value: JsonInput) -> Self:
        """Set ``task_type``, replacing its earlier value; returns this value.

        Args:
            value: The new ``task_type``.
        """
        ...

    def signature(self, value: JsonInput) -> Self:
        """Set ``signature``, replacing its earlier value; returns this value.

        Args:
            value: The new ``signature``.
        """
        ...

    def sample_input(self, value: JsonInput) -> Self:
        """Set ``sample_input``, replacing its earlier value; returns this value.

        Args:
            value: The new ``sample_input``.
        """
        ...

    def card_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``card_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``card_refs`` entries.
        """
        ...

    def add_card_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``card_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

class DataFields(TypedDict, total=False):
    """The ``Data`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    interface: JsonInput
    schema: JsonInput
    card_refs: Sequence[FieldInput]
    splits: Mapping[str, FieldInput]
    target_columns: Sequence[JsonInput]
    sql: JsonInput
    stats: JsonInput

class Data:
    """A Data Card: a dataset's interface, schema, splits, and target columns."""

    def __init__(self, name: str, version: str, **fields: Unpack[DataFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def interface(self, value: JsonInput) -> Self:
        """Set ``interface``, replacing its earlier value; returns this value.

        Args:
            value: The new ``interface``.
        """
        ...

    def schema(self, value: JsonInput) -> Self:
        """Set ``schema``, replacing its earlier value; returns this value.

        Args:
            value: The new ``schema``.
        """
        ...

    def card_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``card_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``card_refs`` entries.
        """
        ...

    def add_card_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``card_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def splits(self, entries: Mapping[str, FieldInput]) -> Self:
        """Set the whole ``splits`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``splits`` entries.
        """
        ...

    def add_split(self, key: str, value: FieldInput) -> Self:
        """Insert the entry ``key`` into ``splits``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def target_columns(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``target_columns`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``target_columns`` entries.
        """
        ...

    def add_target_column(self, item: JsonInput) -> Self:
        """Append one entry to ``target_columns``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def sql(self, value: JsonInput) -> Self:
        """Set ``sql``, replacing its earlier value; returns this value.

        Args:
            value: The new ``sql``.
        """
        ...

    def stats(self, value: JsonInput) -> Self:
        """Set ``stats``, replacing its earlier value; returns this value.

        Args:
            value: The new ``stats``.
        """
        ...

class ExperimentFields(TypedDict, total=False):
    """The ``Experiment`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    experiment_type: JsonInput
    description: JsonInput
    target_refs: Sequence[FieldInput]
    default_parameters: Mapping[str, JsonInput]
    run_refs: Sequence[JsonInput]
    summary_metrics: Sequence[JsonInput]
    best_run_ref: JsonInput
    card_refs: Sequence[FieldInput]
    details: Mapping[str, JsonInput]

class Experiment:
    """An Experiment Card: the targets and parameters of a set of runs, with their summary
    metrics and best run."""

    def __init__(self, name: str, version: str, **fields: Unpack[ExperimentFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def experiment_type(self, value: JsonInput) -> Self:
        """Set ``experiment_type``, replacing its earlier value; returns this value.

        Args:
            value: The new ``experiment_type``.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def target_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``target_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``target_refs`` entries.
        """
        ...

    def add_target_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``target_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def default_parameters(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``default_parameters`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``default_parameters`` entries.
        """
        ...

    def add_default_parameter(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``default_parameters``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def run_refs(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``run_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``run_refs`` entries.
        """
        ...

    def add_run_ref(self, item: JsonInput) -> Self:
        """Append one entry to ``run_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def summary_metrics(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``summary_metrics`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``summary_metrics`` entries.
        """
        ...

    def add_summary_metric(self, item: JsonInput) -> Self:
        """Append one entry to ``summary_metrics``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def best_run_ref(self, value: JsonInput) -> Self:
        """Set ``best_run_ref``, replacing its earlier value; returns this value.

        Args:
            value: The new ``best_run_ref``.
        """
        ...

    def card_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``card_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``card_refs`` entries.
        """
        ...

    def add_card_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``card_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def details(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``details`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``details`` entries.
        """
        ...

    def add_detail(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``details``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class McpFields(TypedDict, total=False):
    """The ``Mcp`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    server_name: JsonInput
    transport: JsonInput
    tool_refs: Sequence[FieldInput]
    scopes: Sequence[JsonInput]
    details: Mapping[str, JsonInput]

class Mcp:
    """An Mcp Card: an MCP server and the tools it exposes."""

    def __init__(self, name: str, version: str, **fields: Unpack[McpFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def server_name(self, value: JsonInput) -> Self:
        """Set ``server_name``, replacing its earlier value; returns this value.

        Args:
            value: The new ``server_name``.
        """
        ...

    def transport(self, value: JsonInput) -> Self:
        """Set ``transport``, replacing its earlier value; returns this value.

        Args:
            value: The new ``transport``.
        """
        ...

    def tool_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``tool_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``tool_refs`` entries.
        """
        ...

    def add_tool_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``tool_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def scopes(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``scopes`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``scopes`` entries.
        """
        ...

    def add_scope(self, item: JsonInput) -> Self:
        """Append one entry to ``scopes``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def details(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``details`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``details`` entries.
        """
        ...

    def add_detail(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``details``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class AuditFields(TypedDict, total=False):
    """The ``Audit`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    subject_refs: Sequence[FieldInput]
    policy_refs: Sequence[FieldInput]
    evidence_refs: Sequence[FieldInput]
    details: Mapping[str, JsonInput]

class Audit:
    """An Audit Card: subjects, policies, and evidence."""

    def __init__(self, name: str, version: str, **fields: Unpack[AuditFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def subject_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``subject_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``subject_refs`` entries.
        """
        ...

    def add_subject_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``subject_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def policy_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``policy_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``policy_refs`` entries.
        """
        ...

    def add_policy_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``policy_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def evidence_refs(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``evidence_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``evidence_refs`` entries.
        """
        ...

    def add_evidence_ref(self, item: FieldInput) -> Self:
        """Append one entry to ``evidence_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def details(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``details`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``details`` entries.
        """
        ...

    def add_detail(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``details``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

class TriggerFields(TypedDict, total=False):
    """The ``Trigger`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    kind: JsonInput
    cron: JsonInput
    tz: JsonInput
    after: JsonInput

class Trigger:
    """A Trigger Card: when a binding's Verifier runs. ``kind`` is ``schedule`` with
    ``cron`` and optional ``tz``, or ``group_end`` with ``after``."""

    def __init__(self, name: str, version: str, **fields: Unpack[TriggerFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def kind(self, value: JsonInput) -> Self:
        """Set ``kind``, replacing its earlier value; returns this value.

        Args:
            value: The new ``kind``.
        """
        ...

    def cron(self, value: JsonInput) -> Self:
        """Set ``cron``, replacing its earlier value; returns this value.

        Args:
            value: The new ``cron``.
        """
        ...

    def tz(self, value: JsonInput) -> Self:
        """Set ``tz``, replacing its earlier value; returns this value.

        Args:
            value: The new ``tz``.
        """
        ...

    def after(self, value: JsonInput) -> Self:
        """Set ``after``, replacing its earlier value; returns this value.

        Args:
            value: The new ``after``.
        """
        ...

class OperatorFields(TypedDict, total=False):
    """The ``Operator`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    action: JsonInput
    budget: JsonInput

class Operator:
    """An Operator Card: what runs when a binding's Verifier fails or changes verdict."""

    def __init__(self, name: str, version: str, **fields: Unpack[OperatorFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored so far, before validation.

        A ``CodeSource.file`` position is ``None`` until validation reads it.
        """
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Nothing is registered or uploaded; local code files are read now.

        Raises:
            WyrdError: ``WYRD_LOADER_400_INVALID_ENVELOPE`` naming each refused
                YAML field path, or ``WYRD_SPEC_409_DUPLICATE_SUBMISSION`` for
                two different definitions of one identity.
        """
        ...

    def labels(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``labels`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``labels`` entries.
        """
        ...

    def add_label(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``labels``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def annotations(self, entries: Mapping[str, str]) -> Self:
        """Set the whole ``annotations`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``annotations`` entries.
        """
        ...

    def add_annotation(self, key: str, value: str) -> Self:
        """Insert the entry ``key`` into ``annotations``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def description(self, value: JsonInput) -> Self:
        """Set ``description``, replacing its earlier value; returns this value.

        Args:
            value: The new ``description``.
        """
        ...

    def action(self, value: JsonInput) -> Self:
        """Set ``action``, replacing its earlier value; returns this value.

        Args:
            value: The new ``action``.
        """
        ...

    def budget(self, value: JsonInput) -> Self:
        """Set ``budget``, replacing its earlier value; returns this value.

        Args:
            value: The new ``budget``.
        """
        ...

class ComponentFields(TypedDict, total=False):
    """The ``Component`` fields a constructor takes as keywords, each set as its setter sets it."""

    source: JsonInput
    verified_by: Sequence[BindingInput]
    config: Mapping[str, JsonInput]
    credential_refs: Sequence[JsonInput]

class BindingFields(TypedDict, total=False):
    """The ``Binding`` fields a constructor takes as keywords, each set as its setter sets it."""

    runs_on: FieldInput
    on_failure: Sequence[FieldInput]
    on_verdict_change: Sequence[FieldInput]

class StepFields(TypedDict, total=False):
    """The ``Step`` fields a constructor takes as keywords, each set as its setter sets it."""

    depends_on: Sequence[JsonInput]
    when: JsonInput
    requirements: Sequence[Requirement | Mapping[str, JsonInput]]
    timeout_seconds: JsonInput
    retries: JsonInput

class InputSlotFields(TypedDict, total=False):
    """The ``InputSlot`` fields a constructor takes as keywords, each set as its setter sets it."""

    max_bytes: JsonInput

class ActionFields(TypedDict, total=False):
    """The ``Action`` fields a constructor takes as keywords, each set as its setter sets it."""

    inputs: Mapping[str, JsonInput]
    workspace: Mapping[str, FieldInput]
    runtime: JsonInput

class CheckFields(TypedDict, total=False):
    """The ``Check`` fields a constructor takes as keywords, each set as its setter sets it."""

    inputs: Mapping[str, JsonInput]
    workspace: Mapping[str, FieldInput]
    runtime: JsonInput
    response_format: JsonInput

class ArtifactFile:
    """A fixed file or directory prefix of one exact Artifact version.

    Built by ``Artifact.file`` or ``CardRef.file``; mounted into a workspace
    or run as code, it cannot be replaced by execution input.
    """

class CodeSource:
    """The source of a code action or check."""

    @staticmethod
    def inline(text: str) -> CodeSource:
        """Run ``text``, embedded in the Card as ``{inline: text}``.

        Args:
            text: The source text.
        """
        ...

    @staticmethod
    def file(path: PathLike) -> CodeSource:
        """Run the UTF-8 file at ``path``, read as inline text when its graph is validated.

        Nothing is read now; each validation reads the file afresh.

        Args:
            path: The local source file.
        """
        ...

class CardDefinition:
    """A Card of any registrable kind, authored as its kind and canonical spec mapping."""

    def __init__(
        self, kind: CardKind | str, name: str, version: str, spec: Mapping[str, JsonInput]
    ) -> None:
        """Define a Card exactly as its YAML ``spec`` would hold it.

        Args:
            kind: The kind, as a ``CardKind`` or its wire name.
            name: The Card name.
            version: The exact Card version.
            spec: The canonical spec mapping.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown kind.
        """
        ...

    def card_ref(self) -> CardRef:
        """The exact reference this Card registers as."""
        ...

    def spec(self) -> dict[str, Any]:
        """The spec as authored."""
        ...

    def validate(self) -> None:
        """Validate the graph this Card roots exactly as its declaration would.

        Raises:
            WyrdError: The shared validation's refusal.
        """
        ...

class Component:
    """One Service component: an alias for a referenced Card and the Verifiers that check it.

    ``Component.verified_by`` judges this component; ``Service.verified_by``
    judges the Service.
    """

    def __init__(self, alias: str, ref: FieldInput, **fields: Unpack[ComponentFields]) -> None:
        """The component ``alias`` for ``ref``.

        Args:
            alias: The component alias.
            ref: A Card, an exact ``CardRef``, or a YAML reference path.
            **fields: Further component fields, by setter name.
        """
        ...

    def source(self, value: JsonInput) -> Self:
        """Set ``source``, replacing its earlier value; returns this value.

        Args:
            value: The new ``source``.
        """
        ...

    def verified_by(self, items: Iterable[BindingInput]) -> Self:
        """Set the whole ``verified_by`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``verified_by`` entries.
        """
        ...

    def add_verified_by(self, item: BindingInput) -> Self:
        """Append one entry to ``verified_by``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def config(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``config`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``config`` entries.
        """
        ...

    def add_config(self, key: str, value: JsonInput) -> Self:
        """Insert the entry ``key`` into ``config``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def credential_refs(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``credential_refs`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``credential_refs`` entries.
        """
        ...

    def add_credential_ref(self, item: JsonInput) -> Self:
        """Append one entry to ``credential_refs``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

class Binding:
    """One verification binding: the Verifier, when it runs, and the Operators reacting to it."""

    def __init__(self, verifier: FieldInput, **fields: Unpack[BindingFields]) -> None:
        """Bind ``verifier``.

        Args:
            verifier: A ``Verifier``, an exact ``CardRef``, or a YAML reference path.
            **fields: Further binding fields, by setter name.
        """
        ...

    def runs_on(self, value: FieldInput) -> Self:
        """Set ``runs_on``, replacing its earlier value; returns this value.

        Args:
            value: The new ``runs_on``.
        """
        ...

    def on_failure(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``on_failure`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``on_failure`` entries.
        """
        ...

    def add_on_failure(self, item: FieldInput) -> Self:
        """Append one entry to ``on_failure``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def on_verdict_change(self, items: Iterable[FieldInput]) -> Self:
        """Set the whole ``on_verdict_change`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``on_verdict_change`` entries.
        """
        ...

    def add_on_verdict_change(self, item: FieldInput) -> Self:
        """Append one entry to ``on_verdict_change``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

class Step:
    """One data-producing Verifier step."""

    def __init__(
        self, id: str, action: Action | Mapping[str, JsonInput], **fields: Unpack[StepFields]
    ) -> None:
        """The step ``id`` running ``action``.

        Args:
            id: The step ID.
            action: The step's action.
            **fields: Further step fields, by setter name.
        """
        ...

    def depends_on(self, items: Iterable[JsonInput]) -> Self:
        """Set the whole ``depends_on`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``depends_on`` entries.
        """
        ...

    def add_depends_on(self, item: JsonInput) -> Self:
        """Append one entry to ``depends_on``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def when(self, value: JsonInput) -> Self:
        """Set ``when``, replacing its earlier value; returns this value.

        Args:
            value: The new ``when``.
        """
        ...

    def requirements(self, items: Iterable[Requirement | Mapping[str, JsonInput]]) -> Self:
        """Set the whole ``requirements`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``requirements`` entries.
        """
        ...

    def add_requirement(self, item: Requirement | Mapping[str, JsonInput]) -> Self:
        """Append one entry to ``requirements``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def timeout_seconds(self, value: JsonInput) -> Self:
        """Set ``timeout_seconds``, replacing its earlier value; returns this value.

        Args:
            value: The new ``timeout_seconds``.
        """
        ...

    def retries(self, value: JsonInput) -> Self:
        """Set ``retries``, replacing its earlier value; returns this value.

        Args:
            value: The new ``retries``.
        """
        ...

class Requirement:
    """One requirement, producing exactly one verdict."""

    def __init__(self, id: str, statement: str, check: Check | Mapping[str, JsonInput]) -> None:
        """The requirement ``id`` claiming ``statement``, judged by ``check``.

        Args:
            id: The requirement ID.
            statement: The claim the requirement judges.
            check: The check producing its verdict.
        """
        ...

class InputSlot:
    """One named evidence slot of a Verifier.

    Omitted evidence stays missing, never ``None``: the checks that need it are
    inconclusive. A ``json`` slot's schema alone decides whether a supplied
    ``None`` is admitted.
    """

    @staticmethod
    def json(schema: type | Mapping[str, JsonInput]) -> InputSlot:
        """A JSON value constrained by ``schema``.

        Args:
            schema: A JSON Schema, or a class standing for one.
        """
        ...

    @staticmethod
    def media(formats: Iterable[str], **fields: Unpack[InputSlotFields]) -> InputSlot:
        """A pinned file in one of ``formats``, such as ``"pdf"``.

        Args:
            formats: The accepted formats.
            **fields: ``max_bytes``.
        """
        ...

    @staticmethod
    def trace() -> InputSlot:
        """A trace resolved through tenant-scoped trace reads."""
        ...

    @staticmethod
    def samples(features: Iterable[str]) -> InputSlot:
        """Recorded measurements of ``features`` for a scheduled window.

        Args:
            features: The recorded features.
        """
        ...

    @staticmethod
    def source(text: str) -> InputSlot:
        """A group Verifier's input authored as a source reference.

        Args:
            text: A reference such as ``verifiers.<name>.inputs.<slot>``.
        """
        ...

    def max_bytes(self, value: JsonInput) -> Self:
        """Set ``max_bytes``, replacing its earlier value; returns this value.

        Args:
            value: The new ``max_bytes``.
        """
        ...

class Action:
    """A step action: extract, an Agent call, or sandboxed code."""

    @staticmethod
    def extract(from_: str) -> Action:
        """Extract text and structure from a media slot.

        Args:
            from_: The YAML ``from`` reference, such as ``inputs.report``.
        """
        ...

    @staticmethod
    def agent(agent: FieldInput, **fields: Unpack[ActionFields]) -> Action:
        """Invoke ``agent``.

        Args:
            agent: An ``Agent``, an exact ``CardRef``, a YAML path, or an inline Agent mapping.
            **fields: Further action fields, by setter name.
        """
        ...

    @staticmethod
    def code(
        source: CodeSource | ArtifactFile | Mapping[str, JsonInput],
        entrypoint: str,
        **fields: Unpack[ActionFields],
    ) -> Action:
        """Run ``entrypoint`` of ``source`` as sandboxed code returning JSON data.

        Args:
            source: The code source.
            entrypoint: The function to call.
            **fields: Further action fields, by setter name.
        """
        ...

    def inputs(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``inputs`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``inputs`` entries.
        """
        ...

    def bind_input(self, key: str, value: JsonInput) -> Self:
        """Bind the argument ``key`` to the reference ``value`` in ``inputs``; returns this value.

        Args:
            key: The argument's name.
            value: The reference it reads, such as ``inputs.report``.
        """
        ...

    def workspace(self, entries: Mapping[str, FieldInput]) -> Self:
        """Set the whole ``workspace`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``workspace`` entries.
        """
        ...

    def add_workspace(self, key: str, value: FieldInput) -> Self:
        """Insert the entry ``key`` into ``workspace``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def runtime(self, value: JsonInput) -> Self:
        """Set ``runtime``, replacing its earlier value; returns this value.

        Args:
            value: The new ``runtime``.
        """
        ...

class Check:
    """A verdict-producing requirement check."""

    @staticmethod
    def assert_(path: str, operator: JsonInput, expected: JsonInput) -> Check:
        """Compare the one value at ``path`` with ``expected``; the YAML ``kind`` is ``assert``.

        Args:
            path: The reference to compare.
            operator: ``"equals"`` or another operator, or its object form.
            expected: The expected value.
        """
        ...

    @staticmethod
    def each(each: str, operator: JsonInput, expected: JsonInput) -> Check:
        """Compare every value ``each`` selects through ``[*]`` with ``expected``.

        Args:
            each: The reference selecting each value.
            operator: The operator or its object form.
            expected: The expected value.
        """
        ...

    @staticmethod
    def trace(on: str, span_selector: str, operator: JsonInput, expected: JsonInput) -> Check:
        """Compare the spans ``span_selector`` selects in the trace slot ``on``.

        Args:
            on: The trace slot reference.
            span_selector: The span selector.
            operator: The operator or its object form.
            expected: The expected value.
        """
        ...

    @staticmethod
    def agent(agent: FieldInput, **fields: Unpack[CheckFields]) -> Check:
        """Judge with ``agent``, which returns the Verdict schema.

        Args:
            agent: An ``Agent``, an exact ``CardRef``, a YAML path, or an inline Agent mapping.
            **fields: Further check fields, by setter name.
        """
        ...

    @staticmethod
    def code(
        source: CodeSource | ArtifactFile | Mapping[str, JsonInput],
        entrypoint: str,
        **fields: Unpack[CheckFields],
    ) -> Check:
        """Judge with ``entrypoint`` of ``source``, which returns the Verdict schema.

        Args:
            source: The code source.
            entrypoint: The function to call.
            **fields: Further check fields, by setter name.
        """
        ...

    @staticmethod
    def drift(on: str, spec: Mapping[str, JsonInput]) -> Check:
        """Judge statistical drift over the samples slot ``on``.

        Args:
            on: The samples slot reference.
            spec: The drift fields: method, signal, condition, and profile.
        """
        ...

    def inputs(self, entries: Mapping[str, JsonInput]) -> Self:
        """Set the whole ``inputs`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``inputs`` entries.
        """
        ...

    def bind_input(self, key: str, value: JsonInput) -> Self:
        """Bind the argument ``key`` to the reference ``value`` in ``inputs``; returns this value.

        Args:
            key: The argument's name.
            value: The reference it reads, such as ``inputs.report``.
        """
        ...

    def workspace(self, entries: Mapping[str, FieldInput]) -> Self:
        """Set the whole ``workspace`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The new ``workspace`` entries.
        """
        ...

    def add_workspace(self, key: str, value: FieldInput) -> Self:
        """Insert the entry ``key`` into ``workspace``; returns this value.

        Args:
            key: The entry's key.
            value: The entry's value, captured as it is now.
        """
        ...

    def runtime(self, value: JsonInput) -> Self:
        """Set ``runtime``, replacing its earlier value; returns this value.

        Args:
            value: The new ``runtime``.
        """
        ...

    def response_format(self, value: JsonInput) -> Self:
        """Set ``response_format``, replacing its earlier value; returns this value.

        Args:
            value: The new ``response_format``.
        """
        ...

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
