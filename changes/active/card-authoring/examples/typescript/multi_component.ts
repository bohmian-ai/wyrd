/**
 * Approved revision-5 interface sketch; future authoring exports are not implemented yet.
 * Options use the generated wire grammar; only methods use camelCase.
 */
import { fileURLToPath } from "node:url";
import { Cards, WyrdState } from "wyrd-ai";
import {
  Agent, Artifact, ArtifactFile, CardRef, CodeSource, Component, Service, Verifier,
} from "wyrd-ai/authoring";

const checks = fileURLToPath(new URL("../python/release_checks.py", import.meta.url));
const policySource = fileURLToPath(new URL(
  "../../../../../fixtures/cards/card_authoring/company-sharing-policy.pdf", import.meta.url,
));
const reportSchema = {
  type: "object", properties: { text: { type: "string" } },
  required: ["text"], additionalProperties: false,
} as const;
const recipientSchema = {
  type: "object", properties: {
    email: { type: "string" }, authorized: { type: "boolean" },
    permitted_purposes: { type: "array", items: { type: "string" } },
  },
  required: ["email", "authorized", "permitted_purposes"], additionalProperties: false,
} as const;
const destinationSchema = {
  type: "object", properties: {
    url: { type: "string" }, encrypted: { type: "boolean" },
    restricted_access: { type: "boolean" }, public_sharing: { type: "boolean" },
  },
  required: ["url", "encrypted", "restricted_access", "public_sharing"],
  additionalProperties: false,
} as const;

function agent(name: string): Agent {
  return new Agent(name, "1.0.0", {
    execution_route: { kind: "wyrd_gateway", deployment: "openai" },
    tool_names: ["fs.read"], run_config: { max_iterations: 3 },
    prompt: {
      provider: "openai", model: "gpt-4o", response_format: "verdict",
      messages: [{ role: "user", content:
        "Read /inputs/policy.json with fs.read. Determine whether the proposed release " +
        "satisfies the company policy. Cite original request and policy evidence. " +
        "Report: {{report}} Recipient: {{recipient}} Purpose: {{purpose}} Destination: {{destination}}",
      }],
    },
  });
}

/** Constructs the same two-component story as the Python and Rust sketches. */
export function makeService(policyFile: ArtifactFile): Service {
  const reviewer = agent("report-reviewer");
  const controls = agent("release-controls");
  const source = CodeSource.file(checks);
  const reportVerifier = new Verifier("report-policy", "1.0.0", {
    inputs: {
      report: { type: "json", schema: reportSchema },
      recipient: { type: "json", schema: recipientSchema },
      purpose: { type: "json", schema: { type: "string" } },
      destination: { type: "json", schema: destinationSchema },
    },
    steps: [
      {
        id: "policy", action: {
          kind: "code", source, entrypoint: "report_facts", inputs: { report: "inputs.report" },
        },
        requirements: [
          {
            id: "no-bank-account-identifier", statement: "The report contains no prohibited bank identifier.",
            check: { kind: "assert", path: "steps.policy.output.contains_bank_identifier",
              operator: "equals", expected: false },
          },
          {
            id: "sharing-policy-satisfied", statement: "The release satisfies the company sharing policy.",
            check: { kind: "agent", agent: reviewer,
              inputs: { report: "inputs.report", recipient: "inputs.recipient",
                purpose: "inputs.purpose", destination: "inputs.destination" },
              workspace: { "/inputs/policy.json": policyFile } },
          },
        ],
      },
      {
        id: "policy-context", action: {
          kind: "code", source, entrypoint: "policy_context",
          inputs: { report: "inputs.report", purpose: "inputs.purpose" },
        },
        requirements: [{
          id: "usable-report", statement: "The proposed release contains report text.",
          check: { kind: "assert", path: "steps.policy-context.output.has_report", operator: "equals", expected: true },
        }],
      },
    ],
  });
  const accessVerifier = new Verifier("release-access", "1.0.0");
  accessVerifier
    .addInput("recipient", { type: "json", schema: recipientSchema })
    .addInput("purpose", { type: "json", schema: { type: "string" } })
    .addInput("destination", { type: "json", schema: destinationSchema })
    .addInput("notes", { type: "json", schema: { type: "string" }, required: false });
  accessVerifier.addStep({
    id: "recipient", action: { kind: "code", source, entrypoint: "recipient_facts",
      inputs: { recipient: "inputs.recipient", purpose: "inputs.purpose" } },
    requirements: [{
      id: "recipient-authorized", statement: "The recipient is authorized for this purpose.",
      check: { kind: "assert", path: "steps.recipient.output.authorized", operator: "equals", expected: true },
    }],
  }).addStep({
    id: "destination", action: { kind: "code", source, entrypoint: "destination_facts",
      inputs: { destination: "inputs.destination" } },
    requirements: [{
      id: "destination-controls-satisfied", statement: "The destination has appropriate access controls.",
      check: { kind: "assert", path: "steps.destination.output.appropriate", operator: "equals", expected: true },
    }],
  });

  return new Service("finance-release", "1.0.0")
    .addComponent(new Component("report-reviewer", reviewer).verifiedBy([reportVerifier]))
    .addComponent(new Component("release-controls", controls).verifiedBy([accessVerifier]));
}

/** Publishes once, reuses the exact registered policy, and checks both subjects. */
export async function main(): Promise<void> {
  const policy = Artifact.fromPath("company-sharing-policy", "1.0.0", policySource).extract(true);
  const cards = await Cards.connect();
  console.log(await cards.declare(makeService(policy.file("extracted/company-sharing-policy.pdf.json"))));
  const registered = CardRef.artifact("company-sharing-policy", "1.0.0");
  const state = await WyrdState.declare(
    makeService(registered.file("extracted/company-sharing-policy.pdf.json")),
  );
  const request = {
    report: { text: "Synthetic customer report: quarterly total $120." },
    recipient: { email: "auditor@example.test", authorized: true, permitted_purposes: ["external-audit"] },
    purpose: "external-audit",
    destination: { url: "https://auditor.example.test/reports", encrypted: true,
      restricted_access: true, public_sharing: false },
  };
  const run = state.run();
  const report = await run.forCard("report-reviewer").observe.verify("report-policy", request);
  const access = await run.forCard("release-controls").observe.verify("release-access", {
    recipient: request.recipient, purpose: request.purpose, destination: request.destination,
  });
  console.log(report.verdict === "passed" && access.verdict === "passed" ? "release" : "hold");
}
