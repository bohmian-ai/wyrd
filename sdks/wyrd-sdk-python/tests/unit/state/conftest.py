"""Checked-in bundles and the interfaces that hydrate them."""

from pathlib import Path

import pytest
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
def trusted_model_hash() -> str:
    """The canonical artifact-manifest hash of ``builtin-model``'s ``model`` Card."""
    return "6df95bdf5f1a6ec1b59fe52d2974e222ec5e7094d877a3a16858aa89d66aaca5"


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
