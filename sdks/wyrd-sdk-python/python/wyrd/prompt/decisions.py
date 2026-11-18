"""Typed decision questions for ``Prompt.jev``, as Pydantic models.

Each model carries its ``type`` discriminator and refuses unknown fields;
``Prompt.jev`` lowers it to the provider's native question, so the stored
Prompt keeps only the native form. A ``NoulCriteria`` lowers to Jev's
``{"true": ..., "false": ...}`` criteria. Native question dictionaries remain
accepted beside these models, in the same map.

This module needs the ``pydantic`` extra; dictionary questions do not.
"""

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field

__all__ = [
    "ChoiceQuestion",
    "DecisionQuestion",
    "NoulCriteria",
    "NoulQuestion",
    "ScoreQuestion",
]


class NoulCriteria(BaseModel):
    """Descriptions of a yes/no question's two outcomes."""

    model_config = ConfigDict(extra="forbid")

    true_description: str
    """When the answer is true."""
    false_description: str
    """When the answer is false."""


class NoulQuestion(BaseModel):
    """A yes/no question, answered with the probability that it holds."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["noul"] = "noul"
    """The discriminator, always ``noul``."""
    instructions: str
    """What to decide about the state."""
    criteria: NoulCriteria | None = None
    """Optional descriptions of the true and false outcomes."""


class ChoiceQuestion(BaseModel):
    """A question choosing exactly one named option; the names are the answer labels."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["choice"] = "choice"
    """The discriminator, always ``choice``."""
    instructions: str
    """What to choose about the state."""
    criteria: dict[str, str]
    """Option name to its description."""


class ScoreQuestion(BaseModel):
    """A question grading on an ordered scale whose levels are indexed from zero, lowest first."""

    model_config = ConfigDict(extra="forbid")

    type: Literal["score"] = "score"
    """The discriminator, always ``score``."""
    instructions: str
    """What to grade about the state."""
    criteria: list[str]
    """Level descriptions, lowest first."""


DecisionQuestion = Annotated[
    NoulQuestion | ChoiceQuestion | ScoreQuestion, Field(discriminator="type")
]
"""Any typed decision question, discriminated by ``type``."""
