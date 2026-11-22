from wyrd.bifrost import Bifrost
from wyrd.cards import CardRef, Cards

RECENT = """
SELECT to_char(window_start, '%H:%M') AS opened, to_char(window_end, '%H:%M') AS closed, verdict
FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND subject_card_uid = $2
ORDER BY window_start DESC LIMIT 3
"""

# The scheduler stores one result per judged window: print the newest three, times in UTC.
cards = Cards()
verifier = cards.get(CardRef("Verifier", "latency-budget", "1.0.0"))
service = cards.get(CardRef("Service", "checkout", "1.0.0"))
rows = Bifrost().sql(RECENT, [verifier.card_ref.uid, service.card_ref.uid]).to_arrow().to_pylist()
print("latency-budget@1.0.0 on checkout@1.0.0")
for row in rows:
    print(f"  {row['opened']}-{row['closed']} {row['verdict']}")
