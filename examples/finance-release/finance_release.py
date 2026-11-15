from wyrd import WyrdState
from wyrd.verify import VerificationResult

from finance_service import Destination, Recipient, Release, Report, service

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)


def verify_release(release: Release) -> VerificationResult:
    with state.run() as run:
        return run.observe.verify("release-check", release)


if __name__ == "__main__":
    recipient = Recipient(email="auditor@example.test", authorized=True)
    destination = Destination(url="https://auditor.example.test/reports", restricted=True)
    for purpose, text in (
        ("external audit", "Synthetic customer report: quarterly total $120."),
        ("external audit", "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001."),
        ("marketing partnership", "Synthetic customer report: quarterly total $120."),
        ("regulator request", "Synthetic customer report: quarterly total $120."),
    ):
        release = Release(purpose=purpose, report=Report(text=text), recipient=recipient, destination=destination)
        result = verify_release(release)
        print(f"{purpose} · {text} → {result.verdict}")
        for requirement in result.requirements:
            if requirement["outcome"] != "passed":
                print(f"  {requirement['id']}: {requirement['reason']}")
    state.shutdown()
