"""Verdict transforms for Agent checks, as Pydantic models.

An Agent check whose Prompt asks decision questions reads one answer through
its ``verdict_transform`` instead of a returned Verdict. Each model carries its
``kind`` discriminator; ``Check.agent(..., verdict_transform=...)`` accepts the
model or its canonical mapping, and the shared Rust contract validates it: a
model when it is constructed, a mapping where it is written.

These models need the ``pydantic`` extra; canonical mappings do not.
"""

from __future__ import annotations

from typing import Annotated, Any, Literal, Self

from pydantic import BaseModel, ConfigDict, Field, model_validator

from .._wyrd import validate_verdict_transform

Outcome = Literal["passed", "failed", "inconclusive"]
"""A canonical check outcome."""

ScalarOperator = Literal[
    "equals",
    "not_equals",
    "greater_than",
    "greater_than_or_equals",
    "less_than",
    "less_than_or_equals",
]
"""A parameterless scalar comparison operator by its wire name."""


class _Transform(BaseModel):
    """A verdict transform, refused by the shared validator when constructed."""

    model_config = ConfigDict(extra="forbid")

    @model_validator(mode="after")
    def _validated(self) -> Self:
        validate_verdict_transform(self)
        return self


class ChoiceTransform(_Transform):
    """Judge a Choice question by its selected label.

    Without ``outcomes`` the labels must be ``passed``, ``failed``, and
    ``inconclusive``; with it, every declared label maps to an outcome.
    Confidence never changes the outcome.
    """

    kind: Literal["choice"] = "choice"
    """The discriminator, always ``choice``."""
    question: str
    """The Choice question key."""
    outcomes: dict[str, Outcome] | None = None
    """Every declared option label to its outcome."""


class ProbabilityTransform(_Transform):
    """Threshold a yes/no question's probability.

    With ``q`` the probability of ``expected``, the check passes when
    ``q >= pass_at``, fails when ``q <= fail_at``, and is inconclusive between.
    """

    kind: Literal["probability"] = "probability"
    """The discriminator, always ``probability``."""
    question: str
    """The Noul question key."""
    expected: bool
    """The answer the check requires."""
    pass_at: float
    """The inclusive passing bound on the expected answer's probability."""
    fail_at: float
    """The inclusive failing bound, below ``pass_at``."""


class ScoreTransform(_Transform):
    """Compare a Score question's level index, from 0 for the lowest level, with ``expected``."""

    kind: Literal["score"] = "score"
    """The discriminator, always ``score``."""
    question: str
    """The Score question key."""
    operator: ScalarOperator | dict[str, Any]
    """A scalar comparison operator: a wire name, or a parameterized operator mapping."""
    expected: float
    """The level index compared against."""


VerdictTransform = Annotated[
    ChoiceTransform | ProbabilityTransform | ScoreTransform, Field(discriminator="kind")
]
"""Any verdict transform, discriminated by ``kind``."""
