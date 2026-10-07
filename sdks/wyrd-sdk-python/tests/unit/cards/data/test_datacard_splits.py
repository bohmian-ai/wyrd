"""Split builders validate their rule, and a card keeps the splits it is given."""

from pathlib import Path

import pandas as pd
import pytest
from wyrd.data import DataCard, Split, WyrdError

TEST_ROWS = {"kind": "Artifact", "name": "test", "version": "1.0.0", "space": "default"}


@pytest.mark.parametrize(
    ("op", "token"),
    [
        ("==", "Eq"),
        ("!=", "Ne"),
        (">", "Gt"),
        (">=", "Ge"),
        ("<", "Lt"),
        ("<=", "Le"),
        ("in", "In"),
    ],
)
def test_column_split_records_its_operator(op: str, token: str) -> None:
    assert Split.column("year", op, 2024).to_dict()["value"]["op"] == token


def test_split_column_invalid_operator_raises_invalid_split_rule() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.column("year", "between", [2020, 2024])

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_index_range_split_records_its_bounds() -> None:
    assert Split.index_range(0, 10).to_dict() == {
        "kind": "IndexRange",
        "value": {"start": 0, "stop": 10},
    }


@pytest.mark.parametrize(("start", "stop"), [(-1, 2), (0, -1)])
def test_split_index_range_rejects_negative_start_or_stop(start: int, stop: int) -> None:
    with pytest.raises(WyrdError) as exc:
        Split.index_range(start, stop)

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_index_range_start_after_stop_rejected() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.index_range(3, 2)

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_indices_split_records_its_rows() -> None:
    assert Split.indices([0, 2, 3]).to_dict() == {"kind": "Indices", "value": [0, 2, 3]}


@pytest.mark.parametrize(
    "values",
    [
        pytest.param([0, -1], id="negative"),
        pytest.param([1, 1], id="duplicate"),
        pytest.param([], id="empty"),
    ],
)
def test_invalid_indices_are_refused(values: list[int]) -> None:
    with pytest.raises(WyrdError) as exc:
        Split.indices(values)

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_materialized_split_records_its_artifact_ref() -> None:
    assert Split.materialized(TEST_ROWS).to_dict() == {"kind": "Materialized", "value": TEST_ROWS}


def test_split_materialized_allows_authored_ref_without_space() -> None:
    split = Split.materialized({"kind": "Artifact", "name": "train", "version": "1.0.0"})

    assert split.to_dict() == {
        "kind": "Materialized",
        "value": {"kind": "Artifact", "name": "train", "version": "1.0.0"},
    }


def test_rule_and_materialized_splits_survive_save_and_load(tmp_path: Path) -> None:
    DataCard(
        pd.DataFrame({"year": [2024]}),
        splits={"train": Split.column("year", "<=", 2024), "test": Split.materialized(TEST_ROWS)},
    ).save(tmp_path)

    splits = DataCard.from_path(tmp_path).splits

    assert {label: split.to_dict()["kind"] for label, split in splits.items()} == {
        "train": "Column",
        "test": "Materialized",
    }
