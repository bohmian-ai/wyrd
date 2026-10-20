"""The ``Card`` authoring protocol shared by the native Card holders."""

from typing import Protocol


class Card(Protocol):
    """Shared authoring capability implemented by native card holders.

    ``DataCard``, ``ModelCard``, and ``PromptCard`` are the declarable public
    implementations. ``AgentCard`` is an envelope holder, not a registry input.
    Pass one of those objects to ``Cards.declare``; do not construct ``Card``
    directly.
    """

    space: str
    name: str
    version: str
    uid: str

    def _to_card_envelope_json(self) -> str:
        """Return the holder's single Wyrd envelope conversion."""
        ...
