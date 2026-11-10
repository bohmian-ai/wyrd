"""Card definitions, the evidence Verifiers accept, and the executions that judge it.

Each named Card is ``Kind(name, version=None, **fields)``; an omitted version is
assigned at registration, and a reference to that Card resolves to the version
registered. Each sub-value takes the fields that identify it. Every other
method is named for the YAML field it edits: references stay strings such as
``inputs.report.text``, closed values take their canonical projections such as
``ComparisonOperator.Equals``, ``ExecutionRoute.wyrd_gateway(deployment=...)``,
``AgentRunConfig(...)`` and the original ``Prompt``, and nested values stay
canonical mappings. A setter such as ``Service.verified_by`` replaces the whole
field; an add method such as ``Service.add_verified_by`` appends one entry.
Both edit the receiver and return it.

Construction and editing never validate, read files, or touch the network.
Passing a child copies its current value, so later edits to the child change
nothing already passed. ``validate()``, ``Cards.declare`` and
``WyrdState.declare`` run the shared Rust validation; declaration publishes
each local Artifact first, then the graph::

    verifier = Verifier("report-policy", inputs={"report": Report})
    service = Service("finance-release")
    service.add_component(Component("reviewer", agent).verified_by([verifier]))
    state = WyrdState.declare(service)

An input slot also accepts ``str``, ``int``, ``float``, ``bool`` and ``None``'s
type, a class with ``model_json_schema()`` such as a Pydantic model, or a bare
JSON Schema; Pydantic is never imported for you.

``File`` and ``ArtifactRef`` name media evidence for ``Observe.verify`` and
``Observe.submit``; ``submit`` returns an ``Execution`` any process can rebuild
from its ID and wait on.
"""

import os
import pathlib
from collections.abc import Iterable, Mapping, Sequence
from typing import (
    Any,
    ClassVar,
    Generic,
    Literal,
    NotRequired,
    Protocol,
    Self,
    TypeAlias,
    TypedDict,
    Unpack,
)

from typing_extensions import TypeVar

from .. import cards
from ..cards import CardKind, JsonScalar, JsonValue
from ..client import WyrdClient
from ..prompt import Prompt, ResponseFormat

# Wire views of the fixed records ``VerificationResult`` and
# ``Execution.status`` return as dictionaries. They mirror the shared Rust
# wire types field for field: ``NotRequired`` fields are absent, not ``None``,
# when the server has no value. They are stub-only, like every response view.

class CardRefWire(TypedDict):
    """An exact Card reference as it appears inside a wire record."""

    kind: str
    name: str
    version: str
    uid: NotRequired[str]

class JsonEvidence(TypedDict):
    """Inline JSON evidence for a ``json`` slot."""

    kind: Literal["json"]
    value: JsonValue

class FileInput(TypedDict):
    """A file pinned by its SHA-256 digest."""

    kind: Literal["file"]
    sha256: str
    size_bytes: int
    format: str
    filename: NotRequired[str]

class ArtifactInput(TypedDict):
    """A declared Artifact Card's file."""

    kind: Literal["artifact"]
    artifact: CardRefWire
    path: NotRequired[str]

class TraceInput(TypedDict):
    """A recorded trace for a ``trace`` slot."""

    kind: Literal["trace"]
    trace_id: str

InputDeclaration: TypeAlias = JsonEvidence | FileInput | ArtifactInput | TraceInput
"""The original evidence one input slot resolved to, tagged by ``kind``."""

class PageLocator(TypedDict):
    """A one-based PDF page."""

    page: int

class SlideLocator(TypedDict):
    """A one-based PPTX slide."""

    slide: int

class _LineRange(TypedDict):
    start: int
    end: int

class LinesLocator(TypedDict):
    """A one-based inclusive text line range."""

    lines: _LineRange

class JsonPointerLocator(TypedDict):
    """An RFC 6901 pointer into JSON input or extracted JSON."""

    json_pointer: str

class _Rectangle(TypedDict):
    page: int
    x: float
    y: float
    width: float
    height: float

class RegionLocator(TypedDict):
    """A rectangle on a one-based page, in page coordinates."""

    region: _Rectangle

Locator: TypeAlias = PageLocator | SlideLocator | LinesLocator | JsonPointerLocator | RegionLocator
"""A one-key location in an original input slot, such as ``{"page": 7}``."""

class Citation(TypedDict):
    """An original-evidence location supporting a judgment."""

    slot: str
    locator: Locator

class _NativeRoute(TypedDict):
    kind: Literal["native"]

class _GatewayRoute(TypedDict):
    kind: Literal["wyrd_gateway"]
    deployment: str

class AgentProvenance(TypedDict):
    """An Agent call; ``agent`` and ``prompt`` are absent when declared inline."""

    kind: Literal["agent"]
    agent: NotRequired[CardRefWire]
    prompt: NotRequired[CardRefWire]
    route: _NativeRoute | _GatewayRoute

class CodeProvenance(TypedDict):
    """A sandboxed code call."""

    kind: Literal["code"]
    sha256: str
    runtime: Literal["python"]

class ExtractProvenance(TypedDict):
    """An extraction."""

    kind: Literal["extract"]
    version: str

class DriftWindow(TypedDict):
    """A scored sample window as RFC 3339 instants, end exclusive."""

    start: str
    end: str

class DriftProvenance(TypedDict):
    """A drift check; ``window`` is set for scheduled evidence."""

    kind: Literal["drift"]
    window: NotRequired[DriftWindow]

CallProvenance: TypeAlias = AgentProvenance | CodeProvenance | ExtractProvenance | DriftProvenance
"""What executed one Agent, code, extract, or drift block, tagged by ``kind``."""

DriftVerdict: TypeAlias = Literal["NoDrift", "Drift", "Inconclusive"]

class FeatureDriftReport(TypedDict):
    """One feature's drift score; ``evidence`` holds PSI bins or SPC chart detail."""

    feature: str
    score: float | None
    threshold: float | None
    verdict: DriftVerdict
    evidence: NotRequired[JsonValue]

class DriftReport(TypedDict):
    """The full per-feature statistics of a drift check."""

    method: Literal["Psi", "Spc", "Custom"]
    features: dict[str, FeatureDriftReport]
    verdict: DriftVerdict

class RequirementDetail(TypedDict):
    """Check-specific requirement detail."""

    drift: DriftReport

class SiblingRefWire(TypedDict):
    """A loader-projected reference to another Card in the same submission."""

    sibling: CardRefWire

CardRefTarget: TypeAlias = CardRefWire | SiblingRefWire | str
"""A declared, sibling, or authored-path (``str``) Card reference."""

ComparisonOperatorWire: TypeAlias = str | dict[str, JsonValue]
"""A scalar operator discriminator, or a parameterized operator object."""

class AssertCheck(TypedDict):
    """Typed comparison over one value (``path``) or every match (``each``)."""

    kind: Literal["assert"]
    path: NotRequired[str]
    each: NotRequired[str]
    operator: ComparisonOperatorWire
    expected: NotRequired[JsonValue]

class TraceCheck(TypedDict):
    """Comparison over one span selected from a trace slot."""

    kind: Literal["trace"]
    on: str
    span_selector: str
    operator: ComparisonOperatorWire
    expected: NotRequired[JsonValue]

class AgentCheck(TypedDict):
    """A tool-less Agent returning the Verdict schema.

    ``agent`` is a Card reference or an inline Agent spec object.
    """

    kind: Literal["agent"]
    agent: CardRefTarget | dict[str, JsonValue]
    inputs: NotRequired[dict[str, str]]
    workspace: NotRequired[dict[str, str]]

class InlineCodeSource(TypedDict):
    """Source text embedded in the Verifier version."""

    inline: str

class _ArtifactFile(TypedDict):
    card: CardRefTarget
    path: NotRequired[str]

class ArtifactCodeSource(TypedDict):
    """A file inside an exact Artifact Card version."""

    artifact: _ArtifactFile

class _VerdictFormat(TypedDict):
    kind: Literal["verdict"]
    data: NotRequired[dict[str, JsonValue]]

class CodeCheck(TypedDict):
    """Sandboxed code returning the Verdict schema.

    ``response_format`` is the declared ``verdict`` format, with ``data`` the
    JSON Schema of its required custom data; absent means plain ``verdict``.
    """

    kind: Literal["code"]
    runtime: NotRequired[Literal["python"]]
    source: InlineCodeSource | ArtifactCodeSource
    entrypoint: str
    inputs: NotRequired[dict[str, str]]
    response_format: NotRequired[Literal["verdict"] | _VerdictFormat]

class DriftCheck(TypedDict):
    """Statistical drift over a samples slot, with the drift spec flattened."""

    kind: Literal["drift"]
    on: str
    description: NotRequired[str]
    method: Literal["Psi", "Spc", "Custom"]
    signal: dict[str, JsonValue]
    condition: dict[str, JsonValue]
    profile: NotRequired[dict[str, JsonValue]]

CheckWire: TypeAlias = AssertCheck | TraceCheck | AgentCheck | CodeCheck | DriftCheck
"""The requirement's check, tagged by ``kind``."""

RequirementOutcome: TypeAlias = Literal["passed", "failed", "inconclusive", "not_applicable"]

DataT = TypeVar("DataT", default=dict[str, JsonValue])
"""The type of each requirement's custom ``data``: JSON by default."""

_Model = TypeVar("_Model", bound="DataModel")

class DataModel(Protocol):
    """Anything that validates one ``data`` object and returns itself typed.

    A Pydantic model class, including a ``RootModel`` union for mixed shapes,
    satisfies this by construction; Pydantic itself is not required.
    """

    @classmethod
    def model_validate(cls: type[_Model], obj: Any, /) -> _Model:
        """Validate one custom ``data`` object into an instance of this model.

        Args:
            obj: the requirement's decoded ``data`` JSON value.
        """
        ...

class RequirementResult(TypedDict, Generic[DataT]):
    """One requirement's judgment.

    ``check`` is the Verifier Card's check declaration as authored; ``actual``
    is the JSON value the check read; ``data`` is the custom data the check's
    declared ``verdict`` format returned, absent when it declared none.
    """

    id: str
    step: NotRequired[str]
    statement: str
    outcome: RequirementOutcome
    reason: str
    actual: NotRequired[JsonValue]
    citations: NotRequired[list[Citation]]
    data: NotRequired[DataT]
    detail: NotRequired[RequirementDetail]
    check: CheckWire
    provenance: NotRequired[CallProvenance]

BuiltinTool: TypeAlias = Literal[
    "cards.list",
    "cards.get",
    "bifrost.list_tables",
    "bifrost.describe_table",
    "bifrost.query",
    "fs.list",
    "fs.read",
    "fs.search",
    "fs.extract",
    "code.run",
]

class ToolCallRecord(TypedDict):
    """One tool call an Agent action made; the raw output is never kept."""

    tool: BuiltinTool
    arguments: JsonValue
    output_sha256: NotRequired[str]
    returned_bytes: int
    error: NotRequired[str]

class StepResult(TypedDict):
    """One step's execution record."""

    id: str
    status: Literal["succeeded", "failed", "skipped"]
    output: NotRequired[JsonValue]
    attempts: int
    duration_ms: int
    reason: NotRequired[str]
    provenance: NotRequired[CallProvenance]
    tool_calls: NotRequired[list[ToolCallRecord]]

VerificationVerdict: TypeAlias = Literal["passed", "failed", "inconclusive"]

class VerificationOutcome(TypedDict, Generic[DataT]):
    """The decided verdict and its full breakdown."""

    verdict: VerificationVerdict
    requirements: list[RequirementResult[DataT]]
    steps: list[StepResult]
    evidence: dict[str, InputDeclaration]

class VerificationResultWire(TypedDict, Generic[DataT]):
    """A result as ``Execution.status()`` carries it under ``result``."""

    execution_id: str
    run_id: NotRequired[str]
    verifier: CardRefWire
    subject: CardRefWire
    outcome: VerificationOutcome[DataT]
    deployment_environment: NotRequired[str]

class VerificationError(TypedDict):
    """A settled execution's secret-free stable error."""

    code: str
    message: str

ExecutionStatus: TypeAlias = Literal[
    "pending", "running", "retrying", "completed", "cancelled", "timed_out", "errored"
]

class FrozenUid(TypedDict):
    """A dispatch's frozen Operator Card UID."""

    uid: str

class FrozenDigest(TypedDict):
    """A dispatch's frozen inline Operator spec digest."""

    digest: str

class OperatorDispatchState(TypedDict):
    """One Operator dispatch's independent delivery state."""

    dispatch_id: str
    operator: FrozenUid | FrozenDigest
    status: Literal["pending", "running", "retrying", "delivered", "failed"]
    error: VerificationError | None

class GroupSourceSnapshot(TypedDict):
    """One source slot's pinned selection: its declaration and source executions in list order."""

    reference: str
    execution_ids: list[str]

class GroupSnapshot(TypedDict):
    """The source snapshot a group execution judges."""

    sources: dict[str, GroupSourceSnapshot]

class ExecutionState(TypedDict, Generic[DataT]):
    """``Execution.status()``: the lifecycle, terminal error, visible result, and grouping."""

    execution_id: str
    status: ExecutionStatus
    error: NotRequired[VerificationError]
    result: NotRequired[VerificationResultWire[DataT]]
    dispatches: list[OperatorDispatchState]
    group_id: NotRequired[str]
    owner_card_uid: NotRequired[str]
    group: NotRequired[GroupSnapshot]

class File:
    """A local file to verify, read only when ``verify`` or ``submit`` uses it.

    Constructing a ``File`` performs no IO and declares nothing. During the
    call the client hashes it with SHA-256, asks the server which digests it
    lacks, and uploads only those straight to tenant storage. An uploaded file
    is evidence pinned by its digest, never an Artifact Card.
    """

    def __init__(
        self,
        source: str | os.PathLike[str] | bytes,
        *,
        format: str | None = None,
        filename: str | None = None,
    ) -> None:
        """Name a file at a path, or in-memory bytes.

        Args:
            source: a path, opened only during the call, or the file's bytes.
            format: the canonical format, such as ``"pdf"`` or ``"pptx"``.
                Omitted, it is inferred from the filename extension, else the
                slot's single accepted format.
            filename: a display name; defaults to the path's file name.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a ``source`` of
                another type or an unknown ``format``.
        """
        ...

class ArtifactRef:
    """A declared Artifact's file, verified with its normal authorization."""

    def __init__(self, artifact: cards.CardRef, path: str | None = None) -> None:
        """Select ``artifact``'s only file, or ``path`` within it.

        Args:
            artifact: the exact Artifact Card reference.
            path: the file inside the Artifact, when it holds several.
        """
        ...

class Recorded:
    """``submit``'s answer for scheduled-window samples.

    The samples were recorded; the Verifier's schedule judges the window
    later, so there is no execution to wait for.
    """

class VerificationResult(Generic[DataT]):
    """The attributable result of one Verifier execution.

    A ``failed`` verdict is an ordinary return value, not an exception.
    ``DataT`` is each requirement's ``data`` type: JSON for ordinary reads,
    the caller's type for ``verify_typed``, ``wait_typed``, and
    ``status_typed``.
    """

    @property
    def execution_id(self) -> str:
        """Server-issued execution ID."""
        ...

    @property
    def run_id(self) -> str | None:
        """The caller's application Run, when there is one."""
        ...

    @property
    def verifier(self) -> cards.CardRef:
        """The exact Verifier executed."""
        ...

    @property
    def subject(self) -> cards.CardRef:
        """The exact subject judged."""
        ...

    @property
    def deployment_environment(self) -> str | None:
        """The deployment environment correlation tag the request supplied, or ``None``."""
        ...

    @property
    def verdict(self) -> VerificationVerdict:
        """The ``decide`` rule applied to every applicable requirement."""
        ...

    @property
    def requirements(self) -> list[RequirementResult[DataT]]:
        """Every requirement's judgment as its wire mapping, in plan order."""
        ...

    @property
    def steps(self) -> list[StepResult]:
        """Every step's result as its wire mapping, in plan order."""
        ...

    @property
    def evidence(self) -> dict[str, InputDeclaration]:
        """The pinned original evidence by slot, as wire declarations."""
        ...

class Execution:
    """One queued Verifier execution, addressed by its server-issued ID.

    Returned by ``Observe.submit`` for Run evidence, or rebuilt in any process
    from its ID. The queue, retries, and results stay server-owned.
    """

    def __init__(self, id: str, client: WyrdClient | None = None) -> None:
        """Name execution ``id`` without IO.

        The client, or the ambient chain when ``client`` is omitted, resolves
        and connects once on the first ``status`` or ``wait``, exactly as
        ``connect`` does; a failed connection is retried by the next call.

        Args:
            id: the execution ID ``submit`` returned.
            client: the client every status read is sent as.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an ``id`` that is not
                an execution ID.
        """
        ...

    @staticmethod
    def connect(id: str, client: WyrdClient | None = None) -> Execution:
        """Rebuild the handle of execution ``id`` and connect it now.

        Args:
            id: the execution ID ``submit`` returned.
            client: the client every status read is sent as. Omitted, the
                ambient chain resolves as ``WyrdClient`` does.

        Returns:
            The connected handle.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an ``id`` that is not
                an execution ID, and the client's configuration or credential
                error.
        """
        ...

    @property
    def id(self) -> str:
        """The execution ID."""
        ...

    def status(self) -> ExecutionState:
        """Read the execution's status, terminal error, and visible result.

        Returns:
            The wire mapping with ``execution_id``, ``status``, and, when
            present, ``error`` and ``result``.

        Raises:
            WyrdError: the client's connection error on first use, the
                server's not-found error before the execution is visible, and
                its authorization or transport error.
        """
        ...

    def status_typed(self, data_type: type[_Model]) -> ExecutionState[_Model]:
        """``status``, with a present result's ``data`` validated as ``data_type``.

        Every present ``data`` object, failed requirements included, passes
        through ``data_type.model_validate``; absent data stays absent. The
        stored result is never changed.

        Args:
            data_type: a model exposing ``model_validate``, such as a
                Pydantic model class.

        Returns:
            The wire mapping, its result's ``data`` typed.

        Raises:
            WyrdError: As ``status``;
                ``WYRD_SDK_422_VERIFICATION_DATA_INVALID`` naming the first
                requirement ``data_type`` refuses, with the data root's
                empty ``path``, never the data; ``WYRD_SPEC_400_VALIDATION`` when ``data_type`` has no
                ``model_validate``.
        """
        ...

    def wait_typed(self, timeout: float, data_type: type[_Model]) -> VerificationResult[_Model]:
        """``wait``, with every present ``data`` validated as ``data_type``.

        Args:
            timeout: seconds to keep polling.
            data_type: a model exposing ``model_validate``.

        Returns:
            The execution's result, its ``data`` typed.

        Raises:
            WyrdError: As ``wait``, then as ``status_typed`` for ``data``.
        """
        ...

    def wait(self, timeout: float) -> VerificationResult:
        """Poll until the execution yields a result, fails, or ``timeout`` passes.

        Polls back off from 100 ms to at most 2 s. A not-yet-visible
        execution and a completion whose result is not visible yet keep
        being polled; a result is never invented from a status.

        Args:
            timeout: seconds to keep polling.

        Returns:
            The execution's result.

        Raises:
            WyrdError: the execution's own error when it settled as errored,
                cancelled, or timed out, including a delayed
                ``WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT``;
                ``WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`` when ``timeout``
                passes first; ``WYRD_SPEC_400_VALIDATION`` for a negative
                ``timeout``; the client's connection error on first use; and
                any other status-read error.
        """
        ...

JsonInput: TypeAlias = JsonScalar | Sequence[JsonInput] | Mapping[str, JsonInput]
"""Any JSON value as authored: nested mappings and sequences of JSON values."""

PathLike: TypeAlias = str | os.PathLike[str] | pathlib.Path

AnyCard: TypeAlias = (
    Service
    | Agent
    | Verifier
    | Artifact
    | Model
    | Data
    | Experiment
    | Mcp
    | Audit
    | Trigger
    | Operator
)
"""Any authored Card; at a reference position it becomes a sibling in the same graph."""

FieldInput: TypeAlias = (
    AnyCard
    | CardRef
    | cards.CardRef
    | Prompt
    | ComparisonOperator
    | ExecutionRoute
    | AgentRunConfig
    | ArtifactFile
    | CodeSource
    | Component
    | Binding
    | Step
    | Requirement
    | InputSlot
    | Action
    | Check
    | JsonScalar
    | Sequence[FieldInput]
    | Mapping[str, FieldInput]
)
"""A value at a reference position: a Card, an exact reference, an authoring value, or JSON,
whose nested mappings and sequences may hold any of these."""

BindingInput: TypeAlias = (
    Binding | Verifier | CardRef | cards.CardRef | str | Mapping[str, FieldInput]
)
"""A binding, or the Verifier, exact reference or YAML path it binds."""

SlotInput: TypeAlias = InputSlot | str | type | bool | Mapping[str, JsonInput]
"""An input slot: a slot, a source reference, a class standing for a schema, a boolean
JSON Schema (``True`` admits every value, ``False`` none), or a mapping.

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
            version: The exact Card version; ``None`` lets registration assign it.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown kind.
        """
        ...

    @staticmethod
    def data(name: str, version: str) -> CardRef:
        """The exact reference to the Data Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def model(name: str, version: str) -> CardRef:
        """The exact reference to the Model Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def artifact(name: str, version: str) -> CardRef:
        """The exact reference to the Artifact Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def experiment(name: str, version: str) -> CardRef:
        """The exact reference to the Experiment Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
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
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def mcp(name: str, version: str) -> CardRef:
        """The exact reference to the Mcp Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def service(name: str, version: str) -> CardRef:
        """The exact reference to the Service Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def audit(name: str, version: str) -> CardRef:
        """The exact reference to the Audit Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def verifier(name: str, version: str) -> CardRef:
        """The exact reference to the Verifier Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def trigger(name: str, version: str) -> CardRef:
        """The exact reference to the Trigger Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
        """
        ...

    @staticmethod
    def operator(name: str, version: str) -> CardRef:
        """The exact reference to the Operator Card ``name`` at ``version``.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
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
    components: Sequence[Component | Mapping[str, FieldInput]]
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[ServiceFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def components(self, items: Iterable[Component | Mapping[str, FieldInput]]) -> Self:
        """Set the whole ``components`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``components`` entries.
        """
        ...

    def add_component(self, item: Component | Mapping[str, FieldInput]) -> Self:
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
    prompt: Prompt | FieldInput
    execution_route: ExecutionRoute | JsonInput
    tool_names: Sequence[str]
    run_config: AgentRunConfig | JsonInput
    verified_by: Sequence[BindingInput]

class Agent:
    """An Agent Card: a prompt run through an execution route with tools."""

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[AgentFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def prompt(self, value: Prompt | FieldInput) -> Self:
        """Set ``prompt``, replacing its earlier value; returns this value.

        Args:
            value: The new ``prompt``.
        """
        ...

    def execution_route(self, value: ExecutionRoute | JsonInput) -> Self:
        """Set ``execution_route``, replacing its earlier value; returns this value.

        Args:
            value: The new ``execution_route``.
        """
        ...

    def tool_names(self, items: Iterable[str]) -> Self:
        """Set the whole ``tool_names`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``tool_names`` entries.
        """
        ...

    def add_tool_name(self, item: str) -> Self:
        """Append one entry to ``tool_names``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def run_config(self, value: AgentRunConfig | JsonInput) -> Self:
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

class VerifierFields(TypedDict, total=False):
    """The ``Verifier`` fields a constructor takes as keywords, each set as its setter sets it."""

    labels: Mapping[str, str]
    annotations: Mapping[str, str]
    description: JsonInput
    inputs: Mapping[str, SlotInput] | type
    steps: Sequence[Step | Mapping[str, FieldInput]]
    requirements: Sequence[Requirement | Mapping[str, FieldInput]]
    decide: JsonInput
    context_capture: JsonInput
    order_by: JsonInput

class Verifier:
    """A Verifier Card: the evidence it accepts, the steps that compute over it, and the
    requirements that judge it."""

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[VerifierFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def inputs(self, entries: Mapping[str, SlotInput] | type) -> Self:
        """Set the whole ``inputs`` map, replacing every earlier entry; returns this value.

        Args:
            entries: The slots by name, or a class whose ``model_json_schema()``
                declares one slot per property, such as a Pydantic model: ``Inputs``
                with ``release: Release`` declares the slots of ``{"release": Release}``.
        """
        ...

    def add_input(self, key: str, value: SlotInput) -> Self:
        """Declare the evidence slot ``key`` in ``inputs``; returns this value.

        Args:
            key: The slot's name.
            value: The slot, or a JSON Schema or class standing for one.
        """
        ...

    def steps(self, items: Iterable[Step | Mapping[str, FieldInput]]) -> Self:
        """Set the whole ``steps`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``steps`` entries.
        """
        ...

    def add_step(self, item: Step | Mapping[str, FieldInput]) -> Self:
        """Append one entry to ``steps``; returns this value.

        Args:
            item: The entry, captured as it is now.
        """
        ...

    def requirements(self, items: Iterable[Requirement | Mapping[str, FieldInput]]) -> Self:
        """Set the whole ``requirements`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``requirements`` entries.
        """
        ...

    def add_requirement(self, item: Requirement | Mapping[str, FieldInput]) -> Self:
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[ArtifactFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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
        name: str, version: str | None, path: PathLike, **fields: Unpack[ArtifactFields]
    ) -> Artifact:
        """The ``file`` Artifact published from ``path`` before its graph.

        Nothing is read until it is published.

        Args:
            name: The Artifact name.
            version: The exact Artifact version; ``None`` lets registration assign it.
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[ModelFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(self, name: str, version: str | None = None, **fields: Unpack[DataFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[ExperimentFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(self, name: str, version: str | None = None, **fields: Unpack[McpFields]) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[AuditFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[TriggerFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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

    def __init__(
        self, name: str, version: str | None = None, **fields: Unpack[OperatorFields]
    ) -> None:
        """An empty Card; nothing is validated until its graph is.

        Args:
            name: The Card name.
            version: The exact Card version; ``None`` lets registration assign it.
            **fields: Spec fields, ``labels`` and ``annotations``, by setter name.
        """
        ...

    def card_ref(self) -> CardRef | None:
        """The exact reference this Card registers as, or ``None`` until registration
        assigns its version."""
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
    requirements: Sequence[Requirement | Mapping[str, FieldInput]]
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
        self, id: str, action: Action | Mapping[str, FieldInput], **fields: Unpack[StepFields]
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

    def requirements(self, items: Iterable[Requirement | Mapping[str, FieldInput]]) -> Self:
        """Set the whole ``requirements`` list, replacing every earlier entry; returns this value.

        Args:
            items: The new ``requirements`` entries.
        """
        ...

    def add_requirement(self, item: Requirement | Mapping[str, FieldInput]) -> Self:
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

    def __init__(self, id: str, statement: str, check: Check | Mapping[str, FieldInput]) -> None:
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
    def json(schema: type | bool | Mapping[str, JsonInput]) -> InputSlot:
        """A JSON value constrained by ``schema``.

        Args:
            schema: A JSON Schema, including ``True`` or ``False``, or a class
                standing for one.
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
        source: CodeSource | ArtifactFile | Mapping[str, FieldInput],
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
    def assert_(path: str, operator: OperatorInput, expected: JsonInput) -> Check:
        """Compare the one value at ``path`` with ``expected``; the YAML ``kind`` is ``assert``.

        Args:
            path: The reference to compare.
            operator: A ``ComparisonOperator``, or its wire form.
            expected: The expected value.
        """
        ...

    @staticmethod
    def each(each: str, operator: OperatorInput, expected: JsonInput) -> Check:
        """Compare every value ``each`` selects through ``[*]`` with ``expected``.

        Args:
            each: The reference selecting each value.
            operator: A ``ComparisonOperator``, or its wire form.
            expected: The expected value.
        """
        ...

    @staticmethod
    def trace(on: str, span_selector: str, operator: OperatorInput, expected: JsonInput) -> Check:
        """Compare the spans ``span_selector`` selects in the trace slot ``on``.

        Args:
            on: The trace slot reference.
            span_selector: The span selector.
            operator: A ``ComparisonOperator``, or its wire form.
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
        source: CodeSource | ArtifactFile | Mapping[str, FieldInput],
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
    def drift(on: str, spec: Mapping[str, FieldInput]) -> Check:
        """Judge statistical drift over the samples slot ``on``.

        Args:
            on: The samples slot reference.
            spec: The drift fields: method, signal, condition, and profile; a Card or
                ``CardRef`` may stand at ``signal.baseline_ref``.
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

class ComparisonOperator:
    """One canonical comparison operator of an assertion ``Check``.

    Operators without fields are class attributes such as
    ``ComparisonOperator.Equals``; operators with fields are factories such as
    ``ComparisonOperator.in_range(min=0.0, max=1.0, inclusive=True)``. The
    canonical boundary validates domain rules.
    """

    Equals: ClassVar[ComparisonOperator]
    NotEquals: ClassVar[ComparisonOperator]
    GreaterThan: ClassVar[ComparisonOperator]
    GreaterThanOrEquals: ClassVar[ComparisonOperator]
    LessThan: ClassVar[ComparisonOperator]
    LessThanOrEquals: ClassVar[ComparisonOperator]
    IsPositive: ClassVar[ComparisonOperator]
    IsNegative: ClassVar[ComparisonOperator]
    IsZero: ClassVar[ComparisonOperator]
    Contains: ClassVar[ComparisonOperator]
    NotContains: ClassVar[ComparisonOperator]
    ContainsIgnoreCase: ClassVar[ComparisonOperator]
    StartsWith: ClassVar[ComparisonOperator]
    EndsWith: ClassVar[ComparisonOperator]
    IsEmail: ClassVar[ComparisonOperator]
    IsUrl: ClassVar[ComparisonOperator]
    IsUuid: ClassVar[ComparisonOperator]
    IsIpv4: ClassVar[ComparisonOperator]
    IsIpv6: ClassVar[ComparisonOperator]
    IsJson: ClassVar[ComparisonOperator]
    In: ClassVar[ComparisonOperator]
    NotIn: ClassVar[ComparisonOperator]
    IsSubset: ClassVar[ComparisonOperator]
    IsSuperset: ClassVar[ComparisonOperator]
    IsDisjoint: ClassVar[ComparisonOperator]
    AllOf: ClassVar[ComparisonOperator]
    AnyOf: ClassVar[ComparisonOperator]
    NoneOf: ClassVar[ComparisonOperator]
    IsEmpty: ClassVar[ComparisonOperator]
    IsNonEmpty: ClassVar[ComparisonOperator]
    UniqueValues: ClassVar[ComparisonOperator]
    IsTruthy: ClassVar[ComparisonOperator]
    IsFalsy: ClassVar[ComparisonOperator]
    IsNull: ClassVar[ComparisonOperator]
    IsNotNull: ClassVar[ComparisonOperator]
    IsString: ClassVar[ComparisonOperator]
    IsNumber: ClassVar[ComparisonOperator]
    IsBoolean: ClassVar[ComparisonOperator]
    IsObject: ClassVar[ComparisonOperator]

    @staticmethod
    def in_range(min: float, max: float, inclusive: bool) -> ComparisonOperator:
        """The ``in_range`` operator with its fields."""
        ...

    @staticmethod
    def not_in_range(min: float, max: float, inclusive: bool) -> ComparisonOperator:
        """The ``not_in_range`` operator with its fields."""
        ...

    @staticmethod
    def approximately_equals(tolerance: float) -> ComparisonOperator:
        """The ``approximately_equals`` operator with its fields."""
        ...

    @staticmethod
    def matches_regex(pattern: str) -> ComparisonOperator:
        """The ``matches_regex`` operator with its fields."""
        ...

    @staticmethod
    def not_matches_regex(pattern: str) -> ComparisonOperator:
        """The ``not_matches_regex`` operator with its fields."""
        ...

    @staticmethod
    def has_min_length(min: int) -> ComparisonOperator:
        """The ``has_min_length`` operator with its fields."""
        ...

    @staticmethod
    def has_max_length(max: int) -> ComparisonOperator:
        """The ``has_max_length`` operator with its fields."""
        ...

    @staticmethod
    def length(expected: int) -> ComparisonOperator:
        """The ``length`` operator with its fields."""
        ...

    @staticmethod
    def length_greater_than(min: int) -> ComparisonOperator:
        """The ``length_greater_than`` operator with its fields."""
        ...

    @staticmethod
    def length_less_than(max: int) -> ComparisonOperator:
        """The ``length_less_than`` operator with its fields."""
        ...

    @staticmethod
    def within_abs_tolerance(tolerance: float) -> ComparisonOperator:
        """The ``within_abs_tolerance`` operator with its fields."""
        ...

    @staticmethod
    def within_pct_tolerance(pct: float) -> ComparisonOperator:
        """The ``within_pct_tolerance`` operator with its fields."""
        ...

    @staticmethod
    def within_std_dev(sigma: float, mean: float, std_dev: float) -> ComparisonOperator:
        """The ``within_std_dev`` operator with its fields."""
        ...

    @staticmethod
    def between_percentiles(lower_pct: float, upper_pct: float) -> ComparisonOperator:
        """The ``between_percentiles`` operator with its fields."""
        ...

    @staticmethod
    def cosine_similarity_at_least(threshold: float) -> ComparisonOperator:
        """The ``cosine_similarity_at_least`` operator with its fields."""
        ...

    @staticmethod
    def is_type(
        expected: Literal["null", "bool", "number", "string", "array", "object"],
    ) -> ComparisonOperator:
        """The ``is_type`` operator with its fields."""
        ...

    @staticmethod
    def divergence_less_than(
        metric: Literal["kl", "js", "wasserstein"], threshold: float
    ) -> ComparisonOperator:
        """The ``divergence_less_than`` operator with its fields."""
        ...

class ExecutionRoute:
    """The canonical route an Agent's model calls take."""

    Native: ClassVar[ExecutionRoute]
    """The executing host's own provider configuration."""

    @staticmethod
    def wyrd_gateway(deployment: str) -> ExecutionRoute:
        """The tenant's Wyrd gateway deployment named ``deployment``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an invalid deployment name.
        """
        ...

class AgentRunConfig:
    """An Agent's canonical run configuration; unset fields stay absent and keep the
    runtime's defaults."""

    def __init__(
        self,
        *,
        max_iterations: int | None = None,
        tool_concurrency_cap: int | None = None,
        session_recent_limit: int | None = None,
        timeout_ms: int | None = None,
    ) -> None: ...

OperatorInput: TypeAlias = ComparisonOperator | str | Mapping[str, JsonInput]
"""A comparison operator, or its canonical wire form."""

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
