// Wait for the schedule to fail the current window, then print each Operator dispatch's delivery.
import { Bifrost, Cards, verify } from "wyrd-ai";
import { z } from "zod";

const FAILED_SINCE = `
SELECT execution_id FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND subject_card_uid = $2 AND verdict = 'failed'
  AND to_unixtime(window_end) > $3
ORDER BY window_start DESC LIMIT 1
`;
const Row = z.object({ execution_id: z.string() });
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

const cards = await Cards.connect();
const verifier = await cards.get("Verifier/latency-budget@1.0.0");
const service = await cards.get("Service/checkout-alerts@1.0.0");
const params = [verifier.metadata.uid ?? "", service.metadata.uid ?? "", Date.now() / 1000];
const deadline = Date.now() + 180_000;
const bifrost = await Bifrost.connect();

// Wait for the schedule to fail the window the requests were recorded in.
let rows = await bifrost.sql(FAILED_SINCE, params, Row);
while (rows.length === 0) {
  if (Date.now() > deadline) throw new Error("no failed window was judged");
  await sleep(2_000);
  rows = await bifrost.sql(FAILED_SINCE, params, Row);
}
const execution = await verify.Execution.connect(rows[0].execution_id);
console.log("verdict: failed");

// Each Operator run is a dispatch on the execution, delivered on its own.
const settled = (status: string) => status === "delivered" || status === "failed";
let { dispatches } = await execution.status();
while (!dispatches.every((dispatch) => settled(dispatch.status))) {
  if (Date.now() > deadline) throw new Error("a dispatch is still pending");
  await sleep(1_000);
  ({ dispatches } = await execution.status());
}
for (const dispatch of dispatches) console.log(`dispatch: ${dispatch.status}`);
