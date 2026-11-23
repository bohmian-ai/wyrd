import time

from wyrd import verify
from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards

FAILED_SINCE = """
SELECT execution_id FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND subject_card_uid = $2 AND verdict = 'failed'
  AND to_unixtime(window_end) > $3
ORDER BY window_start DESC LIMIT 1
"""

cards = Cards()
verifier = cards.get(CardRef("Verifier", "latency-budget", "1.0.0")).card_ref.uid
service = cards.get(CardRef("Service", "checkout-alerts", "1.0.0")).card_ref.uid
since, deadline = time.time(), time.monotonic() + 180

# Wait for the schedule to fail the window the requests were recorded in.
params = [verifier, service, since]
while not (rows := Bifrost().sql(FAILED_SINCE, params).to_arrow().to_pylist()):
    if time.monotonic() > deadline:
        raise TimeoutError("no failed window was judged")
    time.sleep(2)
execution = verify.Execution.connect(rows[0]["execution_id"])
print("verdict: failed")

# Each Operator run is a dispatch on the execution, delivered on its own.
while any(d["status"] not in ("delivered", "failed") for d in execution.status()["dispatches"]):
    if time.monotonic() > deadline:
        raise TimeoutError("a dispatch is still pending")
    time.sleep(1)
for dispatch in execution.status()["dispatches"]:
    print(f"dispatch: {dispatch['status']}")
