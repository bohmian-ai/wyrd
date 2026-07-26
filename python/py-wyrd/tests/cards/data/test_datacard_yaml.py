from __future__ import annotations

import json

import pandas as pd
import yaml
from wyrd.data import DataCard, PandasInterface, Split


def test_datacard_yaml_fixture_loads_through_public_surface(tmp_path) -> None:
    payload = yaml.safe_load(
        """
apiVersion: wyrd/v1
kind: Data
metadata:
  space: prod
  name: churn-train
  version: "1.0.0"
  uid: 01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00
spec:
  interface:
    kind: Sql
    meta:
      dialect: duckdb
  schema:
    columns: []
  sql:
    queries:
      main: select 1
  stats:
    byte_count: 1
    sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
"""
    )

    card = DataCard.model_validate_json(json.dumps(payload))

    assert card.space == "prod"
    assert card.name == "churn-train"


def test_datacard_yaml_round_trips_locked_envelope_with_splits(tmp_path) -> None:
    payload = json.loads(
        DataCard(PandasInterface(data=pd.DataFrame({"year": [2024]}))).model_dump_json()
    )
    payload["metadata"]["name"] = "split-card"
    payload["metadata"]["version"] = "1.0.0"
    payload["spec"]["splits"] = {
        "train": {"label": "train", "strategy": Split.column("year", "<=", 2024).to_dict()}
    }
    restored = DataCard.model_validate_json(json.dumps(yaml.safe_load(yaml.safe_dump(payload))))

    assert restored.name == "split-card"
    assert restored.metadata.to_dict()["splits"]["train"]["label"] == "train"
