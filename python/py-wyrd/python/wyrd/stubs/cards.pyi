#### begin imports ####

from collections.abc import Mapping
from typing import Protocol, TypeAlias

from .data import DataCard, DataInterface
from .model import ModelCard, ModelInterface
from .prompt import Prompt, PromptCard, PromptReference

#### end of imports ####

JsonScalar: TypeAlias = str | int | float | bool | None
JsonValue: TypeAlias = JsonScalar | list[JsonValue] | dict[str, JsonValue]

class Card(Protocol):
    """Shared authoring capability implemented by native card holders.

    `DataCard`, `ModelCard`, `PromptCard`, and `AgentCard` are the public
    implementations.
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

class AgentCard:
    """Local Agent Card holder backed by the native Agent envelope.

    Use `PromptReference.inline(prompt)` for an inline prompt or
    `PromptReference.card(...)` for a registered Prompt Card reference. The
    holder contains no runtime, registry, storage, or artifact state.
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
            prompt: Inline prompt or registered Prompt Card reference.
            space: Card space. Defaults to the repository's Agent space.
            name: Card name. Defaults to `agent`.
            version: Semantic version. Defaults to `0.1.0`.
            uid: Optional server identity. A new local UID is generated when omitted.
            labels: Queryable labels.
            annotations: Free-form annotations.
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
        """Hydrate an Agent Card from a complete persisted envelope."""
        ...

    def _to_card_envelope_json(self) -> str:
        """Return the registry adapter's single envelope conversion."""
        ...

class CardKind:
    """The kind of a registered Wyrd Card.

    Kind-specific registry views use these values internally. Most Python
    callers use `cards.data`, `cards.model`, or `cards.prompt`, which already
    select the kind and return the matching card type.
    """

    Data: CardKind
    Model: CardKind
    Experiment: CardKind
    Prompt: CardKind
    Agent: CardKind
    Workflow: CardKind
    Eval: CardKind
    Drift: CardKind
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
    def name(self) -> str: ...

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
    ) -> None: ...
    def __repr__(self) -> str: ...

class VersionBump:
    """Version intent applied by registration.

    `Cards.register` defaults to `VersionBump.Minor`. Use `Patch` for a
    compatible correction, `Major` for a breaking change, or one of the
    factory methods when the version also needs pre-release or build metadata.

    Example:
        ```python
        cards.model.register(card, VersionBump.Patch)
        cards.model.register(card, VersionBump.pre("rc1"))
        cards.model.register(card, VersionBump.pre_build("rc1", "linux"))
        ```
    """

    Major: VersionBump
    Minor: VersionBump
    Patch: VersionBump

    @staticmethod
    def pre(identifier: str) -> VersionBump: ...
    @staticmethod
    def build(metadata: str) -> VersionBump: ...
    @staticmethod
    def pre_build(pre: str, build: str) -> VersionBump: ...

class DataSaveArgs:
    """Options forwarded to a `DataCard` interface during registration.

    These values are passed to the data interface's `save` implementation
    while Wyrd materializes the local artifact directory that is uploaded with
    the Card. They are not registry query options.

    `copy_bytes` is a Wyrd data-interface option. Additional JSON-compatible
    values depend on the selected interface.
    """

    def __init__(
        self,
        values: Mapping[str, JsonValue] | None = ...,
        *,
        copy_bytes: bool | None = ...,
    ) -> None: ...
    def to_dict(self) -> dict[str, JsonValue]: ...

class ModelSaveArgs:
    """Options forwarded to a `ModelCard` interface during registration.

    Values control local artifact serialization. They do not change Card
    identity or version selection.
    """

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None: ...
    def to_dict(self) -> dict[str, JsonValue]: ...

class DataLoadArgs:
    """Options forwarded to a `DataCard` interface during `DataCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None: ...
    def to_dict(self) -> dict[str, JsonValue]: ...

class ModelLoadArgs:
    """Options forwarded to a `ModelCard` interface during `ModelCard.load`."""

    def __init__(self, values: Mapping[str, JsonValue] | None = ...) -> None: ...
    def to_dict(self) -> dict[str, JsonValue]: ...

class Cards:
    """Tenant-scoped client for registered Wyrd Cards.

    `Cards` owns the connection and authentication context used by registry
    operations. The kind-specific properties are the normal API:

    ```python
    from wyrd import Cards

    cards = Cards()
    latest = cards.model.resolve_latest(space="ml", name="fraud-model")
    card = cards.model.get(uid=latest.uid)
    card.load()
    ```

    `get` retrieves and validates the serialized Card envelope. It does not
    download model or data bytes. Call `ModelCard.load` or `DataCard.load`
    afterward to hydrate those artifacts.

    Args:
        server_url: Optional Wyrd server URL. When omitted, the shared Wyrd
            client configuration supplies it.
        api_key: Optional API key override for this handle. When omitted, the
            shared Wyrd client configuration supplies credentials.

    Raises:
        WyrdError: If local configuration or the API-key override cannot be
            loaded.
    """

    def __init__(self, server_url: str | None = ..., api_key: str | None = ...) -> None: ...
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

    def register(
        self,
        card: Card,
        version_bump: VersionBump = ...,
        save_args: DataSaveArgs | ModelSaveArgs | None = ...,
    ) -> RegistrationReceipt:
        """Register a caller-owned Card and stamp its resolved identity.

        Registration serializes the complete Card envelope, saves DataCard or
        ModelCard artifacts through the attached interface, uploads the
        resulting manifest, and waits for server completion. On success, the
        same `card` object is updated with the server-assigned `uid`, resolved
        version, and normalized identity fields.

        Use the kind-specific view when you want static type checking for the
        card argument. The unscoped method is useful when the Card kind is
        selected dynamically.

        Args:
            card: A `DataCard`, `ModelCard`, or `PromptCard`.
            version_bump: Version intent. Defaults to `VersionBump.Minor`.
                `VersionBump.pre`, `build`, and `pre_build` add version
                metadata.
            save_args: Interface-specific save options. Use `DataSaveArgs`
                for `DataCard` and `ModelSaveArgs` for `ModelCard`. Prompt
                cards do not accept save arguments.

        Returns:
            A receipt containing the resolved root reference and one outcome
            for each registered Card.

        Raises:
            WyrdError: If `card` is not a supported native holder, required
                interface data is missing, serialization or artifact saving
                fails, the server rejects the registration, or completion
                fails.

        Example:
            ```python
            card = PromptCard(
                Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
                space="ml",
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
    data_card = cards.data.get(space="ml", name="training-data")
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

        The caller-owned card is updated with the server-assigned identity
        after registration completes. `save_args` is passed to the card's
        data interface; it is not sent as registry metadata.

        Args:
            card: DataCard to register.
            version_bump: Version intent. Defaults to `VersionBump.Minor`.
            save_args: Optional `DataSaveArgs` forwarded to the data interface.

        Returns:
            Receipt for the completed registration.

        Raises:
            WyrdError: If the card has no usable data interface, saving,
                serialization, upload, validation, or server completion fails.
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
    ) -> DataCard:
        """Retrieve and validate one complete `DataCard` envelope.

        Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
        required, and omitting `version` selects the server's latest resolved
        version. A custom data interface must be supplied here when the
        serialized Card uses one. `get` returns the hydrated card holder;
        artifact bytes are loaded later by `DataCard.load`.

        Args:
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector; supplied identity fields
                are checked against the returned Card.
            space: Card space, required when `uid` is omitted.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omit it to resolve the latest version.
            interface: Built-in or custom `DataInterface` instance/class used
                to rebuild the Python interface from Card metadata.

        Returns:
            A native `DataCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, the
                envelope fails validation, or a custom interface is missing.
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
            space: Optional space filter.
            name: Optional name filter.
            version_range: Optional semantic-version range.
            status: Optional lifecycle status filter.
            filter: Optional metadata query expression.
            include_prerelease: Include pre-release versions in the result.
            limit: Maximum number of summaries in this page.
            cursor: Opaque cursor returned by a previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: If a filter, version range, or server request is invalid.
        """
        ...

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest DataCard reference for a name.

        This returns identity only. Call `get(uid=reference.uid)` to retrieve
        the complete card envelope.

        Args:
            space: Card space.
            name: Card name.

        Returns:
            The latest exact `CardRef` selected by the server.

        Raises:
            WyrdError: If the identity is invalid or no matching Card exists.
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

        Provide `uid` for an exact deletion. Otherwise provide `space`,
        `name`, and optionally `version`.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, or
                deletion is rejected.
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

        Args:
            card: ModelCard to register.
            version_bump: Version intent. Defaults to `VersionBump.Minor`.
            save_args: Optional `ModelSaveArgs` forwarded to the model
                interface.

        Returns:
            Receipt for the completed registration. The caller-owned card is
            updated with the resolved server identity.

        Raises:
            WyrdError: If the model interface is missing or saving,
                serialization, upload, validation, or server completion fails.
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
    ) -> ModelCard:
        """Retrieve and validate one complete `ModelCard` envelope.

        Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
        required, and omitting `version` selects the latest resolved version.
        Supply a custom model interface here when the serialized Card cannot
        rebuild its interface from built-in metadata. Artifact bytes are not
        downloaded by `get`; call `ModelCard.load` afterward.

        Args:
            uid: Exact server-assigned Card UID. When present, it takes
                precedence over the named selector.
            space: Card space, required when `uid` is omitted.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omit it to resolve the latest version.
            interface: Built-in or custom `ModelInterface` instance/class used
                to rebuild the Python interface from Card metadata.

        Returns:
            A native `ModelCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, the
                envelope fails validation, or a required custom interface is
                missing.
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

        This operation does not fetch model bytes. Use `get` for the full
        envelope and `ModelCard.load` for artifact hydration.

        Args:
            space: Optional space filter.
            name: Optional name filter.
            version_range: Optional semantic-version range.
            status: Optional lifecycle status filter.
            filter: Optional metadata query expression.
            include_prerelease: Include pre-release versions in the result.
            limit: Maximum number of summaries in this page.
            cursor: Opaque cursor returned by a previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: If a filter, version range, or server request is invalid.
        """
        ...

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest ModelCard reference for a name.

        Call `get(uid=reference.uid)` to retrieve the complete card envelope.

        Args:
            space: Card space.
            name: Card name.

        Returns:
            The latest exact `CardRef` selected by the server.

        Raises:
            WyrdError: If the identity is invalid or no matching Card exists.
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

        Provide `uid` for an exact deletion. Otherwise provide `space`,
        `name`, and optionally `version`.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, or
                deletion is rejected.
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

        Args:
            card: PromptCard to register.
            version_bump: Version intent. Defaults to `VersionBump.Minor`.

        Returns:
            Receipt for the completed registration.

        Raises:
            WyrdError: If serialization, validation, upload, or server
                completion fails.
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

        Pass `uid` for an exact lookup. Without `uid`, `space` and `name` are
        required, and omitting `version` selects the latest resolved version.
        Prompt cards do not have a separate artifact hydration step.

        Args:
            uid: Exact server-assigned Card UID.
            space: Card space, required when `uid` is omitted.
            name: Card name, required when `uid` is omitted.
            version: Exact version. Omit it to resolve the latest version.

        Returns:
            A native `PromptCard` populated from the server-stored Card JSON.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, or
                the envelope fails validation.
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

        Args:
            space: Optional space filter.
            name: Optional name filter.
            version_range: Optional semantic-version range.
            status: Optional lifecycle status filter.
            filter: Optional metadata query expression.
            include_prerelease: Include pre-release versions in the result.
            limit: Maximum number of summaries in this page.
            cursor: Opaque cursor returned by a previous page.

        Returns:
            One cursor-paginated `CardList`.

        Raises:
            WyrdError: If a filter, version range, or server request is invalid.
        """
        ...

    def resolve_latest(self, *, space: str, name: str) -> CardRef:
        """Resolve the latest PromptCard reference for a name.

        Call `get(uid=reference.uid)` to retrieve the complete prompt Card.

        Args:
            space: Card space.
            name: Card name.

        Returns:
            The latest exact `CardRef` selected by the server.

        Raises:
            WyrdError: If the identity is invalid or no matching Card exists.
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

        Provide `uid` for an exact deletion. Otherwise provide `space`,
        `name`, and optionally `version`.

        Raises:
            WyrdError: If the selector is invalid, the Card is not found, or
                deletion is rejected.
        """
        ...

class CardSummary:
    """Metadata-only result returned by a kind-specific `list` operation.

    A summary contains enough information to select an exact Card, but it does
    not contain the serialized envelope or local data/model artifacts.
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

    Request another page by passing `next_cursor` as `cursor` to `list`.
    """

    items: list[CardSummary]
    refs: list[CardRef]
    next_cursor: str | None

class RegistrationOutcome:
    """Server-derived result for one registered Card.

    The outcome includes the exact reference and hashes that the server
    accepted after validation and artifact completion.
    """

    card_ref: CardRef
    spec_hash: str
    artifact_hash: str | None
    status: str
    outcome: str
    card_blob_uri: str | None

class RegistrationReceipt:
    """Result returned after registration and artifact completion succeed.

    `root` identifies the requested Card. `outcomes` contains the root and any
    dependency Cards registered as part of the operation.
    """

    @property
    def root(self) -> CardRef: ...
    @property
    def outcomes(self) -> list[RegistrationOutcome]: ...

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
]
