"""Boundary tests for the Vala bifrost/observe Python surface."""

from __future__ import annotations

import json

import pytest

from wyrd.bifrost import Bifrost
from wyrd.observe import record

SCHEMA = json.dumps(
    {
        "type": "object",
        "properties": {"id": {"type": "integer"}},
        "required": ["id"],
    }
)
CARD_REF = "prod/Service/alpha@1.0.0"
ROW = json.dumps({"id": 1})


def test_extension_submodules_import():
    import wyrd._wyrd.bifrost  # noqa: F401
    import wyrd._wyrd.observe  # noqa: F401


def test_bifrost_insert_round_trips_at_boundary():
    bifrost = Bifrost("http://localhost", "secret")
    bifrost.insert(table="ns.tbl", schema=SCHEMA, row=ROW, card_ref=CARD_REF)
    bifrost.insert(table="ns.tbl", schema=SCHEMA, row=ROW, card_ref=CARD_REF)
    assert bifrost.producer_count == 1
    bifrost.insert(table="ns.other", schema=SCHEMA, row=ROW, card_ref=CARD_REF)
    assert bifrost.producer_count == 2
    assert bifrost.dropped == 0


def test_observe_record_is_fire_and_forget():
    bifrost = Bifrost("http://localhost", "secret")
    record(bifrost, table="ns.tbl", schema=SCHEMA, row=ROW, card_ref=CARD_REF)
    assert bifrost.dropped == 0


def test_insert_rejects_bad_card_ref():
    bifrost = Bifrost("http://localhost", "secret")
    with pytest.raises(ValueError):
        bifrost.insert(table="ns.tbl", schema=SCHEMA, row=ROW, card_ref="not-a-card-ref")


def test_producer_key_and_client_scope_are_not_importable():
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ProducerKey  # noqa: F401
    with pytest.raises(ImportError):
        from wyrd._wyrd.bifrost import ClientScope  # noqa: F401
