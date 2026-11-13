// Verify each proposed report release before the application sends it.
//
// The `finance-release` Service shares synthetic customer reports with outside
// auditors. Before it releases one, it calls the `release-check` Verifier once,
// and the Verifier judges three requirements together:
//
// - `recipient-authorized`: the supplied recipient record says the recipient
//   is authorized (an assertion).
// - `follows-policy`: `policy-judge` reads the company sharing policy PDF and
//   judges the report against it (an Agent check, through the Wyrd gateway).
// - `destination-restricted`: the supplied destination record says access is
//   restricted (an assertion).
//
// Wyrd returns the verdict; the application decides. Only `passed` releases
// the report; anything else holds it for review. Nothing is sent anywhere.
import { readFileSync } from "node:fs";

import { type CardTypes, verify, WyrdError, WyrdState } from "wyrd-ai";

const { Agent, Artifact, Check, Prompt, Requirement, Service, Verifier } = verify;

// The company sharing policy, uploaded once from this directory with its text
// extracted so the judge can read it.
const policy = Artifact.fromPath("company-sharing-policy", "company-sharing-policy.pdf").extract(true);

// `policy-judge`: a model that reads the policy with `fs.read` and returns a
// verdict on the report. Its model calls go through the `openai` gateway
// deployment.
const policyJudge = new Agent("policy-judge", {
  prompt: Prompt.openaiChat("gpt-4o", {
    messages:
      "Read /inputs/policy.json with fs.read. Decide whether the company sharing " +
      "policy permits sharing this report with this recipient, citing the report " +
      "and the policy line you apply. Report: {{report}} Recipient: {{recipient}}",
    responseFormat: { type: "verdict" },
  }),
  execution_route: { kind: "wyrd_gateway", deployment: "openai" },
  tool_names: ["fs.read"],
  run_config: { max_iterations: 3 },
});

const followsPolicy = Check.agent(policyJudge)
  .inputs({ report: "inputs.report", recipient: "inputs.recipient" })
  .addWorkspace("/inputs/policy.json", policy.file("extracted/company-sharing-policy.pdf.json"));

const releaseCheck = new Verifier("release-check", {
  // The evidence the application sends, and the JSON Schema each must match.
  inputs: {
    report: { type: "object", required: ["text"], properties: { text: { type: "string" } } },
    recipient: {
      type: "object",
      required: ["email", "authorized"],
      properties: { email: { type: "string" }, authorized: { type: "boolean" } },
    },
    destination: {
      type: "object",
      required: ["url", "restricted"],
      properties: { url: { type: "string" }, restricted: { type: "boolean" } },
    },
  },
  requirements: [
    new Requirement(
      "recipient-authorized",
      "The recipient is authorized to receive the report.",
      Check.assert("inputs.recipient.authorized", "equals", true),
    ),
    new Requirement("follows-policy", "The company sharing policy permits this release.", followsPolicy),
    new Requirement(
      "destination-restricted",
      "The destination restricts access to the report.",
      Check.assert("inputs.destination.restricted", "equals", true),
    ),
  ],
});

const service = new Service("finance-release", { verified_by: [releaseCheck] });

// Upload the policy, declare every Card, and get a ready state.
const state = await WyrdState.declare(service);

try {
  const releases = JSON.parse(readFileSync("releases.json", "utf8")) as Record<string, CardTypes.JsonValue>[];
  for (const { name, ...evidence } of releases) {
    let result;
    try {
      // One synchronous call judges all three requirements.
      result = await state.run().observe.verify("release-check", evidence);
    } catch (error) {
      if (!(error instanceof WyrdError)) {
        throw error;
      }
      console.log(`${name}: ${error.code} -> held for review`);
      continue;
    }
    const { verdict, requirements } = result.outcome;
    // Only a passed verdict releases the report.
    const decision = verdict === "passed" ? "released" : "held for review";
    console.log(`${name}: ${verdict} (${result.verifier.name}@${result.verifier.version}) -> ${decision}`);
    for (const requirement of requirements) {
      if (requirement.outcome !== "passed") {
        console.log(`  ${requirement.id} ${requirement.outcome}: ${requirement.reason}`);
      }
    }
  }
} finally {
  await state.shutdown();
}
