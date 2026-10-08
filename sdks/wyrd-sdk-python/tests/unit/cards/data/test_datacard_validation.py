"""A saved Data Card that breaks one envelope rule is refused on load."""

from pathlib import Path

import pytest
from wyrd.data import DataCard, WyrdError


@pytest.mark.parametrize(
    ("fixture", "code"),
    [
        ("unknown-target-column", "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN"),
        ("empty-schema", "WYRD_DATA_400_VALIDATION"),
        ("duplicate-column", "WYRD_DATA_400_VALIDATION"),
        ("split-on-unknown-column", "WYRD_DATA_400_INVALID_SPLIT_RULE"),
        ("split-label-mismatch", "WYRD_DATA_400_VALIDATION"),
        ("malformed-digest", "WYRD_DATA_400_VALIDATION"),
        ("missing-default-query", "WYRD_DATA_400_VALIDATION"),
    ],
)
def test_invalid_data_card_is_refused(fixtures_dir: Path, fixture: str, code: str) -> None:
    with pytest.raises(WyrdError) as exc:
        DataCard.from_path(fixtures_dir / "invalid" / "data" / f"{fixture}.yaml")

    assert exc.value.code == code
