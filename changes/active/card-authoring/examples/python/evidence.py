"""Real Pydantic evidence models for the proposed synthetic release demo."""

from pydantic import BaseModel, ConfigDict


class Evidence(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class Report(Evidence):
    text: str


class Recipient(Evidence):
    email: str
    authorized: bool
    permitted_purposes: list[str]


class Destination(Evidence):
    url: str
    encrypted: bool
    restricted_access: bool
    public_sharing: bool


class ReleaseRequest(Evidence):
    report: Report
    recipient: Recipient
    purpose: str
    destination: Destination


def permitted_request() -> ReleaseRequest:
    return ReleaseRequest(
        report=Report(text="Synthetic customer report: quarterly total $120."),
        recipient=Recipient(
            email="auditor@example.test",
            authorized=True,
            permitted_purposes=["external-audit"],
        ),
        purpose="external-audit",
        destination=Destination(
            url="https://auditor.example.test/reports",
            encrypted=True,
            restricted_access=True,
            public_sharing=False,
        ),
    )


def prohibited_identifier_request() -> ReleaseRequest:
    request = permitted_request().model_dump()
    request["report"]["text"] += " Bank identifier: BANK-ACCOUNT-DEMO-001."
    return ReleaseRequest.model_validate(request)
