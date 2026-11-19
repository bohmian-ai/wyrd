import { setTimeout as sleep } from "node:timers/promises";

import { Bifrost, Cards, verify } from "wyrd-ai";
import { z } from "zod";

const FAILED = `
SELECT execution_id FROM bifrost.verification.results
WHERE verifier_card_uid = $1 AND verdict = 'failed'
ORDER BY ended_at DESC LIMIT 1
`;
const bifrost = await Bifrost.connect();

// ANCHOR: find
// Results are stored shortly after a check: look for up to a minute.
const verifier = await (await Cards.connect()).get("Verifier/refund-workflow@0.1.0");
let rows: { execution_id: string }[] = [];
for (let attempt = 0; attempt < 60 && rows.length === 0; attempt++) {
  if (attempt > 0) await sleep(1000);
  rows = await bifrost.sql(FAILED, [verifier.metadata.uid ?? ""], z.object({ execution_id: z.string() }));
}
const [failed] = rows;
if (failed === undefined) throw new Error("no failed refund-workflow result yet");
// ANCHOR_END: find

// ANCHOR: details
// One read: the exact versions judged, each check as written, and what it found:
// here, the names of the spans in the trace, sorted.
const details = await (await verify.Execution.connect(failed.execution_id)).details();
const { verifier: judged, subject } = details;
console.log(`${judged.name}@${judged.version} judged ${subject.name}@${subject.version}: ${details.outcome.verdict}`);
for (const requirement of details.outcome.requirements) {
  if (requirement.outcome !== "passed") {
    const { check } = requirement;
    console.log(`  ${requirement.id}: ${requirement.outcome} · ${requirement.reason}`);
    console.log(`    expected: ${JSON.stringify("expected" in check ? check.expected : null)}`);
    console.log(`    found:    ${JSON.stringify([...(requirement.actual as string[])].sort())}`);
  }
}
// ANCHOR_END: details
