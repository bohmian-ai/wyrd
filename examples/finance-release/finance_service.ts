import { verify } from "wyrd-ai";
import { z } from "zod";

const { Agent, Artifact, Check, Prompt, Requirement, Service, Verifier } = verify;

// One proposed report release: the `release-check` input.
export const Release = z.object({
  purpose: z.string(),
  report: z.object({ text: z.string() }),
  recipient: z.object({ email: z.string(), authorized: z.boolean() }),
  destination: z.object({ url: z.string(), restricted: z.boolean() }),
});
export type Release = z.infer<typeof Release>;

// The company sharing policy, uploaded from this directory with its text
// extracted so the judge can read it.
const policy = Artifact.fromPath("company-sharing-policy", "company-sharing-policy.pdf").extract(true);

// `policy-judge`: a model that reads the policy with `fs.read` and returns a
// verdict on the report. Its model calls go through the `openai` gateway
// deployment.
const policyJudge = new Agent("policy-judge", {
  prompt: Prompt.openaiChat("gpt-4o", {
    messages:
      "Read /inputs/company-sharing-policy.pdf.json with fs.read. Decide whether the company sharing " +
      "policy permits sharing this report with this recipient for this purpose, " +
      "citing the report and the policy line you apply. Purpose: {{purpose}} " +
      "Report: {{report}} Recipient: {{recipient}}",
    responseFormat: { type: "verdict" },
  }),
  execution_route: { kind: "wyrd_gateway", deployment: "openai" },
  tool_names: ["fs.read"],
  run_config: { max_iterations: 3 },
});

const followsPolicy = Check.agent(policyJudge)
  .inputs({ purpose: "inputs.purpose", report: "inputs.report", recipient: "inputs.recipient" })
  .addWorkspace("/inputs/company-sharing-policy.pdf.json", policy.file("extracted/company-sharing-policy.pdf.json"));

// ANCHOR: verifier
const releaseCheck = new Verifier("release-check", {
  inputs: Release,
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
// ANCHOR_END: verifier

export const service = new Service("finance-release", { verified_by: [releaseCheck] });

