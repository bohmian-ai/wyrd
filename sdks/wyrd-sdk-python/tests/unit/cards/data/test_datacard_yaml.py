"""A Data Card file loads with its identity, splits, and target columns."""

from pathlib import Path

from wyrd.data import DataCard


def test_sql_data_card_file_loads_its_identity(fixtures_dir: Path) -> None:
    card = DataCard.from_path(fixtures_dir / "authoring" / "data" / "churn-queries.yaml")

    assert (card.space, card.name, card.version) == ("prod", "churn-queries", "1.0.0")


def test_data_card_file_keeps_its_splits_and_target_columns(fixtures_dir: Path) -> None:
    card = DataCard.from_path(fixtures_dir / "authoring" / "data" / "churn-features.yaml")

    assert card.splits["train"].to_dict() == {
        "kind": "Column",
        "value": {"name": "year", "op": "Le", "value": 2024},
    }
    assert card.target_columns == ["churned"]
