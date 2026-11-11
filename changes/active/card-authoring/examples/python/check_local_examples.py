"""Check syntax and real local logic without importing the proposed SDK API."""

import ast
from pathlib import Path

from evidence import ReleaseRequest, permitted_request, prohibited_identifier_request
from pydantic import ValidationError
from release_checks import (
    destination_facts,
    policy_context,
    recipient_facts,
    report_facts,
    summarize_findings,
)


def main() -> None:
    paths = sorted(Path(__file__).parent.glob("*.py"))
    for path in paths:
        source = path.read_text()
        ast.parse(source, filename=str(path))
        compile(source, str(path), "exec")

    allowed = permitted_request()
    leaking = prohibited_identifier_request()
    assert policy_context(allowed.report.model_dump(), allowed.purpose) == {
        "has_report": True,
        "purpose": "external-audit",
    }
    assert policy_context({"text": "  "}, allowed.purpose)["has_report"] is False
    assert ReleaseRequest.model_validate_json(allowed.model_dump_json()) == allowed
    assert (
        report_facts(allowed.report.model_dump())["contains_bank_identifier"] is False
    )
    assert report_facts(leaking.report.model_dump())["contains_bank_identifier"] is True
    assert recipient_facts(allowed.recipient.model_dump(), allowed.purpose)[
        "authorized"
    ]
    assert not recipient_facts(allowed.recipient.model_dump(), "marketing")[
        "authorized"
    ]
    recipient = allowed.recipient.model_dump()
    recipient["authorized"] = False
    assert not recipient_facts(recipient, allowed.purpose)["authorized"]
    assert destination_facts(allowed.destination.model_dump())["appropriate"]
    for field in ("encrypted", "restricted_access", "public_sharing"):
        destination = allowed.destination.model_dump()
        destination[field] = not destination[field]
        assert not destination_facts(destination)["appropriate"], field
    assert summarize_findings({"a": 1}, {"b": 2}, {"c": 3}) == {
        "recipient": {"a": 1},
        "policy": {"b": 2},
        "destination": {"c": 3},
    }

    invalid = allowed.model_dump()
    invalid["destination"]["unexpected"] = True
    try:
        ReleaseRequest.model_validate(invalid)
    except ValidationError:
        pass
    else:
        raise AssertionError("unknown evidence fields must be rejected")

    print(f"PASS: {len(paths)} Python files compile; local evidence/check cases pass")
    print("Proposed SDK declaration, upload, concurrency and model calls were not run")


if __name__ == "__main__":
    main()
