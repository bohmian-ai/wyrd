"""Release facts every card_authoring Verifier step computes."""


def recipient_facts(recipient: dict, purpose: str) -> dict:
    return {
        "authorized": (
            recipient["authorized"] and purpose in recipient["permitted_purposes"]
        ),
    }


def report_facts(report: dict) -> dict:
    return {
        "contains_bank_identifier": "BANK-ACCOUNT-DEMO-001" in report["text"],
    }


def destination_facts(destination: dict) -> dict:
    return {
        "appropriate": (
            destination["encrypted"]
            and destination["restricted_access"]
            and not destination["public_sharing"]
        ),
    }


def summarize_findings(recipient: dict, destination: dict) -> dict:
    return {
        "releasable": recipient["authorized"] and destination["appropriate"],
        "recipient": recipient,
        "destination": destination,
    }


def policy_context(report: dict, purpose: str) -> dict:
    return {"has_report": bool(report["text"].strip()), "purpose": purpose}
