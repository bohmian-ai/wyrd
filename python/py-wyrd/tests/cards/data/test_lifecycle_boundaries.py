from __future__ import annotations

from wyrd.data import DataCard, SqlInterface


def test_datacard_has_no_register_instance_method() -> None:
    card = DataCard(SqlInterface(data={"queries": {"q": "select 1"}}, dialect="duckdb"))

    assert not hasattr(card, "register")
