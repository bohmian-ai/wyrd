// Print the newest three windows the scheduler judged, times in UTC.
import { Bifrost, Cards } from "wyrd-ai";
import { z } from "zod";

const RECENT = `
SELECT to_char(window_start, '%H:%M') AS opened, to_char(window_end, '%H:%M') AS closed, verdict
FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND subject_card_uid = $2
ORDER BY window_start DESC LIMIT 3
`;
const Row = z.object({ opened: z.string(), closed: z.string(), verdict: z.string() });

const cards = await Cards.connect();
const verifier = await cards.get("Verifier/latency-budget@1.0.0");
const service = await cards.get("Service/checkout@1.0.0");
const bifrost = await Bifrost.connect();
const rows = await bifrost.sql(RECENT, [verifier.metadata.uid ?? "", service.metadata.uid ?? ""], Row);
console.log("latency-budget@1.0.0 on checkout@1.0.0");
for (const row of rows) console.log(`  ${row.opened}-${row.closed} ${row.verdict}`);
