import json
import time

from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards
from wyrd.verify import Execution

FAILED = """
SELECT execution_id FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND verdict = 'failed'
ORDER BY ended_at DESC LIMIT 1
"""

# ANCHOR: find
# Results are stored shortly after a check: look for up to a minute.
verifier = Cards().get(CardRef("Verifier", "refund-workflow", "0.1.0"))
for _ in range(60):
    rows = Bifrost().sql(FAILED, [verifier.card_ref.uid]).to_arrow().to_pylist()
    if rows:
        break
    time.sleep(1)
else:
    raise SystemExit("no failed refund-workflow result yet")
execution_id = rows[0]["execution_id"]
# ANCHOR_END: find

# ANCHOR: details
# One read: the exact versions judged, each check as written, and what it found:
# here, the names of the spans in the trace, sorted.
details = Execution.connect(execution_id).details()
judged, subject = details["verifier"], details["subject"]
print(f"{judged['name']}@{judged['version']} judged {subject['name']}@{subject['version']}: {details['outcome']['verdict']}")
for requirement in details["outcome"]["requirements"]:
    if requirement["outcome"] != "passed":
        print(f"  {requirement['id']}: {requirement['outcome']} · {requirement['reason']}")
        print(f"    expected: {json.dumps(requirement['check']['expected'], separators=(',', ':'))}")
        print(f"    found:    {json.dumps(sorted(requirement['actual']), separators=(',', ':'))}")
# ANCHOR_END: details
