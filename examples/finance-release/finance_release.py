from wyrd import WyrdState

from finance_service import Destination, Recipient, Release, Report, service

# Upload the policy, declare every Card, and get a ready state.
state = WyrdState.declare(service)
run = state.run()

recipient = Recipient(email="auditor@example.test", authorized=True)
destination = Destination(url="https://auditor.example.test/reports", restricted=True)
for text in (
    "Synthetic customer report: quarterly total $120.",
    "Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.",
):
    release = Release(report=Report(text=text), recipient=recipient, destination=destination)
    result = run.observe.verify("release-check", release)
    print(f"{result.verdict}: {text}")

state.shutdown()
