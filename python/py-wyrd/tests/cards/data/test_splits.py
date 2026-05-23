from __future__ import annotations

import pytest
from wyrd.data import DataCard, Split, SqlInterface, WyrdError


def test_split_column_eq_ne_gt_ge_lt_le_in() -> None:
    ops = {"==": "Eq", "!=": "Ne", ">": "Gt", ">=": "Ge", "<": "Lt", "<=": "Le", "in": "In"}
    for op, expected in ops.items():
        assert Split.column("year", op, 2024).to_dict()["value"]["op"] == expected


def test_split_invalid_inputs_raise_invalid_split_rule() -> None:
    with pytest.raises(WyrdError):
        Split.column("year", "between", [2020, 2024])
    with pytest.raises(WyrdError):
        Split.index_range(-1, 2)
    with pytest.raises(WyrdError):
        Split.index_range(3, 2)
    with pytest.raises(WyrdError):
        Split.indices([1, 1])
    with pytest.raises(WyrdError):
        Split.materialized({"kind": "Data", "name": "not-artifact"})


def test_datacard_does_not_expose_split_data_execution() -> None:
    card = DataCard(SqlInterface(data={"queries": {"q": "select 1"}}, dialect="duckdb"))

    assert not hasattr(card, "split_data")
