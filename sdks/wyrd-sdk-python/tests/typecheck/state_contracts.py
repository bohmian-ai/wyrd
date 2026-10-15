"""Type fixture pinning hydrated Card lookup and Agent-to-Prompt navigation.

`ty` checks this module in the `py:typecheck` lane with unused suppressions as
errors, so every positive access below must type-check and every
``ty: ignore`` line must be a real rejection. It sits outside pytest
collection.
"""

from __future__ import annotations

from typing_extensions import assert_type
from wyrd.cards import AgentCard, CardRef
from wyrd.data import DataCard
from wyrd.model import ModelCard
from wyrd.prompt import PromptCard
from wyrd.state import CardEnvelope, HydratedCard, WyrdState


def typed_accessors_navigate_to_exact_types(state: WyrdState) -> None:
    agent = state.agent("agent")
    assert_type(agent, AgentCard)
    assert_type(agent.prompt(), PromptCard)
    assert_type(agent.prompt().ref, CardRef)
    assert_type(state.prompt("my_prompt").ref, CardRef)


def generic_lookup_narrows_with_isinstance(state: WyrdState) -> None:
    card = state["agent"]
    assert_type(card, HydratedCard)
    assert_type(state.get("agent"), HydratedCard)
    if isinstance(card, AgentCard):
        assert_type(card.prompt().ref, CardRef)
    elif isinstance(card, PromptCard):
        assert_type(card.ref, CardRef)
    elif isinstance(card, ModelCard):
        model: ModelCard = card
        del model
    elif isinstance(card, DataCard):
        data: DataCard = card
        del data
    else:
        assert_type(card.card_ref, CardRef)
        envelope: CardEnvelope = card
        del envelope


def unnarrowed_lookup_is_not_an_arbitrary_card(state: WyrdState) -> None:
    agent: AgentCard = state["agent"]  # ty: ignore[invalid-assignment]
    prompt_ref = state.get("agent").prompt_ref  # ty: ignore[unresolved-attribute]
    prompt: str = state.agent("agent").prompt()  # ty: ignore[invalid-assignment]
    missing = state.agent("agent").prompt().ref.missing  # ty: ignore[unresolved-attribute]
    del agent, prompt_ref, prompt, missing
