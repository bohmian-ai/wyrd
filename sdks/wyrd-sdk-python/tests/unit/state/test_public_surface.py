"""Card envelope projection and typed accessor contracts for ``wyrd.state``."""

import json

import pytest
import wyrd
from wyrd.state import WyrdState


def test_card_envelope_exposes_complete_registered_card(state: WyrdState) -> None:
    """CardEnvelope projects identity, aliases, kind, and complete JSON fields."""
    envelope = state.card("model")
    assert envelope.kind.name == "Model"
    assert envelope.aliases == ("model", "primary_model")
    assert envelope.card_ref.name == "model"
    assert envelope.metadata["space"] == "default"
    assert envelope.spec["task_type"] == "Other"
    assert envelope.relationships == {}
    assert envelope.status is None
    dumped = envelope.model_dump()
    assert (dumped["apiVersion"], dumped["kind"], dumped["metadata"]["name"]) == (
        "wyrd/v1",
        "Model",
        "model",
    )
    assert json.loads(envelope.model_dump_json()) == dumped


def test_wrong_kind_accessor_raises_stable_wyrd_error(state: WyrdState) -> None:
    """Typed accessors reject a Card of another kind with stable mismatch details."""
    with pytest.raises(wyrd.WyrdError) as caught:
        state.model("root")
    assert caught.value.code == "WYRD_SDK_400_CARD_KIND_MISMATCH"
    assert caught.value.details["alias"] == "root"
    assert caught.value.details["expected_kind"] == "Model"
    assert caught.value.details["actual_kind"] == "Service"
    assert caught.value.details["card_ref"]["kind"] == "Service"
    assert caught.value.details["card_ref"]["name"] == "service"
