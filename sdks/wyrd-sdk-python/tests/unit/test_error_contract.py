import pytest
import wyrd
import wyrd.errors
from wyrd import WyrdError
from wyrd.prompt import ResponseFormat


def test_wyrd_error_exposes_the_catalog_problem() -> None:
    with pytest.raises(WyrdError) as raised:
        ResponseFormat.json_schema("bad", [])  # ty: ignore[invalid-argument-type]

    error = raised.value
    assert error.code == "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA"
    assert error.status == 400
    assert error.title
    assert error.remediation
    assert error.detail == error.message
    assert error.type.endswith(error.code)
    assert isinstance(error.details, dict)


def test_errors_module_projects_the_root_error_class() -> None:
    assert wyrd.errors.WyrdError is wyrd.WyrdError
