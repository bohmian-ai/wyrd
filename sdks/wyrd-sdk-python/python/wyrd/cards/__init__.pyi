import builtins
from collections.abc import Mapping
from os import PathLike
from pathlib import Path
from typing import Any, Literal, TypeAlias, TypeVar, overload

from ..client import WyrdClient
from ..data import DataCard, DataInterface
from ..model import ModelCard, ModelInterface
from ..prompt import PromptCard, PromptReference
from ..state import CardEnvelope
from ._card import Card

_DataInterfaceT = TypeVar("_DataInterfaceT", bound=DataInterface)
_ModelInterfaceT = TypeVar("_ModelInterfaceT", bound=ModelInterface)

JsonScalar: TypeAlias = str | int | float | bool | None
JsonValue: TypeAlias = JsonScalar | list[JsonValue] | dict[str, JsonValue]

DeclarableCard: TypeAlias = DataCard[Any] | ModelCard[Any] | PromptCard

class AgentCard:
    """Local Agent Card holder backed by the native Agent envelope.

    Use `PromptReference.inline(prompt)` for an inline prompt or
    `PromptReference.card(...)` for a declared Prompt Card reference. The
    holder contains no runtime, registry, storage, or artifact state.

    Attributes:
        spec: the durable Agent spec as a dict.
        cascade_children: the referenced Prompt Card when the prompt is
            card-backed; empty for an inline prompt.
        prompt_ref: the durable prompt reference.
        kind: the Card kind, always ``"Agent"``.
    """

    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    spec: dict[str, JsonValue]
    cascade_children: list[CardRef]
    prompt_ref: PromptReference
    kind: str

    def __init__(
        self,
        prompt: PromptReference,
        *,
        execution_route: Mapping[str, str],
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
    ) -> None:
        """Create an Agent Card from one prompt reference.

        Args:
            prompt: the inline prompt or declared Prompt Card reference.
            execution_route: the required route, ``{"kind": "native"}`` or
                ``{"kind": "wyrd_gateway", "deployment": <name>}``.
            name: the Card name. ``"agent"`` if omitted.
            version: the semantic version. ``"0.1.0"`` if omitted.
            uid: the Card UID. A new UUIDv7 is generated if omitted.
            labels: queryable labels, merged per key with the workspace
                ``wyrd.toml`` defaults; your keys win.
            annotations: free-form annotations, merged like ``labels``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a malformed
                ``execution_route``, or for an invalid label or annotation, or
                identity values the Agent envelope rejects.

        """
        ...

    def prompt(self) -> PromptCard:
        """Return the declared Prompt Card this hydrated Agent pins.

        A local read that performs no IO or declaration. An authored inline
        prompt is read through ``prompt_ref.prompt``.

        Returns:
            The hydrated `PromptCard`, the same Card any explicit Prompt alias
            of the state names.

        Raises:
            WyrdError: when this Agent was not loaded through ``WyrdState``.
        """
        ...

    def model_dump(self) -> dict[str, JsonValue]:
        """Return the complete Agent Card envelope as a dictionary.

        Returns:
            The envelope as plain Python values.

        Raises:
            WyrdError: If the envelope cannot be serialized.
        """
        ...

    def model_dump_json(self) -> str:
        """Return the complete Agent Card envelope as JSON.

        Returns:
            The envelope as a JSON string.

        Raises:
            WyrdError: If the envelope cannot be serialized.
        """
        ...

    @staticmethod
    def model_validate_json(json_string: str) -> AgentCard:
        """Hydrate an Agent Card from a complete persisted envelope.

        Args:
            json_string: an envelope as produced by ``model_dump_json()``.

        Returns:
            The hydrated `AgentCard`.

        Raises:
            WyrdError: when the JSON is not a complete Agent Card envelope or
                lacks ``metadata.uid``.

        """
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

class CardKind:
    """The kind of a declared Wyrd Card.

    Kind-specific registry views use these values internally. Most Python
    callers use `cards.data`, `cards.model`, or `cards.prompt`, which already
    select the kind and return the matching card type. `External` marks a
    foreign schema descriptor and cannot be declared.
    """

    Data: CardKind
    Model: CardKind
    Experiment: CardKind
    Prompt: CardKind
    Agent: CardKind
    Verifier: CardKind
    Service: CardKind
    Policy: CardKind
    Mcp: CardKind
    Audit: CardKind
    Artifact: CardKind
    Trigger: CardKind
    Operator: CardKind
    Source: CardKind
    External: CardKind

    @property
    def name(self) -> str:
        """The kind's wire name, identical to the attribute name, such as ``"Model"``."""
        ...

class CardRef:
    """Reference to one exact declared Card.

    A reference identifies a kind, name, and version. A server-assigned
    `uid` is present after declaration. Use a `CardRef` from `CardSummary` or
    `DeclarationReceipt` when you need to pass an exact identity between
    operations.
    """

    kind: CardKind
    name: str
    version: str
    uid: str | None

    def __init__(
        self,
        kind: CardKind | str,
        name: str,
        version: str,
        *,
        uid: str | None = ...,
    ) -> None:
        """Build a reference from its kind, name, and version.

        Args:
            kind: The Card kind, as a `CardKind` or its wire name.
            name: The Card name.
            version: The exact Card version.
            uid: The server-assigned Card UID, when known.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` if a field is invalid.
        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return this reference as its ``CardRef`` mapping, satisfying ``CardRefLike``.

        Returns:
            The ``kind``, ``name``, ``version``, and ``uid`` mapping.

        Raises:
            WyrdError: If the reference cannot be converted to Python values.
        """
        ...
    def __str__(self) -> str:
        """The canonical ``Kind/name@version[#uid]`` text of this reference."""
        ...
    def __repr__(self) -> str: ...

class VersionBump:
    """How declaration derives the next version.

    The server applies the bump to the latest stable registered version in
    the Card's version line when the Card's version is empty or a scope such
    as ``"1"`` or ``"1.2"``; with no declared version, the line's first
    version is used. A Card whose version is an exact ``major.minor.patch``
    pin declares that version and accepts no bump. `Major`, `Minor`, and
    `Patch` increment their component and clear pre-release and build
    metadata; `pre`, `build`, and `pre_build` set metadata on the current
    version without incrementing it.

    Example:
        ```python
        cards.model.declare(card, VersionBump.Minor)
        cards.model.declare(card, VersionBump.pre("rc.1"))
        cards.model.declare(card, VersionBump.pre_build("rc.1", "linux"))
        ```
    """

    Major: VersionBump
    Minor: VersionBump
    Patch: VersionBump

    @staticmethod
    def pre(identifier: str) -> VersionBump:
        """Set the SemVer pre-release identifier, such as ``"rc.1"``.

        Args:
            identifier: The pre-release identifier. The server validates it
                at declaration.

        Returns:
            The pre-release bump.
        """
        ...
    @staticmethod
    def build(metadata: str) -> VersionBump:
        """Set the SemVer build metadata, such as ``"sha-abc123"``.

        Args:
            metadata: The build metadata. The server validates it at
                declaration.

        Returns:
            The build-metadata bump.
        """
        ...
    @staticmethod
    def pre_build(pre: str, build: str) -> VersionBump:
        """Set both the pre-release identifier and the build metadata.

        Args:
            pre: The pre-release identifier, such as ``"rc.1"``.
            build: The build metadata, such as ``"linux"``.

        Returns:
            The combined bump.
        """
        ...

class DataSaveArgs:
    """Options forwarded to a `DataCard` interface during registration.

    These values are passed to the data interface's `save` implementation
    while Wyrd materializes the local artifact directory that is uploaded with
    the Card. They are not registry query options.
    """

    def __init__(
        self,
        values: Mapping[str, JsonValue] | None = ...,
        *,
        copy_bytes: bool | None = ...,
    ) -> None:
        """Collect data-interface save options.

        Args:
            values: interface-specific options, copied into a new dict;
                accepted keys depend on the interface. Omitted, no options.
            copy_bytes: for the manifest-backed image and text interfaces,
                ``True`` also copies the referenced files into the artifact;
                other built-in interfaces ignore it. Overrides a
                ``copy_bytes`` key in ``values``. Omitted, the key is not set.

        Raises:
            WyrdError: If ``values`` cannot be converted by ``dict()``.

        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface.

        Returns:
            A dict of the collected options.
        """
        ...

class ModelSaveArgs:
    """Options forwarded to a `ModelCard` interface during registration.

    Values control local artifact serialization. They do not change Card
    identity or version selection.
    """

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """Collect interface-specific options, copied into a new dict.

        Args:
            values: Interface-specific options; accepted keys depend on the
                interface. Omitted, the interface receives no options.

        Raises:
            WyrdError: If ``values`` cannot be converted by ``dict()``.
        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface.

        Returns:
            A dict of the collected options.
        """
        ...

class DataLoadArgs:
    """Options forwarded to a `DataCard` interface during `DataCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """Collect interface-specific options, copied into a new dict.

        Args:
            values: Interface-specific options; accepted keys depend on the
                interface. Omitted, the interface receives no options.

        Raises:
            WyrdError: If ``values`` cannot be converted by ``dict()``.
        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface.

        Returns:
            A dict of the collected options.
        """
        ...

class ModelLoadArgs:
    """Options forwarded to a `ModelCard` interface during `ModelCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """Collect interface-specific options, copied into a new dict.

        Args:
            values: Interface-specific options; accepted keys depend on the
                interface. Omitted, the interface receives no options.

        Raises:
            WyrdError: If ``values`` cannot be converted by ``dict()``.
        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface.

        Returns:
            A dict of the collected options.
        """
        ...

class Cards:
    """Tenant-scoped client for declared Wyrd Cards.

    `Cards` owns the connection and authentication context used by registry
    operations. The kind-specific properties are the normal API:

    ```python
    from wyrd import Cards

    cards = Cards()
    latest = cards.model.resolve_latest(name="fraud-model")
    card = cards.model.get(uid=latest.uid)
    ```

    `get` retrieves and validates the serialized Card envelope and downloads
    no model or data bytes. `pull` publishes a Service graph bundle that
    `WyrdState.from_path` loads into runtime objects.
    """

    def __init__(self, client: WyrdClient | None = None) -> None:
        """Build a handle; no IO or credential resolution happens here.

        The first server operation connects exactly as ``connect`` does, once
        per handle, and raises its errors.

        Args:
            client: the ``WyrdClient`` to act as, sharing its transport and
                token cache. Omitted, the ambient client resolves from
                ``[client]`` in the Wyrd ``config.toml``, then the
                environment, then the saved ``wyrd auth login``.

        """
        ...
    @staticmethod
    def connect(client: WyrdClient | None = None) -> Cards:
        """Build a handle and connect it now.

        Resolves the client and completes any credential exchange it needs; a
        bearer token or fresh cached token makes no request.

        Args:
            client: the ``WyrdClient`` to act as. Omitted, the ambient client
                resolves as for the constructor.

        Raises:
            WyrdError: ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves; ``WYRD_CLIENT_400_CONFIG_INVALID`` for an unreadable
                ``config.toml``; the server's error for a refused credential
                exchange.

        """
        ...
    @property
    def data(self) -> DataCardRegistry:
        """Return the typed registry view for `DataCard` operations."""
        ...

    @property
    def model(self) -> ModelCardRegistry:
        """Return the typed registry view for `ModelCard` operations."""
        ...

    @property
    def prompt(self) -> PromptCardRegistry:
        """Return the typed registry view for `PromptCard` operations."""
        ...

    def declare(
        self,
        card: DeclarableCard,
        version_bump: VersionBump = ...,
        save_args: DataSaveArgs | ModelSaveArgs | None = ...,
    ) -> DeclarationReceipt:
        """Declare a caller-owned Card and stamp its resolved identity.

        Declaration serializes the complete Card envelope, saves DataCard or
        ModelCard artifacts through the attached interface, uploads the
        resulting manifest, and waits for server completion. On success, the
        same `card` object is updated with the server-assigned `uid`, resolved
        version, and normalized identity fields; on failure it is unchanged.

        Use the kind-specific view when you want static type checking for the
        card argument. The unscoped method is useful when the Card kind is
        selected dynamically.

        Args:
            card: A `DataCard`, `ModelCard`, or `PromptCard`.
            version_bump: how the server derives the version; see
                `VersionBump`. Omitted, `VersionBump.Patch`. Passing one for
                a Card whose version is an exact pin is an error.
            save_args: `DataSaveArgs` for a `DataCard` or `ModelSaveArgs` for
                a `ModelCard`, forwarded to its interface. A `PromptCard`
                accepts none.

        Returns:
            A receipt containing the resolved root reference and one outcome
            for each declared Card.

        Raises:
            WyrdError: If `card` is not a supported native holder, the
                arguments do not match its kind, required interface data is
                missing, serialization or artifact saving fails, the server
                rejects the declaration, or completion fails.

        Example:
            ```python
            card = PromptCard(
                Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
                name="welcome",
                version="0.1.0",
            )
            receipt = cards.prompt.declare(card)
            assert card.uid == receipt.root.uid
            ```
        """
        ...

    def declare_from_path(self, path: str | PathLike[str]) -> DeclarationReceipt:
        """Declare a Card from a Card file or a bundle directory.

        This is the file-based declaration path. For Python-authored
        `DataCard`, `ModelCard`, and `PromptCard` objects, prefer `declare`
        so the card interface and caller-owned object are part of the same
        operation.

        Args:
            path: A Card file or a bundle directory.

        Returns:
            A receipt for the completed declaration.

        Raises:
            WyrdError: If the bundle is invalid, its artifacts cannot be read,
                or declaration does not complete.
        """
        ...

    def upload_artifact(
        self,
        source: str | PathLike[str],
        metadata: Mapping[str, object],
        *,
        version_bump: VersionBump | None = None,
        spec: Mapping[str, object] | None = None,
        extract: bool = False,
    ) -> CardRef:
        """Publish one file or directory as an Artifact Card.

        A file keeps its basename; a directory keeps every regular file at
        its relative path. Nothing is parsed unless ``extract`` is set: then
        each supported document ``P`` (PDF, DOCX, XLSX, PPTX, CSV, JSON, text,
        Markdown, HTML) is extracted locally and published beside its
        original as ``extracted/P.json``, and ``spec.metadata.extraction``
        records the extractor version, the source/output pairs, and the
        skipped paths. The source tree is never written.

        Example:
            ```python
            ref = cards.upload_artifact(
                "policies/",
                {"name": "refund-policies", "version": "1.0.0"},
                spec={"artifact_kind": "policies"},
                extract=True,
            )
            ```

        Args:
            source: The regular file or directory to publish.
            metadata: The Card metadata: ``name``, an optional exact
                ``version`` or version scope (omitted, the server assigns
                one), and any ``labels`` or ``annotations``.
            version_bump: How the server bumps the latest version; never
                read from ``metadata``.
            spec: The Artifact spec; defaults to ``{"artifact_kind": "file"}``.
            extract: Prepare extraction outputs before declaring.

        Returns:
            The exact Active reference, UID included.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` naming the field for
                invalid metadata, name, or spec, before anything is sent;
                otherwise for a blank kind, an invalid source, a symlink, an
                output path that collides with a source, an authored
                extraction report, a document the extractor refuses, or a
                failed declaration.
        """
        ...

    def list_artifacts(self, card_ref: CardRef) -> builtins.list[dict[str, object]]:
        """List the files a declared Card holds, from the server inventory.

        Args:
            card_ref: Exact reference of the Card.

        Returns:
            One dict per file with ``relative_path``, ``size_bytes``, base64
            ``sha256``, and ``content_type``.

        Raises:
            WyrdError: When the Card is absent or unauthorized.
        """
        ...

    def get(self, card_ref: CardRef) -> CardEnvelope:
        """Fetch one declared Card by exact reference as its complete envelope.

        The result is the native `CardEnvelope`: `card_ref` is the exact
        UID-pinned identity, `kind` is the `CardKind`, and `metadata`, `spec`,
        `relationships`, and `status` are JSON-compatible mappings of the
        server envelope. A registry read belongs to no hydrated graph, so
        `aliases` is `()`. No artifact bytes are downloaded and nothing is
        hydrated.

        Args:
            card_ref: Exact reference carrying its version.

        Returns:
            The declared envelope with server-derived relationships and
            status.

        Raises:
            WyrdError: If the Card is absent or the server request fails.

        Example:
            ```python
            card = cards.get(CardRef("Verifier", "churn-drift", "1.0.0"))
            if card.kind == CardKind.Verifier and card.status is not None:
                verification = card.status.get("verification")
            ```
        """
        ...

    def list(
        self,
        *,
        kind: CardKind | str | None = None,
        name: str | None = ...,
        version_range: str | None = ...,
        status: str | None = ...,
        filter: str | None = ...,
        include_prerelease: bool = ...,
        limit: int | None = ...,
        cursor: str | None = ...,
    ) -> CardList:
        """List metadata-only summaries of declared Cards of any kind.

        Args:
            kind: Only Cards of this kind. Omitted, every kind.
            name: Only Cards with this name.
            version_range: Only versions in this SemVer range.
            status: Only this lifecycle status, as for
                ``DataCardRegistry.list()``.
            filter: A metadata query over labels, annotations, and reserved
                columns.
            include_prerelease: Include pre-release versions. Default
                ``False``.
            limit: Page size from 1 to 200. Omitted, 50.
            cursor: The `next_cursor` of the previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: A validation error for an unknown `kind` or another
                invalid argument, as for ``DataCardRegistry.list()``.
        """
        ...

    def resolve_latest(self, kind: CardKind | str, name: str) -> CardRef:
        """Resolve the latest Active version of one named Card.

        Args:
            kind: The Card kind.
            name: The Card name.

        Returns:
            The exact reference of the latest Active version.

        Raises:
            WyrdError: If `kind` or `name` is invalid, or no Active
                Card matches.
        """
        ...

    def delete(self, card_ref: CardRef) -> None:
        """Delete one declared Card, of any kind, by exact reference.

        The Card is marked ``deleted`` and its stored artifacts are removed.

        Args:
            card_ref: Exact reference carrying its version.

        Raises:
            WyrdError: ``WYRD_REGISTRY_404_CARD_NOT_FOUND`` when no Card
                matches; ``WYRD_SPEC_409_CONFLICT`` when the Card is not
                Active or another visible Card references it.
        """
        ...

    def pull(
        self,
        card_ref: CardRef,
        destination: str | PathLike[str],
        metadata_only: bool = False,
    ) -> PullSummary:
        """Pull a declared Card's reachable graph into a local bundle.

        The bundle is published atomically under `destination`, ready for
        ``WyrdState.from_path``.

        Args:
            card_ref: The root Card's exact reference.
            destination: The bundle directory to publish.
            metadata_only: ``True`` writes Card metadata only; ``False``
                also downloads and verifies every artifact.

        Returns:
            What was published.

        Raises:
            WyrdError: If the graph cannot be resolved, the destination is
                invalid, an artifact cannot be downloaded or verified, or the
                bundle cannot be published.
        """
        ...

class PullSummary:
    """Result of one published ``Cards.pull``."""

    @property
    def root(self) -> CardRef:
        """The exact resolved root reference."""
        ...
    @property
    def destination(self) -> Path:
        """The published bundle directory."""
        ...
    @property
    def mode(self) -> Literal["complete", "metadata"]:
        """``"complete"`` or ``"metadata"``."""
        ...
    @property
    def card_count(self) -> int:
        """Number of unique Cards in the bundle."""
        ...
    @property
    def artifact_count(self) -> int:
        """Number of server-owned artifact inventory entries."""
        ...
    @property
    def downloaded_artifact_count(self) -> int:
        """Number of artifact payloads downloaded and verified."""
        ...

class DataCardRegistry:
    """Typed operations for declared `DataCard` objects.

    Obtain this view from `Cards.data`. It uses the connection and tenant
    context from the parent `Cards` object:

    ```python
    cards = Cards()
    data_card = cards.data.get(name="training-data")
    data_card.load()
    ```
    """

    def declare(
        self,
        card: DataCard[Any],
        version_bump: VersionBump = ...,
        save_args: DataSaveArgs | None = ...,
    ) -> DeclarationReceipt:
        """Declare a `DataCard` and upload its saved data artifacts.

        As ``Cards.declare()``, restricted to `DataCard`. `save_args` is
        passed to the card's data interface; it is not sent as registry
        metadata.

        Args:
            card: The `DataCard` to declare; it receives the server identity.
            version_bump: How the server derives the version; see
                `VersionBump`. Omitted, `VersionBump.Patch`.
            save_args: `DataSaveArgs` forwarded to the data interface.

        Returns:
            A receipt containing the resolved root reference and one outcome
            for each declared Card.

        Raises:
            WyrdError: If `card` is not a `DataCard` or has no usable data
                interface, or saving, serialization, upload, validation, or
                server completion fails.
        """
        ...
    @overload
    def get(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: None = ...,
    ) -> DataCard:
        """Retrieve a `DataCard` whose built-in interface is rebuilt from its metadata."""
        ...
    @overload
    def get(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: _DataInterfaceT | type[_DataInterfaceT],
    ) -> DataCard[_DataInterfaceT]:
        """Retrieve and validate one complete `DataCard` envelope.

        Pass a `CardRef` alone, `uid` for an exact lookup, or `name` for a
        named one.

        Args:
            card_ref: Exact `CardRef` of a Data Card, used alone in place of
                the other selector fields.
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector; supplied `name` and
                `version` are checked against the returned Card.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omitted, the latest Active version.
            interface: Built-in or custom `DataInterface` instance or class
                used to rebuild the Python interface from Card metadata.
                Required when the serialized Card uses a custom interface.

        Returns:
            A native `DataCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: ``WYRD_REGISTRY_404_CARD_NOT_FOUND`` when no Card
                matches, or a validation error for an invalid selector, an
                envelope that fails validation, or a missing custom interface.
        """
        ...
    def list(
        self,
        *,
        name: str | None = ...,
        version_range: str | None = ...,
        status: str | None = ...,
        filter: str | None = ...,
        include_prerelease: bool = ...,
        limit: int | None = ...,
        cursor: str | None = ...,
    ) -> CardList:
        """List metadata-only DataCard summaries.

        Results contain identity, hashes, lifecycle status, and exact
        references. Wyrd does not download Card envelopes or data artifacts
        for this operation. Use `get` when you need the full holder.

        Args:
            name: Only Cards with this name.
            version_range: Only versions in this SemVer range, such as
                ``"^1.2"``, ``"1.*"``, or ``">=1.0,<2.0"``.
            status: Only this lifecycle status, case-insensitive:
                ``"pending"``, ``"active"``, ``"deprecated"``, ``"failed"``,
                ``"expired"``, or ``"deleted"``. Omitted, every status except
                ``"deleted"``.
            filter: A metadata query over labels, annotations, and reserved
                columns, such as ``'labels.env = "prod"'``.
            include_prerelease: Include pre-release versions. Default
                ``False``.
            limit: Page size from 1 to 200. Omitted, 50.
            cursor: The `next_cursor` of the previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: ``WYRD_REGISTRY_400_LIST_LIMIT_OUT_OF_RANGE`` for a bad
                `limit`, ``WYRD_QUERY_400_INVALID_SYNTAX`` for a malformed
                `filter`, or a validation error for another invalid argument.
        """
        ...

    def resolve_latest(self, *, name: str) -> CardRef:
        """Resolve the latest Active DataCard version to an exact reference.

        This returns identity only. Call `get(reference)` to retrieve the
        complete card envelope.

        Args:
            name: The Card name.

        Returns:
            The exact reference of the latest Active version.

        Raises:
            WyrdError: If `name` is invalid or no Active Card
                matches.
        """
        ...

    def delete(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a declared DataCard and its stored artifacts.

        The Card is marked ``deleted`` and its artifacts are removed. Deleting
        an already deleted Card is a no-op. Provide a `CardRef` alone or
        `uid` for an exact deletion; otherwise provide `name` and the exact
        `version`.

        Args:
            card_ref: Exact `CardRef` of a Data Card, used alone in place of
                the other selector fields.
            uid: Exact server-assigned Card UID.
            name: Card name, required when `uid` is omitted.
            version: Exact version, required with `name`.

        Raises:
            WyrdError: ``WYRD_REGISTRY_400_VERSION_REQUIRED`` for a named
                selector without `version`; ``WYRD_REGISTRY_404_CARD_NOT_FOUND``
                when no Card matches; ``WYRD_SPEC_409_CONFLICT`` when the Card
                is not Active or another visible Card references it.
        """
        ...

class ModelCardRegistry:
    """Typed operations for declared `ModelCard` objects.

    Obtain this view from `Cards.model`. `get` rebuilds the holder from the
    server-stored JSON and downloads no model bytes.
    """

    def declare(
        self,
        card: ModelCard[Any],
        version_bump: VersionBump = ...,
        save_args: ModelSaveArgs | None = ...,
    ) -> DeclarationReceipt:
        """Declare a `ModelCard` and upload its saved model artifacts.

        As ``Cards.declare()``, restricted to `ModelCard`. `save_args` is
        passed to the card's model interface.

        Args:
            card: The `ModelCard` to declare; it receives the server identity.
            version_bump: How the server derives the version; see
                `VersionBump`. Omitted, `VersionBump.Patch`.
            save_args: `ModelSaveArgs` forwarded to the model interface.

        Returns:
            A receipt containing the resolved root reference and one outcome
            for each declared Card.

        Raises:
            WyrdError: If `card` is not a `ModelCard`, the model interface is
                missing, or saving, serialization, upload, validation, or
                server completion fails.
        """
        ...
    @overload
    def get(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: None = ...,
    ) -> ModelCard:
        """Retrieve a `ModelCard` whose built-in interface is rebuilt from its metadata."""
        ...
    @overload
    def get(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: _ModelInterfaceT | type[_ModelInterfaceT],
    ) -> ModelCard[_ModelInterfaceT]:
        """Retrieve and validate one complete `ModelCard` envelope.

        As ``DataCardRegistry.get()``, with a `ModelInterface` for
        `interface`.

        Args:
            card_ref: Exact `CardRef` of a Model Card, used alone in place of
                the other selector fields.
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omitted, the latest Active version.
            interface: Built-in or custom `ModelInterface` instance or class
                used to rebuild the Python interface from Card metadata.

        Returns:
            A native `ModelCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: As for ``DataCardRegistry.get()``.
        """
        ...
    def list(
        self,
        *,
        name: str | None = ...,
        version_range: str | None = ...,
        status: str | None = ...,
        filter: str | None = ...,
        include_prerelease: bool = ...,
        limit: int | None = ...,
        cursor: str | None = ...,
    ) -> CardList:
        """List metadata-only ModelCard summaries.

        As ``DataCardRegistry.list()``. No model bytes are fetched.

        Args:
            name: Only Cards with this name.
            version_range: Only versions in this SemVer range.
            status: Only this lifecycle status, as for
                ``DataCardRegistry.list()``.
            filter: A metadata query over labels, annotations, and reserved
                columns.
            include_prerelease: Include pre-release versions. Default
                ``False``.
            limit: Page size from 1 to 200. Omitted, 50.
            cursor: The `next_cursor` of the previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: A validation error for an invalid argument, as for
                ``DataCardRegistry.list()``.
        """
        ...

    def resolve_latest(self, *, name: str) -> CardRef:
        """Resolve the latest Active ModelCard version.

        As ``DataCardRegistry.resolve_latest()``.

        Args:
            name: The Card name.

        Returns:
            The exact reference of the latest Active version.

        Raises:
            WyrdError: If `name` is invalid or no Active Card
                matches.
        """
        ...

    def delete(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a declared ModelCard and its stored artifacts.

        As ``DataCardRegistry.delete()``.

        Args:
            card_ref: Exact `CardRef` of a Model Card, used alone in place of
                the other selector fields.
            uid: Exact server-assigned Card UID.
            name: Card name, required when `uid` is omitted.
            version: Exact version, required with `name`.

        Raises:
            WyrdError: As for ``DataCardRegistry.delete()``.
        """
        ...

class PromptCardRegistry:
    """Typed operations for declared `PromptCard` objects.

    Obtain this view from `Cards.prompt`. Prompt cards have no artifact save
    options; declaration persists the serialized prompt Card directly.
    """

    def declare(
        self,
        card: PromptCard,
        version_bump: VersionBump = ...,
    ) -> DeclarationReceipt:
        """Declare a `PromptCard` and update its server identity.

        As ``Cards.declare()``, restricted to `PromptCard`.

        Args:
            card: The `PromptCard` to declare; it receives the server identity.
            version_bump: How the server derives the version; see
                `VersionBump`. Omitted, `VersionBump.Patch`.

        Returns:
            A receipt containing the resolved root reference and one outcome
            for each declared Card.

        Raises:
            WyrdError: If `card` is not a `PromptCard`, or serialization,
                validation, upload, or server completion fails.
        """
        ...
    def get(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> PromptCard:
        """Retrieve and validate one complete `PromptCard` envelope.

        As ``DataCardRegistry.get()`` for `uid`, `name`, and
        `version`. Prompt cards have no separate artifact hydration step.

        Args:
            card_ref: Exact `CardRef` of a Prompt Card, used alone in place
                of the other selector fields.
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omitted, the latest Active version.

        Returns:
            A native `PromptCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: ``WYRD_REGISTRY_404_CARD_NOT_FOUND`` when no Card
                matches, or a validation error for an invalid selector or an
                envelope that fails validation.
        """
        ...
    def list(
        self,
        *,
        name: str | None = ...,
        version_range: str | None = ...,
        status: str | None = ...,
        filter: str | None = ...,
        include_prerelease: bool = ...,
        limit: int | None = ...,
        cursor: str | None = ...,
    ) -> CardList:
        """List metadata-only PromptCard summaries.

        As ``DataCardRegistry.list()``.

        Args:
            name: Only Cards with this name.
            version_range: Only versions in this SemVer range.
            status: Only this lifecycle status, as for
                ``DataCardRegistry.list()``.
            filter: A metadata query over labels, annotations, and reserved
                columns.
            include_prerelease: Include pre-release versions. Default
                ``False``.
            limit: Page size from 1 to 200. Omitted, 50.
            cursor: The `next_cursor` of the previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: A validation error for an invalid argument, as for
                ``DataCardRegistry.list()``.
        """
        ...

    def resolve_latest(self, *, name: str) -> CardRef:
        """Resolve the latest Active PromptCard version.

        As ``DataCardRegistry.resolve_latest()``.

        Args:
            name: The Card name.

        Returns:
            The exact reference of the latest Active version.

        Raises:
            WyrdError: If `name` is invalid or no Active Card
                matches.
        """
        ...

    def delete(
        self,
        card_ref: CardRef | None = None,
        *,
        uid: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a declared PromptCard and its stored Card envelope.

        As ``DataCardRegistry.delete()``.

        Args:
            card_ref: Exact `CardRef` of a Prompt Card, used alone in place of
                the other selector fields.
            uid: Exact server-assigned Card UID.
            name: Card name, required when `uid` is omitted.
            version: Exact version, required with `name`.

        Raises:
            WyrdError: As for ``DataCardRegistry.delete()``.
        """
        ...

class CardSummary:
    """Metadata-only result returned by a kind-specific `list` operation.

    A summary contains enough information to select an exact Card, but it does
    not contain the serialized envelope or local data/model artifacts.

    Attributes:
        spec_hash: BLAKE3 hash of the Card spec.
        artifact_hash: BLAKE3 hash of the artifact manifest, or `None` for a
            Card without artifacts.
        status: lifecycle status: ``"pending"``, ``"active"``,
            ``"deprecated"``, ``"failed"``, ``"expired"``, or ``"deleted"``.
        created_at: RFC 3339 creation time.
        updated_at: RFC 3339 time of the last update.
    """

    uid: str
    kind: CardKind
    name: str
    version: str
    spec_hash: str
    artifact_hash: str | None
    status: str
    created_at: str
    updated_at: str
    card_ref: CardRef

class CardList:
    """One cursor-paginated page of metadata-only Card summaries.

    `refs` holds each summary's `card_ref` in the same order. Request the
    next page by passing `next_cursor` as `cursor` to `list`; it is `None` on
    the last page.
    """

    items: list[CardSummary]
    refs: list[CardRef]
    next_cursor: str | None

class DeclarationOutcome:
    """Server-derived result for one declared Card.

    The outcome includes the exact reference and hashes that the server
    accepted after validation and artifact completion.

    Attributes:
        status: the Card's lifecycle status, as in `CardSummary.status`.
        outcome: ``"declared"`` for a new version, ``"deduplicated"`` when
            an Active Card with identical content already existed and was
            reused, or ``"idempotent_noop"`` when the same exact version was
            already declared with identical content.
        card_blob_uri: where the Card envelope was stored, when one was
            written.
    """

    card_ref: CardRef
    spec_hash: str
    artifact_hash: str | None
    status: str
    outcome: str
    card_blob_uri: str | None

class DeclarationReceipt:
    """Result returned after declaration and artifact completion succeed."""

    @property
    def root(self) -> CardRef:
        """The server-resolved reference of the Card you declared."""
        ...
    @property
    def outcomes(self) -> list[DeclarationOutcome]:
        """One outcome per Card in the declaration, root included, dependencies first."""
        ...

__all__ = [
    "AgentCard",
    "Card",
    "CardKind",
    "CardRef",
    "VersionBump",
    "DataSaveArgs",
    "ModelSaveArgs",
    "DataLoadArgs",
    "ModelLoadArgs",
    "Cards",
    "DataCardRegistry",
    "ModelCardRegistry",
    "PromptCardRegistry",
    "CardSummary",
    "CardList",
    "DeclarationOutcome",
    "DeclarationReceipt",
    "PullSummary",
]
