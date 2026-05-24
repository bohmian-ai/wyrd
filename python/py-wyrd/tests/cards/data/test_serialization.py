from __future__ import annotations

import json

from wyrd.data import DataCard, SqlInterface


def test_model_dump_json_round_trips_through_public_surface() -> None:
    card = DataCard(
        SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"),
        space="prod",
        name="churn-train",
        version="1.0.0",
        labels={"format": "tabular"},
    )

    payload = json.loads(card.model_dump_json())
    restored = DataCard.model_validate_json(card.model_dump_json())

    assert payload["apiVersion"] == "wyrd/v1"
    assert payload["kind"] == "Data"
    assert payload["metadata"]["labels"] == {"format": "tabular"}
    assert "tags" not in payload["metadata"]
    assert restored.space == "prod"
    assert restored.name == "churn-train"
    assert restored.version == "1.0.0"
    assert restored.labels == {"format": "tabular"}


def test_serialized_datacard_contains_spec_metadata_not_python_state() -> None:
    card = DataCard(SqlInterface(data={"queries": {"train": "select 1"}}, dialect="duckdb"))
    payload = json.loads(card.model_dump_json())

    assert "data" not in payload["spec"]["interface"]["meta"]
    assert payload["spec"]["interface"]["kind"] == "Sql"
