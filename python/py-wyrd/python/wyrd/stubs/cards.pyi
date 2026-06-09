#### begin imports ####

#### end of imports ####

class Kind:
    """Native Wyrd card kind.

    Use these constants when constructing CardRef values from Python.
    """

    Data: Kind
    Model: Kind
    Experiment: Kind
    Prompt: Kind
    Tool: Kind
    Agent: Kind
    Workflow: Kind
    Eval: Kind
    Drift: Kind
    Service: Kind
    Policy: Kind
    Mcp: Kind
    Skill: Kind
    SubAgent: Kind
    Audit: Kind
    Artifact: Kind
    Trigger: Kind
    Operator: Kind

    @property
    def name(self) -> str:
        """Return the native Wyrd kind wire name."""
        ...

class CardRef:
    """Reference to a registered Wyrd Card.

    CardRef carries kind, name, version, optional space, and optional resolved
    UID. Python callers may pass either a Kind constant or the native kind wire
    string to the constructor.
    """

    kind: Kind
    name: str
    version: str
    space: str | None
    uid: str | None

    def __init__(
        self,
        kind: Kind | str,
        name: str,
        version: str,
        *,
        space: str | None = ...,
        uid: str | None = ...,
    ) -> None:
        """Create a Wyrd card reference.

        Args:
            kind (Kind | str): Native Wyrd card kind. External kinds are not
                constructable from Python.
            name (str): Referenced card name.
            version (str): Exact referenced card version.
            space (str | None): Optional card space.
            uid (str | None): Optional resolved card UID.

        Raises:
            WyrdError: If kind is not native, or identity fields are invalid.
        """
        ...

    def __repr__(self) -> str:
        """Return a concise Python representation."""
        ...

__all__ = ["CardRef", "Kind"]
