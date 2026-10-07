"""Checked-in bundles and the interfaces that hydrate them."""

from pathlib import Path

import pytest
from wyrd.model import ModelCard
from wyrd.state import WyrdState

from .support import TinyDataInterface, TinyModelInterface


@pytest.fixture
def complete_bundle(fixtures_dir: Path) -> Path:
    """A complete Service bundle: two custom Models, Data, a Prompt, two Agents, a Verifier.

    ``model`` and ``primary_model`` alias one Model Card, and the root Service
    binds the ``ok-check`` Eval Verifier.
    """
    return fixtures_dir / "bundles" / "complete"


@pytest.fixture
def builtin_model_bundle(fixtures_dir: Path) -> Path:
    """The complete bundle with ``model`` as a fitted sklearn joblib artifact."""
    return fixtures_dir / "bundles" / "builtin-model"


@pytest.fixture
def trusted_model_hash(builtin_model_bundle: Path) -> str:
    """The ``model`` Card's registered artifact hash, read from its envelope."""
    card = ModelCard.from_path(builtin_model_bundle / "cards" / "model" / "card.yaml")
    assert card.artifact_hash is not None
    return card.artifact_hash


@pytest.fixture
def interfaces() -> dict[str, object]:
    """Fresh custom interfaces for the complete bundle's Model and Data aliases."""
    return {
        "model": TinyModelInterface(),
        "backup": TinyModelInterface(),
        "training_data": TinyDataInterface(),
    }


@pytest.fixture
def state(complete_bundle: Path, interfaces: dict[str, object]) -> WyrdState:
    """The complete bundle hydrated offline with its custom interfaces."""
    return WyrdState.from_path(complete_bundle, interfaces=interfaces)
