import pytest
from wyrd import WyrdError
from wyrd.prompt import ResponseFormat

REQUIRED_ATTRIBUTES = (
    "code",
    "message",
    "detail",
    "details",
    "remediation",
    "status",
    "title",
    "type",
    "problem",
)


def raise_catalog_error() -> WyrdError:
    with pytest.raises(WyrdError) as error:
        ResponseFormat.json_schema("bad", [])
    return error.value


def test_wyrd_error_exposes_every_required_attribute() -> None:
    error = raise_catalog_error()

    for name in REQUIRED_ATTRIBUTES:
        assert hasattr(error, name), f"shared WyrdError is missing {name}"

    assert error.code == "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA"
    assert error.status == 400
    assert error.title
    assert error.remediation


def test_direct_attributes_agree_with_the_problem_payload() -> None:
    error = raise_catalog_error()
    problem = error.problem

    assert problem["code"] == error.code
    assert problem["status"] == error.status
    assert problem["title"] == error.title
    assert problem["remediation"] == error.remediation
    assert problem["type"] == error.type
    assert problem["details"] == error.details
    assert problem["detail"] == error.detail == error.message
    assert error.type.endswith(error.code)


def test_error_is_catchable_as_the_shared_base_class() -> None:
    with pytest.raises(WyrdError):
        ResponseFormat.json_schema("bad", [])
