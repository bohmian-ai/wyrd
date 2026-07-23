from __future__ import annotations

import json

import pandas as pd
import pytest
from wyrd.data import DataCard, PandasInterface, Split, WyrdError


def test_split_column_eq_ne_gt_ge_lt_le_in() -> None:
    expected = {"==": "Eq", "!=": "Ne", ">": "Gt", ">=": "Ge", "<": "Lt", "<=": "Le", "in": "In"}

    for op, token in expected.items():
        assert Split.column("year", op, 2024).to_dict()["value"]["op"] == token


def test_split_column_invalid_operator_raises_invalid_split_rule() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.column("year", "between", [2020, 2024])

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_index_range_serializes_start_stop() -> None:
    assert Split.index_range(0, 10).to_dict()["value"] == {"start": 0, "stop": 10}


def test_split_index_range_rejects_negative_start_or_stop() -> None:
    for start, stop in [(-1, 2), (0, -1)]:
        with pytest.raises(WyrdError) as exc:
            Split.index_range(start, stop)
        assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_index_range_start_after_stop_rejected() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.index_range(3, 2)

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_indices_serializes_values() -> None:
    assert Split.indices([0, 2, 3]).to_dict()["value"] == [0, 2, 3]


def test_split_indices_rejects_negative_values() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.indices([0, -1])

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_indices_rejects_duplicate_values() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.indices([1, 1])

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"


def test_split_materialized_accepts_artifact_card_ref() -> None:
    payload = Split.materialized(
        {"kind": "Artifact", "name": "train", "version": "1.0.0", "space": "default"}
    ).to_dict()

    assert payload["kind"] == "Materialized"
    assert payload["value"]["kind"] == "Artifact"


def test_split_materialized_allows_authored_ref_without_space() -> None:
    split = Split.materialized({"kind": "Artifact", "name": "train", "version": "1.0.0"})

    assert split.to_dict() == {
        "kind": "Materialized",
        "value": {"kind": "Artifact", "name": "train", "version": "1.0.0"},
    }


def test_mixed_materialized_ref_and_rule_based_splits_round_trip(tmp_path) -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024]})))
    payload = json.loads(card.model_dump_json())
    payload["spec"]["splits"] = {
        "train": {"label": "train", "strategy": Split.column("year", "<=", 2024).to_dict()},
        "test": {
            "label": "test",
            "strategy": Split.materialized(
                {"kind": "Artifact", "name": "test", "version": "1.0.0", "space": "default"}
            ).to_dict(),
        },
    }
    restored = DataCard.model_validate_json(json.dumps(payload))

    assert restored.metadata.to_dict()["splits"]["train"]["strategy"]["kind"] == "Column"


def test_split_key_must_match_serialized_label() -> None:
    card = DataCard(PandasInterface(data=pd.DataFrame({"year": [2024]})))
    payload = json.loads(card.model_dump_json())
    payload["spec"]["splits"] = {
        "train": {"label": "test", "strategy": Split.column("year", "<=", 2024).to_dict()}
    }

    with pytest.raises(WyrdError):
        DataCard.model_validate_json(json.dumps(payload))


def test_datacard_does_not_expose_split_data_execution() -> None:
    assert not hasattr(DataCard(PandasInterface(data=pd.DataFrame({"x": [1]}))), "split_data")


def test_split_indices_rejects_empty_list() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.indices([])

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"
