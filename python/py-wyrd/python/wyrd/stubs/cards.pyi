#### begin imports ####

#### end of imports ####

class CardKind:
    """Wyrd card kind.

    Use these constants when constructing CardRef values from Python.
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
    def name(self) -> str:
        """Return the Wyrd kind wire name."""
        ...

class CardRef:
    """Reference to a registered Wyrd Card.

    CardRef carries kind, name, version, space, and optional resolved UID.
    Python callers may pass either a CardKind constant or the native kind wire
    string to the constructor.
    """

    @property
    def kind(self) -> CardKind:
        """Card kind."""
        ...

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
        """Create a Wyrd card reference.

        Args:
            kind (CardKind | str): Wyrd card kind.
            name (str): Referenced card name.
            version (str): Exact referenced card version.
            space (str): Card space; required.
            uid (str | None): Optional resolved card UID.

        Raises:
            WyrdError: If kind is unknown, or identity fields are invalid.
        """
        ...

    def __repr__(self) -> str:
        """Return a concise Python representation."""
        ...

__all__ = ["CardKind", "CardRef"]
