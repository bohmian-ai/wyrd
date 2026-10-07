import pytest
from pydantic import BaseModel
from wyrd import WyrdError
from wyrd.bifrost import TableConfig


class Reserved(BaseModel):
    card_ref: str


def test_a_server_owned_column_is_refused_before_any_request() -> None:
    with pytest.raises(WyrdError) as raised:
        TableConfig(Reserved, "vala.datasets.reserved")
    assert raised.value.code == "WYRD_VALA_400_BIFROST_RESERVED_COLUMN"
