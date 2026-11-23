// A finance API that verifies each report release before it sends the report.
//
// It serves the Quickstart's `finance-release` Service, pulled once at deploy time with
// `wyrd pull --kind Service --name finance-release --version 0.1.0 --output-dir .wyrd-bundle`,
// and listens on PORT, by default 8000.
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { json } from "node:stream/consumers";

import { type VerificationResult, type VerifierInputs, WyrdError, WyrdState } from "wyrd-ai";

const state = WyrdState.fromPath(".wyrd-bundle");

// ANCHOR: handler
// Verify one release, then release the report only when the verdict passed.
async function sendReport(release: VerifierInputs): Promise<[number, object]> {
  try {
    const result = await state.run().observe.verify("release-check", release);
    return [200, result.passed ? releaseReport(result) : holdForReview(result)];
  } catch (error) {
    // No verdict, such as a refused credential or a timeout: nothing was judged, so nothing is sent.
    if (error instanceof WyrdError) return [503, { decision: "held for review", error: error.code }];
    throw error;
  }
}
// ANCHOR_END: handler

// ANCHOR: decisions
// A real API sends the report to its destination here, only after the verdict.
function releaseReport(result: VerificationResult) {
  return { decision: "released", run_id: result.run_id, execution_id: result.execution_id };
}

// Failed and inconclusive verdicts both wait for a person, who sees which requirements did not pass.
function holdForReview(result: VerificationResult) {
  const requirements = Object.fromEntries(
    result.outcome.requirements.filter((r) => r.outcome !== "passed").map((r) => [r.id, r.outcome]),
  );
  return {
    decision: "held for review",
    verdict: result.outcome.verdict,
    run_id: result.run_id,
    execution_id: result.execution_id,
    requirements,
  };
}
// ANCHOR_END: decisions

const server = createServer(async (request, response) => {
  if (request.method !== "POST" || request.url !== "/reports/send") {
    return response.writeHead(404).end();
  }
  const [status, body] = await sendReport((await json(request)) as VerifierInputs);
  response.writeHead(status, { "content-type": "application/json" }).end(JSON.stringify(body));
});
server.listen(Number(process.env.PORT ?? 8000), "127.0.0.1", () => {
  console.log(`listening on http://127.0.0.1:${(server.address() as AddressInfo).port}`);
});
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, async () => {
    server.close();
    // Drain what the state recorded before the process exits.
    await state.shutdown();
    process.exit(0);
  });
}
