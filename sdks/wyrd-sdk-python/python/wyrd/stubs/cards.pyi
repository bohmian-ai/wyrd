#### begin imports ####

from collections.abc import Mapping
from typing import Protocol, TypeAlias, overload

from ..agent import Workflow
from ..data import DataCard, DataInterface
from ..model import ModelCard, ModelInterface
from ..prompt import Prompt, PromptCard, PromptReference

#### end of imports ####

JsonScalar: TypeAlias = str | int | float | bool | None
JsonValue: TypeAlias = JsonScalar | list[JsonValue] | dict[str, JsonValue]

class Card(Protocol):
    """Shared authoring capability implemented by native card holders.

    `DataCard`, `ModelCard`, and `PromptCard` are the registerable public
    implementations. `AgentCard` is an envelope holder, not a registry input.
    Pass one of those objects to `Cards.register`; do not construct `Card`
    directly.
    """

    space: str
    name: str
    version: str
    uid: str

    def _to_card_envelope_json(self) -> str:
        """Return the holder's single Wyrd envelope conversion."""
        ...

RegisterableCard: TypeAlias = DataCard | ModelCard | PromptCard

class AgentCard:
    """Local Agent Card holder backed by the native Agent envelope.

    Use `PromptReference.inline(prompt)` for an inline prompt or
    `PromptReference.card(...)` for a registered Prompt Card reference. The
    holder contains no runtime, registry, storage, or artifact state.

    Attributes:
        spec: the durable Agent spec as a dict.
        cascade_children: the referenced Prompt Card when the prompt is
            card-backed; empty for an inline prompt.
        prompt_ref: the durable prompt reference.
        prompt: the inline `Prompt`, or `None` while the prompt is a
            reference to a registered Prompt Card.
        kind: the Card kind, always ``"Agent"``.
    """

    space: str
    name: str
    version: str
    uid: str
    labels: dict[str, str]
    annotations: dict[str, str]
    spec: dict[str, JsonValue]
    cascade_children: list[CardRef]
    prompt_ref: PromptReference
    prompt: Prompt | None
    kind: str

    def __init__(
        self,
        prompt: PromptReference,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        uid: str | None = ...,
        labels: Mapping[str, str] | None = ...,
        annotations: Mapping[str, str] | None = ...,
    ) -> None:
        """Create an Agent Card from one prompt reference.

        Args:
            prompt: the inline prompt or registered Prompt Card reference.
            space: the Card space. Omitted, the workspace ``wyrd.toml``
                ``[kind.Agent]`` or ``[defaults]`` space applies, else
                ``"default"``.
            name: the Card name. ``"agent"`` if omitted.
            version: the semantic version. ``"0.1.0"`` if omitted.
            uid: the Card UID. A new UUIDv7 is generated if omitted.
            labels: queryable labels, merged per key with the workspace
                ``wyrd.toml`` defaults; your keys win.
            annotations: free-form annotations, merged like ``labels``.

        Raises:
            WyrdError: for an invalid label or annotation, or identity values
                the Agent envelope rejects.

        """
        ...

    def model_dump(self) -> dict[str, JsonValue]:
        """Return the complete Agent Card envelope as a dictionary."""
        ...

    def model_dump_json(self) -> str:
        """Return the complete Agent Card envelope as JSON."""
        ...

    @staticmethod
    def model_validate_json(json_string: str) -> AgentCard:
        """Hydrate an Agent Card from a complete persisted envelope.

        Args:
            json_string: an envelope as produced by ``model_dump_json()``.

        Raises:
            WyrdError: when the JSON is not a complete Agent Card envelope or
                lacks ``metadata.uid``.

        """
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

class CardKind:
    """The kind of a registered Wyrd Card.

    Kind-specific registry views use these values internally. Most Python
    callers use `cards.data`, `cards.model`, or `cards.prompt`, which already
    select the kind and return the matching card type. `External` marks a
    foreign schema descriptor and cannot be registered.
    """

    Data: CardKind
    Model: CardKind
    Experiment: CardKind
    Prompt: CardKind
    Agent: CardKind
    Workflow: CardKind
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
    """Reference to one exact registered Card.

    A reference identifies a kind, space, name, and version. A server-assigned
    `uid` is present after registration. Use a `CardRef` from `CardSummary` or
    `RegistrationReceipt` when you need to pass an exact identity between
    operations.
    """

    kind: CardKind
    name: str
    version: str
    space: str
    uid: str | None

    def __init__(
        self,
        kind: CardKind | str,
        name: str,
        version: str,
        *,
        space: str,
        uid: str | None = ...,
    ) -> None:
        """Build a reference from its identity components.

        Args:
            kind: a `CardKind` or its wire name, such as ``"Model"``.
            name: the Card name: 3 to 64 characters, starting with a lowercase
                letter, then lowercase letters, digits, ``_``, or ``-``.
            version: an exact semantic version such as ``"1.2.0"``; a range is
                rejected.
            space: the Card space, under the same rules as ``name``.
            uid: the server-assigned UUIDv7. Omitted or ``""``, the reference
                is unresolved.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an unknown kind or an
                invalid name, version, space, or UID.

        """
        ...
    def __repr__(self) -> str: ...

class VersionBump:
    """How registration derives the next version.

    The server applies the bump to the latest stable registered version in
    the Card's version line when the Card's version is empty or a scope such
    as ``"1"`` or ``"1.2"``; with no registered version, the line's first
    version is used. A Card whose version is an exact ``major.minor.patch``
    pin registers that version and accepts no bump. `Major`, `Minor`, and
    `Patch` increment their component and clear pre-release and build
    metadata; `pre`, `build`, and `pre_build` set metadata on the current
    version without incrementing it.

    Example:
        ```python
        cards.model.register(card, VersionBump.Minor)
        cards.model.register(card, VersionBump.pre("rc.1"))
        cards.model.register(card, VersionBump.pre_build("rc.1", "linux"))
        ```
    """

    Major: VersionBump
    Minor: VersionBump
    Patch: VersionBump

    @staticmethod
    def pre(identifier: str) -> VersionBump:
        """Set the SemVer pre-release identifier, such as ``"rc.1"``."""
        ...
    @staticmethod
    def build(metadata: str) -> VersionBump:
        """Set the SemVer build metadata, such as ``"sha-abc123"``."""
        ...
    @staticmethod
    def pre_build(pre: str, build: str) -> VersionBump:
        """Set both the pre-release identifier and the build metadata."""
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

        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface."""
        ...

class ModelSaveArgs:
    """Options forwarded to a `ModelCard` interface during registration.

    Values control local artifact serialization. They do not change Card
    identity or version selection.
    """

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """Collect interface-specific options, copied into a new dict.

        Omitted, the interface receives no options.
        """
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface."""
        ...

class DataLoadArgs:
    """Options forwarded to a `DataCard` interface during `DataCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """As ``ModelSaveArgs()``."""
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface."""
        ...

class ModelLoadArgs:
    """Options forwarded to a `ModelCard` interface during `ModelCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None:
        """As ``ModelSaveArgs()``."""
        ...
    def to_dict(self) -> dict[str, JsonValue]:
        """Return the options dict handed to the interface."""
        ...

class Cards:
    """Tenant-scoped client for registered Wyrd Cards.

    `Cards` owns the connection and authentication context used by registry
    operations. The kind-specific properties are the normal API:

    ```python
    from wyrd import Cards

    cards = Cards()
    latest = cards.model.resolve_latest(space="risk", name="fraud-model")
    card = cards.model.get(uid=latest.uid)
    card.load()
    ```

    `get` retrieves and validates the serialized Card envelope. It does not
    download model or data bytes unless `eager_load=True`; otherwise call
    `ModelCard.load` or `DataCard.load` afterward to hydrate those artifacts.
    """

    def __init__(
        self,
        server_url: str | None = ...,
        credential: str | None = ...,
    ) -> None:
        """Build a registry client. No network call or token exchange happens here.

        Args:
            server_url: the Wyrd server URL. Resolved from ``[client]``
                ``http_url`` in the Wyrd ``config.toml``
                (``~/.config/wyrd/config.toml`` by default), then
                ``WYRD_SERVER_URL``, then ``http://localhost:8080`` if
                omitted.
            credential: the API key or bearer token. Resolved as for
                ``WyrdClient()`` if omitted.

        Raises:
            WyrdError: ``WYRD_CLIENT_400_CONFIG_INVALID`` for an empty
                ``server_url`` or an unreadable ``config.toml``;
                ``WYRD_CLIENT_401_NO_CREDENTIALS`` when no credential
                resolves.

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

    @property
    def workflow(self) -> WorkflowCards:
        """Return the typed view for loading registered Workflows."""
        ...

    def register(
        self,
        card: RegisterableCard,
        version_bump: VersionBump = ...,
        save_args: DataSaveArgs | ModelSaveArgs | None = ...,
    ) -> RegistrationReceipt:
        """Register a caller-owned Card and stamp its resolved identity.

        Registration serializes the complete Card envelope, saves DataCard or
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
            for each registered Card.

        Raises:
            WyrdError: If `card` is not a supported native holder, the
                arguments do not match its kind, required interface data is
                missing, serialization or artifact saving fails, the server
                rejects the registration, or completion fails.

        Example:
            ```python
            card = PromptCard(
                Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
                space="growth",
                name="welcome",
                version="0.1.0",
            )
            receipt = cards.prompt.register(card)
            assert card.uid == receipt.root.uid
            ```
        """
        ...

    def register_from_path(self, path: str) -> RegistrationReceipt:
        """Register a declarative Card bundle from a local directory.

        This is the file-based registration path. For Python-authored
        `DataCard`, `ModelCard`, and `PromptCard` objects, prefer `register`
        so the card interface and caller-owned object are part of the same
        operation.

        Args:
            path: Directory containing the declarative Card bundle.

        Returns:
            A receipt for the completed registration.

        Raises:
            WyrdError: If the bundle is invalid, its artifacts cannot be read,
                or registration does not complete.
        """
        ...

class DataCardRegistry:
    """Typed operations for registered `DataCard` objects.

    Obtain this view from `Cards.data`. It uses the connection and tenant
    context from the parent `Cards` object:

    ```python
    cards = Cards()
    data_card = cards.data.get(space="risk", name="training-data")
    data_card.load()
    ```
    """

    def register(
        self,
        card: DataCard,
        version_bump: VersionBump = ...,
        save_args: DataSaveArgs | None = ...,
    ) -> RegistrationReceipt:
        """Register a `DataCard` and upload its saved data artifacts.

        As ``Cards.register()``, restricted to `DataCard`. `save_args` is
        passed to the card's data interface; it is not sent as registry
        metadata.

        Raises:
            WyrdError: If `card` is not a `DataCard` or has no usable data
                interface, or saving, serialization, upload, validation, or
                server completion fails.
        """
        ...
    def get(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: DataInterface | type[DataInterface] | None = ...,
        eager_load: bool = ...,
        load_kwargs: DataLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> DataCard:
        """Retrieve and validate one complete `DataCard` envelope.

        Pass `uid` for an exact lookup, or `space` and `name` for a named one.

        Args:
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector; supplied `space`, `name`,
                and `version` are checked against the returned Card.
            space: Card space, required when `uid` is omitted.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omitted, the latest Active version.
            interface: Built-in or custom `DataInterface` instance or class
                used to rebuild the Python interface from Card metadata.
                Required when the serialized Card uses a custom interface.
            eager_load: ``True`` downloads the verified artifacts and loads
                the data before returning. Default ``False`` defers that to
                `DataCard.load`.
            load_kwargs: `DataLoadArgs` or a JSON-compatible mapping forwarded
                to the load; used only with ``eager_load=True``.

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
        space: str | None = ...,
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
            space: Only Cards in this space.
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

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest Active DataCard version to an exact reference.

        This returns identity only. Call `get(uid=reference.uid)` to retrieve
        the complete card envelope.

        Raises:
            WyrdError: If `space` or `name` is invalid or no Active Card
                matches.
        """
        ...

    def delete(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a registered DataCard and its stored artifacts.

        The Card is marked ``deleted`` and its artifacts are removed. Deleting
        an already deleted Card is a no-op. Provide `uid` for an exact
        deletion; otherwise provide `space`, `name`, and the exact `version`.

        Raises:
            WyrdError: ``WYRD_REGISTRY_400_VERSION_REQUIRED`` for a named
                selector without `version`; ``WYRD_REGISTRY_404_CARD_NOT_FOUND``
                when no Card matches; ``WYRD_SPEC_409_CONFLICT`` when the Card
                is not Active or another visible Card references it.
        """
        ...

class ModelCardRegistry:
    """Typed operations for registered `ModelCard` objects.

    Obtain this view from `Cards.model`. `get` rebuilds the holder from the
    server-stored JSON; `ModelCard.load` downloads and hydrates model bytes.
    """

    def register(
        self,
        card: ModelCard,
        version_bump: VersionBump = ...,
        save_args: ModelSaveArgs | None = ...,
    ) -> RegistrationReceipt:
        """Register a `ModelCard` and upload its saved model artifacts.

        As ``Cards.register()``, restricted to `ModelCard`. `save_args` is
        passed to the card's model interface.

        Raises:
            WyrdError: If `card` is not a `ModelCard`, the model interface is
                missing, or saving, serialization, upload, validation, or
                server completion fails.
        """
        ...
    def get(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
        interface: ModelInterface | type[ModelInterface] | None = ...,
        eager_load: bool = ...,
        load_kwargs: ModelLoadArgs | Mapping[str, JsonValue] | None = ...,
    ) -> ModelCard:
        """Retrieve and validate one complete `ModelCard` envelope.

        As ``DataCardRegistry.get()``, with a `ModelInterface` for
        `interface`, `ModelLoadArgs` for `load_kwargs`, and `ModelCard.load`
        as the deferred load.
        """
        ...
    def list(
        self,
        *,
        space: str | None = ...,
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
        """
        ...

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest Active ModelCard version.

        As ``DataCardRegistry.resolve_latest()``.
        """
        ...

    def delete(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a registered ModelCard and its stored artifacts.

        As ``DataCardRegistry.delete()``.
        """
        ...

class WorkflowCards:
    """Load registered Workflows from the Wyrd registry.

    Obtain this view from `Cards.workflow`. A loaded Workflow runs the exact
    Agent and Prompt versions it was registered with; registering a newer
    Agent version later does not change it.

    Example:
        ```python
        from wyrd.cards import Cards

        cards = Cards()
        workflow = cards.workflow.load(
            space="reviews", name="code-review", version="1.0.0"
        )
        same = cards.workflow.load(uid=workflow_uid)
        ```
    """

    @overload
    def load(self, *, space: str, name: str, version: str) -> Workflow:
        """Load one registered Workflow by its exact identity.

        Args:
            space (str): Space the Workflow is registered in.
            name (str): Workflow name.
            version (str): Exact registered version, such as `"1.0.0"`.
                Version ranges and omitted versions are refused.

        Returns:
            Workflow: A validated, runnable `wyrd.agent.Workflow` whose Agents
                and Prompts are the exact versions it was registered with.

        Raises:
            WyrdError: `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no such
                Workflow exists or it was deleted;
                `WYRD_PERMISSION_403_DENIED_RBAC` when the credential cannot
                read Cards; `WYRD_WORKFLOW_400_INVALID_CARD_REF` when
                `version` is missing or a field is malformed; a Workflow validation error
                when the stored graph is invalid.
        """
        ...

    @overload
    def load(self, *, uid: str) -> Workflow:
        """Load one registered Workflow by its UID.

        Args:
            uid (str): The Workflow Card's UID, as returned by registration.

        Returns:
            Workflow: A validated, runnable `wyrd.agent.Workflow` whose Agents
                and Prompts are the exact versions it was registered with.

        Raises:
            WyrdError: `WYRD_REGISTRY_404_CARD_NOT_FOUND` when no Workflow has
                this UID; `WYRD_PERMISSION_403_DENIED_RBAC` when the
                credential cannot read Cards;
                `WYRD_WORKFLOW_400_INVALID_CARD_REF` when `uid` is combined
                with `space`, `name`, or `version`.
        """
        ...

class PromptCardRegistry:
    """Typed operations for registered `PromptCard` objects.

    Obtain this view from `Cards.prompt`. Prompt cards have no artifact save
    options; registration persists the serialized prompt Card directly.
    """

    def register(
        self,
        card: PromptCard,
        version_bump: VersionBump = ...,
    ) -> RegistrationReceipt:
        """Register a `PromptCard` and update its server identity.

        As ``Cards.register()``, restricted to `PromptCard`.

        Raises:
            WyrdError: If `card` is not a `PromptCard`, or serialization,
                validation, upload, or server completion fails.
        """
        ...
    def get(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> PromptCard:
        """Retrieve and validate one complete `PromptCard` envelope.

        As ``DataCardRegistry.get()`` for `uid`, `space`, `name`, and
        `version`. Prompt cards have no separate artifact hydration step.
        """
        ...
    def list(
        self,
        *,
        space: str | None = ...,
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
        """
        ...

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest Active PromptCard version.

        As ``DataCardRegistry.resolve_latest()``.
        """
        ...

    def delete(
        self,
        *,
        uid: str | None = ...,
        space: str | None = ...,
        name: str | None = ...,
        version: str | None = ...,
    ) -> None:
        """Delete a registered PromptCard and its stored Card envelope.

        As ``DataCardRegistry.delete()``.
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
    space: str
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

class RegistrationOutcome:
    """Server-derived result for one registered Card.

    The outcome includes the exact reference and hashes that the server
    accepted after validation and artifact completion.

    Attributes:
        status: the Card's lifecycle status, as in `CardSummary.status`.
        outcome: ``"registered"`` for a new version, ``"deduplicated"`` when
            an Active Card with identical content already existed and was
            reused, or ``"idempotent_noop"`` when the same exact version was
            already registered with identical content.
        card_blob_uri: where the Card envelope was stored, when one was
            written.
    """

    card_ref: CardRef
    spec_hash: str
    artifact_hash: str | None
    status: str
    outcome: str
    card_blob_uri: str | None

class RegistrationReceipt:
    """Result returned after registration and artifact completion succeed."""

    @property
    def root(self) -> CardRef:
        """The server-resolved reference of the Card you registered."""
        ...
    @property
    def outcomes(self) -> list[RegistrationOutcome]:
        """One outcome per Card in the registration, root included, dependencies first."""
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
    "RegistrationOutcome",
    "RegistrationReceipt",
    "WorkflowCards",
]
