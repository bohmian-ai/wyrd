import { existsSync, writeFileSync } from "node:fs";

import { verify } from "wyrd-ai";
import { z } from "zod";

const { Agent, Artifact, Check, Prompt, Requirement, Service, Verifier } = verify;

// One proposed report release: the `release-check` input.
export const Release = z.object({
  report: z.object({ text: z.string() }),
  recipient: z.object({ email: z.string(), authorized: z.boolean() }),
  destination: z.object({ url: z.string(), restricted: z.boolean() }),
});
export type Release = z.infer<typeof Release>;

// The company sharing policy. The program saves it beside itself the first
// time; edit the file and run again to publish a new policy version.
const POLICY = `# Company sharing policy

Reports may be shared outside the company only for external-audit purposes.
A shared report must never contain a bank account identifier such as BANK-ACCOUNT-DEMO-001.
`;
const policyFile = "company-sharing-policy.md";
if (!existsSync(policyFile)) {
  writeFileSync(policyFile, POLICY);
}
const policy = Artifact.fromPath("company-sharing-policy", policyFile);

// `policy-judge`: a model that reads the policy with `fs.read` and returns a
// verdict on the report. Its model calls go through the `openai` gateway
// deployment.
const policyJudge = new Agent("policy-judge", {
  prompt: Prompt.openaiChat("gpt-4o", {
    messages:
      "Read /inputs/policy.md with fs.read. Decide whether the company sharing " +
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
  .addWorkspace("/inputs/policy.md", policy.file("company-sharing-policy.md"));

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

export const service = new Service("finance-release", { verified_by: [releaseCheck] });

