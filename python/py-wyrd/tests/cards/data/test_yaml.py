from __future__ import annotations

import json

import yaml
from wyrd.data import DataCard


def test_datacard_yaml_fixture_loads_through_public_surface() -> None:
    payload = yaml.safe_load(
        """
apiVersion: wyrd/v1
kind: Data
metadata:
  space: prod
  name: churn-train
  version: "1.0.0"
spec:
  interface:
    kind: Sql
    meta:
      dialect: duckdb
  schema:
    columns: []
  stats:
    byte_count: 1
    sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
"""
    )

    card = DataCard.model_validate_json(json.dumps(payload))

    assert card.space == "prod"
    assert card.name == "churn-train"
