"""Public Python lifecycle surfaces for local Card state."""

from pathlib import Path
from tempfile import TemporaryDirectory

import wyrd
from wyrd.cards import Cards
from wyrd.cli import run_wyrd_cli
from wyrd.state import CardEnvelope, HydratedArtifact, WyrdState


def test_public_wyrdstate_and_cards_exports() -> None:
    """Top-level exports point at the supported state and CLI surfaces."""
    assert wyrd.Cards is Cards
    assert wyrd.WyrdState is WyrdState
    assert wyrd.CardEnvelope is CardEnvelope
    assert wyrd.HydratedArtifact is HydratedArtifact
    assert wyrd.run_wyrd_cli is run_wyrd_cli


def test_metadata_only_bundle_is_rejected_with_stable_error() -> None:
    """Metadata-only bundles retain the stable unhydrated-artifact error."""
    manifest = """\
apiVersion: wyrd/hydrated-bundle/v1
hydration: metadata
root:
  kind: Data
  name: dataset
  version: 1.0.0
  space: default
cards: []
card_count: 0
artifact_count: 0
downloaded_artifact_count: 0
"""
    with TemporaryDirectory() as temporary_directory:
        path = Path(temporary_directory)
        (path / "metadata.yaml").write_text(manifest, encoding="utf-8")

        try:
            WyrdState.from_path(path)
        except wyrd.WyrdError as error:
            assert error.code == "WYRD_SDK_400_UNHYDRATED_ARTIFACT"
        else:
            raise AssertionError("metadata-only bundles must be rejected")
