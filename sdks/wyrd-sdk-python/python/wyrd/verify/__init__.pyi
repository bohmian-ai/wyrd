import os
from typing import Literal, NotRequired, TypeAlias, TypedDict

from ..cards import CardRef, JsonValue
from ..client import WyrdClient

# Wire views of the fixed records ``VerificationResult`` and
# ``Execution.status`` return as dictionaries. They mirror the shared Rust
# wire types field for field: ``NotRequired`` fields are absent, not ``None``,
# when the server has no value. They are stub-only, like every response view.

class CardRefWire(TypedDict):
    """An exact Card reference as it appears inside a wire record."""

    kind: str
    name: str
    version: str
    space: NotRequired[str]
    uid: NotRequired[str]

class JsonInput(TypedDict):
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

InputDeclaration: TypeAlias = JsonInput | FileInput | ArtifactInput | TraceInput
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

class CodeCheck(TypedDict):
    """Sandboxed code returning the Verdict schema."""

    kind: Literal["code"]
    runtime: NotRequired[Literal["python"]]
    source: InlineCodeSource | ArtifactCodeSource
    entrypoint: str
    inputs: NotRequired[dict[str, str]]

class DriftCheck(TypedDict):
    """Statistical drift over a samples slot, with the drift spec flattened."""

    kind: Literal["drift"]
    on: str
    description: NotRequired[str]
    method: Literal["Psi", "Spc", "Custom"]
    signal: dict[str, JsonValue]
    condition: dict[str, JsonValue]
    profile: NotRequired[dict[str, JsonValue]]

Check: TypeAlias = AssertCheck | TraceCheck | AgentCheck | CodeCheck | DriftCheck
"""The requirement's check, tagged by ``kind``."""

RequirementOutcome: TypeAlias = Literal["passed", "failed", "inconclusive", "not_applicable"]

class RequirementResult(TypedDict):
    """One requirement's judgment.

    ``check`` is the Verifier Card's check declaration as authored; ``actual``
    is the JSON value the check read.
    """

    id: str
    step: NotRequired[str]
    statement: str
    outcome: RequirementOutcome
    reason: str
    actual: NotRequired[JsonValue]
    citations: NotRequired[list[Citation]]
    detail: NotRequired[RequirementDetail]
    check: Check
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

class VerificationOutcome(TypedDict):
    """The decided verdict and its full breakdown."""

    verdict: VerificationVerdict
    requirements: list[RequirementResult]
    steps: list[StepResult]
    evidence: dict[str, InputDeclaration]

class VerificationResultWire(TypedDict):
    """A result as ``Execution.status()`` carries it under ``result``."""

    execution_id: str
    run_id: NotRequired[str]
    verifier: CardRefWire
    subject: CardRefWire
    outcome: VerificationOutcome

class VerificationError(TypedDict):
    """A settled execution's secret-free stable error."""

    code: str
    message: str

ExecutionStatus: TypeAlias = Literal[
    "pending", "running", "retrying", "completed", "cancelled", "timed_out", "errored"
]

class ExecutionState(TypedDict):
    """``Execution.status()``: the lifecycle, terminal error, and visible result."""

    execution_id: str
    status: ExecutionStatus
    error: NotRequired[VerificationError]
    result: NotRequired[VerificationResultWire]

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

    def __init__(self, artifact: CardRef, path: str | None = None) -> None:
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

class VerificationResult:
    """The attributable result of one Verifier execution.

    A ``failed`` verdict is an ordinary return value, not an exception.
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
    def verifier(self) -> CardRef:
        """The exact Verifier executed."""
        ...

    @property
    def subject(self) -> CardRef:
        """The exact subject judged."""
        ...

    @property
    def verdict(self) -> VerificationVerdict:
        """The ``decide`` rule applied to every applicable requirement."""
        ...

    @property
    def requirements(self) -> list[RequirementResult]:
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

__all__ = [
    "ArtifactRef",
    "Execution",
    "File",
    "Recorded",
    "VerificationResult",
]
