from __future__ import annotations

import pytest
from wyrd.data import HuggingfaceInterface, Split, WyrdError


def test_huggingface_revision_must_be_hex_7_to_40_chars() -> None:
    with pytest.raises(WyrdError):
        HuggingfaceInterface(dataset_id="local", revision="bad-rev")


def test_split_index_range_validation_code() -> None:
    with pytest.raises(WyrdError) as exc:
        Split.index_range(10, 1)

    assert exc.value.code == "WYRD_DATA_400_INVALID_SPLIT_RULE"
