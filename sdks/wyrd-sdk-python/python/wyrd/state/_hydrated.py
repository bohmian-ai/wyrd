"""The Card union returned by generic ``WyrdState`` lookup."""

from typing import TypeAlias

from ..cards import AgentCard
from ..data import DataCard
from ..model import ModelCard
from ..prompt import PromptCard
from . import CardEnvelope

HydratedCard: TypeAlias = AgentCard | PromptCard | ModelCard | DataCard | CardEnvelope
"""A Card ``WyrdState.get`` returns, in its kind's hydrated holder.

Narrow it with ``isinstance``; kinds without a typed holder are a
``CardEnvelope``.
"""
