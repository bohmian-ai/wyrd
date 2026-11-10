// The `card_authoring` story, authored in TypeScript through the `verify`
// namespace of `wyrd-ai`.
//
// Service `finance-release` composes Agents `report-reviewer` and
// `release-controls`, each verified by its own two-step Verifier
// (`report-policy`, `release-access`); the report judge reads the extracted
// `company-sharing-policy` Artifact. The graph is built in code and declared
// through `Cards.declare` or `WyrdState.declare`, which publish a local
// Artifact first and then declare the graph. The local upstream plays the
// policy judge: it reads `/inputs/policy.json`, then is inconclusive for a
// release naming `PROVIDER-UNCERTAIN`, fails one naming
// `BANK-ACCOUNT-DEMO-001`, and otherwise passes citing the report text and the
// policy line permitting `external-audit`.
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import {
  type CardTypes,
  Cards,
  type DeclarationReceipt,
  type VerificationResult,
  verify,
  WyrdClient,
  WyrdError,
  WyrdState,
} from "wyrd-ai";

import { judgeReply, toolCall, toolTurn } from "../support/assistant.js";
import type { ReceivedRequest, Reply } from "../support/http-receiver.js";
import { configureGateway } from "../support/local-development.js";
import { fixture, serverTest } from "../support/server.js";

const { Action, Agent, Artifact, CardRef, Check, CodeSource, Component, Prompt, Requirement, Service, Step, Verifier } =
  verify;
type Agent = verify.Agent;
type Artifact = verify.Artifact;
type ArtifactFile = verify.ArtifactFile;
type InputSlotArg = verify.InputSlotArg;
type Service = verify.Service;
type Step = verify.Step;
type Verifier = verify.Verifier;

vi.setConfig({ testTimeout: 300_000 });

const POLICY = fixture("cards/card_authoring/company-sharing-policy.pdf");
const CHECKS = fixture("cards/card_authoring/release_checks.py");
const STORY = fixture("cards/card_authoring/finance-release.yaml");
/** The extracted policy file every report judge mounts. */
const POLICY_FILE = "extracted/company-sharing-policy.pdf.json";

const RECIPIENT = {
  type: "object",
  additionalProperties: false,
  required: ["email", "authorized", "permitted_purposes"],
  properties: {
    email: { type: "string" },
    authorized: { type: "boolean" },
    permitted_purposes: { type: "array", items: { type: "string" } },
  },
};
const DESTINATION = {
  type: "object",
  additionalProperties: false,
  required: ["url", "encrypted", "restricted_access", "public_sharing"],
  properties: {
    url: { type: "string" },
    encrypted: { type: "boolean" },
    restricted_access: { type: "boolean" },
    public_sharing: { type: "boolean" },
  },
};
const REPORT = {
  type: "object",
  additionalProperties: false,
  required: ["text"],
  properties: { text: { type: "string" } },
};

type Release = Readonly<Record<string, CardTypes.JsonValue>>;

/** An external audit of a clean report to an authorized auditor over a controlled destination. */
function permitted(): Record<string, Record<string, CardTypes.JsonValue> | string> {
  return {
    report: { text: "Synthetic customer report: quarterly total $120." },
    recipient: { email: "auditor@example.test", authorized: true, permitted_purposes: ["external-audit"] },
    purpose: "external-audit",
    destination: {
      url: "https://auditor.example.test/reports",
      encrypted: true,
      restricted_access: true,
      public_sharing: false,
    },
  };
}

/** {@link permitted} with `slot` merged with `change`. */
function changed(slot: "recipient" | "destination", change: Release): Release {
  const release = permitted();
  return { ...release, [slot]: { ...(release[slot] as Release), ...change } };
}

/** The policy judge's first turn: read its mounted policy. */
const READ_POLICY = [toolCall("call-policy", "fs.read", { path: "/inputs/policy.json" })];

/** The policy judge's Verdict once `body` carries its `fs.read` result; no Verdict when the read has no `external-audit` line. */
function sharingVerdict(body: string): string {
  const messages = (JSON.parse(body) as { messages: { role: string; content: string | null }[] }).messages;
  const release = JSON.stringify(messages.filter(({ role }) => role === "user").map(({ content }) => content));
  let output: { sources?: string[]; units?: { text: string; locator: unknown }[] };
  try {
    output = JSON.parse(messages.find(({ role }) => role === "tool")?.content ?? "") as typeof output;
  } catch {
    return "the policy did not read";
  }
  const line = (output.units ?? []).find(({ text }) => text.includes("external-audit"));
  if (line === undefined) {
    return "the policy did not read";
  }
  const citations = [
    { slot: "report", locator: { json_pointer: "/text" } },
    { slot: output.sources?.[0], locator: line.locator },
  ];
  if (release.includes("PROVIDER-UNCERTAIN")) {
    return JSON.stringify({
      outcome: "inconclusive",
      reason: "the policy does not settle this release",
      citations: [],
    });
  }
  if (release.includes("BANK-ACCOUNT-DEMO-001")) {
    return JSON.stringify({ outcome: "failed", reason: "the report names a bank account identifier", citations });
  }
  return JSON.stringify({ outcome: "passed", reason: "an external audit may receive this report", citations });
}

/** The local model upstream: the policy judge, and nothing else. */
function upstream(request: ReceivedRequest): Reply {
  if (!request.body.includes("POLICY JUDGE")) {
    return { status: 404 };
  }
  return /"role"\s*:\s*"tool"/.test(request.body) ? judgeReply(sharingVerdict(request.body)) : toolTurn(READ_POLICY);
}

/** A policy-judge Agent `name` at `version`, as the story's Agent files declare it. */
function policyJudge(name: string, version: string): Agent {
  return new Agent(name, {
    version,
    execution_route: { kind: "wyrd_gateway", deployment: "openai" },
    tool_names: ["fs.read"],
    run_config: { max_iterations: 3 },
    prompt: Prompt.openaiChat("gpt-4o", {
      messages:
        "POLICY JUDGE: read /inputs/policy.json with fs.read. Determine whether the proposed release" +
        " satisfies the company policy. Cite original request and policy evidence. Report: {{report}}" +
        " Recipient: {{recipient}} Purpose: {{purpose}} Destination: {{destination}}",
      responseFormat: { type: "verdict" },
    }),
  });
}

/** A code step running `entrypoint` over `inputs`, judged by one equality requirement. */
function factStep(
  id: string,
  entrypoint: string,
  inputs: readonly string[],
  [requirement, statement, field, expected]: readonly [string, string, string, boolean],
): Step {
  const action = Action.code(CodeSource.file(CHECKS), entrypoint);
  for (const name of inputs) {
    action.bindInput(name, `inputs.${name}`);
  }
  const check = Check.assert(`steps.${id}.output.${field}`, "equals", expected);
  return new Step(id, action, { requirements: [new Requirement(requirement, statement, check)] });
}

/** The recipient, purpose, and destination slots. */
function releaseInputs(): Readonly<Record<string, InputSlotArg>> {
  return { recipient: RECIPIENT, purpose: { type: "string" }, destination: DESTINATION };
}

/** `report-policy` at `version`: the identifier scan with the judge reading `policy`, then context. */
function reportPolicy(version: string, judge: Agent, policy: ArtifactFile): Verifier {
  const verifier = new Verifier("report-policy", {
    version,
    description: "The report is fit to share under the company sharing policy.",
    inputs: releaseInputs(),
  });
  verifier.addInput("report", REPORT);
  const call = Check.agent(judge).inputs(
    Object.fromEntries(["report", "recipient", "purpose"].map((name) => [name, `inputs.${name}`])),
  );
  call.bindInput("destination", "inputs.destination").addWorkspace("/inputs/policy.json", policy);
  const scan = factStep(
    "policy",
    "report_facts",
    ["report"],
    [
      "no-bank-account-identifier",
      "The report contains no prohibited bank identifier.",
      "contains_bank_identifier",
      false,
    ],
  );
  scan.addRequirement(
    new Requirement("sharing-policy-satisfied", "The release satisfies the company sharing policy.", call),
  );
  const context = factStep(
    "policy-context",
    "policy_context",
    ["report", "purpose"],
    ["usable-report", "The proposed release contains report text.", "has_report", true],
  );
  return verifier.steps([scan, context]);
}

/** `release-access` at `version`: independent recipient and destination steps, unused `notes`. */
function releaseAccess(version: string, description?: string): Verifier {
  const verifier = new Verifier("release-access", {
    version,
    description: description ?? "The recipient and destination may receive the report.",
    inputs: releaseInputs(),
  });
  verifier.addInput("notes", { type: "string" });
  return verifier
    .addStep(
      factStep(
        "recipient",
        "recipient_facts",
        ["recipient", "purpose"],
        ["recipient-authorized", "The recipient is authorized for this purpose.", "authorized", true],
      ),
    )
    .addStep(
      factStep(
        "destination",
        "destination_facts",
        ["destination"],
        ["destination-controls-satisfied", "The destination has appropriate access controls.", "appropriate", true],
      ),
    );
}

/** `release-access` at `version` with a `summary` step consuming both fact steps' outputs. */
function summarizedAccess(version: string): Verifier {
  const summary = Action.code(CodeSource.file(CHECKS), "summarize_findings").inputs({
    recipient: "steps.recipient.output",
    destination: "steps.destination.output",
  });
  const check = Check.assert("steps.summary.output.releasable", "equals", true);
  return releaseAccess(version).addStep(
    new Step("summary", summary, {
      depends_on: ["recipient", "destination"],
      requirements: [
        new Requirement("release-permitted", "Both the recipient and the destination permit the release.", check),
      ],
    }),
  );
}

/** `finance-release` with every Card at `version`, its reviewer judged against `policy`. */
function financeRelease(version: string, policy: ArtifactFile, access: Verifier): Service {
  const reviewer = policyJudge("report-reviewer", version);
  return new Service("finance-release", { version, service_type: "api" })
    .addComponent(new Component("report-reviewer", reviewer).verifiedBy([reportPolicy(version, reviewer, policy)]))
    .addComponent(new Component("release-controls", policyJudge("release-controls", version)).verifiedBy([access]));
}

/** The local policy Artifact `version`, published from `source` with its documents extracted. */
function localPolicy(version: string, source: string = POLICY): Artifact {
  return Artifact.fromPath("company-sharing-policy", source, { version }).extract(true);
}

/** The registered policy Artifact `version`'s extracted file, used without uploading. */
function registeredPolicy(version: string): ArtifactFile {
  return CardRef.artifact("company-sharing-policy", version).file(POLICY_FILE);
}

/** The baseline story graph over `policy`. */
function baseline(policy: ArtifactFile): Service {
  return financeRelease("1.0.0", policy, releaseAccess("1.0.0"));
}

/** Verify `request` against both components, each under its own subject; a refusal is returned. */
async function verifyBoth(
  state: WyrdState,
  request: Readonly<Record<string, unknown>>,
): Promise<readonly [VerificationResult | WyrdError, VerificationResult | WyrdError]> {
  const attempt = async (
    alias: string,
    verifier: string,
    slots: readonly string[],
  ): Promise<VerificationResult | WyrdError> => {
    const inputs = Object.fromEntries(slots.filter((slot) => slot in request).map((slot) => [slot, request[slot]]));
    try {
      return await state.run(alias).observe.verify(verifier, inputs as Record<string, CardTypes.JsonValue>);
    } catch (error) {
      if (error instanceof WyrdError) {
        return error;
      }
      throw error;
    }
  };
  return [
    await attempt("report-reviewer", "report-policy", ["report", "recipient", "purpose", "destination"]),
    await attempt("release-controls", "release-access", ["recipient", "purpose", "destination", "notes"]),
  ];
}

/** `result` as a judged result; a refusal fails the test with its code. */
function judged(result: VerificationResult | WyrdError): VerificationResult {
  if (result instanceof WyrdError) {
    throw new Error(`refused: ${result.code}: ${result.detail}`);
  }
  return result;
}

/** Both judged results of {@link verifyBoth}. */
async function judgedBoth(
  state: WyrdState,
  request: Readonly<Record<string, unknown>>,
): Promise<readonly [VerificationResult, VerificationResult]> {
  const [report, access] = await verifyBoth(state, request);
  return [judged(report), judged(access)];
}

/** The outcome of requirement `id` in `result`. */
function outcome(result: VerificationResult, id: string): string | undefined {
  return result.outcome.requirements.find((each) => each.id === id)?.outcome;
}

/** Whether the simulated release may proceed: only both passed verdicts permit it. */
function releases(report: VerificationResult, access: VerificationResult): boolean {
  return report.outcome.verdict === "passed" && access.outcome.verdict === "passed";
}

/** Whether `report`'s passed policy verdict cites a line of the mounted policy. */
function citesPolicy(report: VerificationResult): boolean {
  const policy = report.outcome.requirements.find((each) => each.id === "sharing-policy-satisfied");
  return (
    policy?.outcome === "passed" &&
    (policy.citations ?? []).some((each) => each.slot.startsWith("attachment_") && "lines" in each.locator)
  );
}

/** Each declared Card of `receipt` by name, with its resolved identity and spec hash. */
function resolved(receipt: DeclarationReceipt): Record<string, unknown> {
  return Object.fromEntries(receipt.outcomes.map((each) => [each.card_ref.name, [each.card_ref, each.spec_hash]]));
}

/** The kinds `receipt` declared. */
function declaredKinds(receipt: DeclarationReceipt): string[] {
  return receipt.outcomes.map((each) => each.card_ref.kind);
}

/** The files registered policy `version` holds; none when it is unregistered. */
async function policyFiles(cards: Cards, version: string): Promise<Set<string>> {
  try {
    const inventory = await cards.listArtifacts({ kind: "Artifact", name: "company-sharing-policy", version });
    return new Set(inventory.artifacts.map((each) => String(each.relative_path)));
  } catch (error) {
    if (error instanceof WyrdError) {
      return new Set();
    }
    throw error;
  }
}

/** The policy version the registered `report-policy` `version` mounts. */
async function mountedPolicy(cards: Cards, version: string): Promise<string | undefined> {
  const { spec } = (await cards.get({ kind: "Verifier", name: "report-policy", version })) as unknown as {
    spec: {
      steps: {
        requirements: { check: { workspace: Record<string, { artifact: { card: { version: string } } }> } }[];
      }[];
    };
  };
  return spec.steps[0]?.requirements[1]?.check.workspace["/inputs/policy.json"]?.artifact.card.version;
}

/** The stable code `promise` is refused with. */
async function refusedCode(promise: Promise<unknown>): Promise<string> {
  try {
    await promise;
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return (error as WyrdError).code;
  }
  throw new Error("expected a refusal");
}

/** The number of policy-judge requests the local upstream has received. */
function judgeCalls(provider: { readonly received: readonly ReceivedRequest[] }): number {
  return provider.received.filter(({ path, body }) => path === "/v1/chat/completions" && body.includes("POLICY JUDGE"))
    .length;
}

const test = serverTest({ provider: upstream, verificationRuntime: true }).extend<{ cards: Cards }>({
  cards: [
    async ({ server: _ }, use) => {
      await configureGateway(WyrdClient.connect());
      await use(await Cards.connect());
    },
    { scope: "file" },
  ],
});

test("multi_component_service_verifies_each_subject", async ({ cards, provider }) => {
  const service = baseline(localPolicy("1.0.0").file(POLICY_FILE));
  const state = await WyrdState.declare(service);
  const asked = judgeCalls(provider);
  try {
    let [report, access] = await judgedBoth(state, permitted());
    for (const [result, alias, verifier, steps] of [
      [report, "report-reviewer", "report-policy", ["policy", "policy-context"]],
      [access, "release-controls", "release-access", ["recipient", "destination"]],
    ] as const) {
      expect(result.subject).toEqual(state.cardRef(alias));
      expect(result.verifier.name).toBe(verifier);
      expect(new Set(result.outcome.steps.map(({ id }) => id))).toEqual(new Set(steps));
      expect(
        result.outcome.steps.every(({ status }) => status === "succeeded"),
        JSON.stringify(result.outcome.steps),
      ).toBe(true);
    }
    expect(releases(report, access), JSON.stringify([report.outcome, access.outcome])).toBe(true);
    expect(judgeCalls(provider) - asked, "one policy read, then the verdict").toBe(2);
    const policy = report.outcome.requirements.find(({ id }) => id === "sharing-policy-satisfied");
    expect(policy?.outcome).toBe("passed");
    expect(policy?.citations).toContainEqual({ slot: "report", locator: { json_pointer: "/text" } });
    expect(citesPolicy(report), "the judge cites the policy line it read").toBe(true);

    const prohibited = {
      ...permitted(),
      report: { text: "Synthetic report. Bank identifier: BANK-ACCOUNT-DEMO-001." },
    };
    for (const [request, verdicts, failed] of [
      [prohibited, ["failed", "passed"], "no-bank-account-identifier"],
      [changed("recipient", { authorized: false }), ["passed", "failed"], "recipient-authorized"],
      [changed("destination", { public_sharing: true }), ["passed", "failed"], "destination-controls-satisfied"],
    ] as const) {
      [report, access] = await judgedBoth(state, request);
      expect([report.outcome.verdict, access.outcome.verdict], failed).toEqual(verdicts);
      expect(outcome(verdicts[0] === "failed" ? report : access, failed)).toBe("failed");
      expect(releases(report, access), "a failure holds the release").toBe(false);
    }

    const { report: _, ...missing } = permitted();
    report = judged((await verifyBoth(state, missing))[0]);
    expect(report.outcome.verdict).toBe("inconclusive");
    for (const id of ["no-bank-account-identifier", "usable-report"]) {
      expect(outcome(report, id)).toBe("inconclusive");
    }
    expect(report.outcome.requirements.every((each) => each.outcome !== "failed")).toBe(true);

    [report, access] = await judgedBoth(state, {
      ...permitted(),
      report: { text: "Synthetic report flagged PROVIDER-UNCERTAIN." },
    });
    expect(report.outcome.verdict).toBe("inconclusive");
    expect(outcome(report, "sharing-policy-satisfied")).toBe("inconclusive");
    for (const id of ["no-bank-account-identifier", "usable-report"]) {
      expect(outcome(report, id)).toBe("passed");
    }
    expect(releases(report, access), "uncertainty holds the release").toBe(false);

    const [refused] = await verifyBoth(state, { ...permitted(), report: { text: 123 } });
    expect(refused).toBeInstanceOf(WyrdError);
    expect((refused as WyrdError).code).toBe("WYRD_VERIFICATION_400_INPUT_INVALID");

    for (const [alias, verifier] of [
      ["report-reviewer", "release-access"],
      ["release-controls", "report-policy"],
    ] as const) {
      const { recipient, purpose, destination } = permitted();
      expect(
        await refusedCode(state.run(alias).observe.verify(verifier, { recipient, purpose, destination } as never)),
      ).toBe("WYRD_SDK_404_UNKNOWN_VERIFIER");
    }
  } finally {
    await state.shutdown();
  }

  const rerun = await cards.declare(service);
  expect(rerun.root.name).toBe("finance-release");
  expect(
    rerun.outcomes.every(({ outcome }) => outcome === "idempotent_noop"),
    JSON.stringify(rerun.outcomes),
  ).toBe(true);
  const again = await WyrdState.declare(service);
  try {
    for (const alias of ["report-reviewer", "release-controls"]) {
      expect(again.cardRef(alias)).toEqual(state.cardRef(alias));
    }
  } finally {
    await again.shutdown();
  }

  const summarized = await WyrdState.declare(
    financeRelease("1.1.0", registeredPolicy("1.0.0"), summarizedAccess("1.1.0")),
  );
  try {
    const registered = (await cards.get({ kind: "Verifier", name: "release-access", version: "1.1.0" })) as unknown as {
      spec: { steps: { depends_on?: string[] }[] };
    };
    expect(registered.spec.steps[2]?.depends_on).toEqual(["recipient", "destination"]);
    for (const [request, releasable, verdict] of [
      [permitted(), true, "passed"],
      [changed("destination", { public_sharing: true }), false, "failed"],
    ] as const) {
      const access = judged((await verifyBoth(summarized, request))[1]);
      const ran = access.outcome.steps.map(({ id }) => id);
      expect(new Set(ran)).toEqual(new Set(["recipient", "destination", "summary"]));
      expect(ran.at(-1), "the summary runs after both prerequisites").toBe("summary");
      const summary = access.outcome.steps.find(({ id }) => id === "summary");
      expect(summary?.status).toBe("succeeded");
      expect(summary?.output, "the summary consumes both prerequisite outputs").toEqual({
        releasable,
        recipient: { authorized: true },
        destination: { appropriate: releasable },
      });
      expect(outcome(access, "release-permitted")).toBe(verdict);
      expect(access.outcome.verdict).toBe(verdict);
    }
  } finally {
    await summarized.shutdown();
  }
});

test("local_artifacts_and_declaration_parity", async ({ cards, server }) => {
  const scratch = mkdtempSync(join(tmpdir(), "wyrd-ts-authoring-"));
  const authored = baseline(localPolicy("1.0.0").file(POLICY_FILE));
  const receipt = await cards.declare(authored);
  expect(receipt.root.name).toBe("finance-release");
  expect(declaredKinds(receipt)).not.toContain("Artifact");
  expect(await policyFiles(cards, "1.0.0")).toContain(POLICY_FILE);
  expect(await mountedPolicy(cards, "1.0.0")).toBe("1.0.0");

  const files = await cards.declare(STORY);
  expect(files.root).toEqual(receipt.root);
  expect(resolved(files), "the authored graph is the file graph").toEqual(resolved(receipt));
  expect(files.outcomes.every(({ outcome }) => outcome === "idempotent_noop")).toBe(true);
  expect(await cards.declareFromPath(STORY), "the path helper declares the same file graph").toEqual(files);
  expect(resolved(await cards.declare(baseline(registeredPolicy("1.0.0"))))).toEqual(resolved(files));

  const fromCode = await WyrdState.declare(baseline(registeredPolicy("1.0.0")));
  const fromFile = await WyrdState.declare(STORY);
  try {
    for (const alias of ["report-reviewer", "release-controls"]) {
      expect(fromCode.cardRef(alias)).toEqual(fromFile.cardRef(alias));
    }
    for (let round = 0; round < 2; round += 1) {
      const [report, access] = await judgedBoth(fromCode, permitted());
      expect(releases(report, access)).toBe(true);
      expect(report.verifier.version).toBe("1.0.0");
    }
  } finally {
    await fromFile.shutdown();
  }

  expect(
    await refusedCode(cards.declare(financeRelease("9.0.0", registeredPolicy("9.0.0"), releaseAccess("9.0.0")))),
  ).toBe("WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");
  expect(await policyFiles(cards, "9.0.0")).not.toContain(POLICY_FILE);

  const revised = join(scratch, "company-sharing-policy.pdf");
  writeFileSync(revised, Buffer.concat([readFileSync(POLICY), Buffer.from("% revision 2\n")]));
  const revisedState = await WyrdState.declare(
    financeRelease("2.0.0", localPolicy("2.0.0", revised).file(POLICY_FILE), releaseAccess("2.0.0")),
  );
  try {
    expect(await policyFiles(cards, "2.0.0")).toContain(POLICY_FILE);
    expect(await mountedPolicy(cards, "2.0.0")).toBe("2.0.0");
    expect(await mountedPolicy(cards, "1.0.0")).toBe("1.0.0");
    expect(judged((await verifyBoth(revisedState, permitted()))[0]).verifier.version).toBe("2.0.0");
    const old = judged((await verifyBoth(fromCode, permitted()))[0]);
    expect(old.verifier.version, "the old state keeps the old policy").toBe("1.0.0");
  } finally {
    await revisedState.shutdown();
    await fromCode.shutdown();
  }

  expect(await refusedCode(cards.declare(baseline(localPolicy("1.0.0", revised).file(POLICY_FILE))))).toBe(
    "WYRD_REGISTRY_409_SPEC_DRIFT",
  );
  const edited = financeRelease(
    "1.0.0",
    registeredPolicy("1.0.0"),
    releaseAccess("1.0.0", "Changed after publication."),
  );
  expect(await refusedCode(cards.declare(edited))).toBe("WYRD_REGISTRY_409_SPEC_DRIFT");
  expect(await refusedCode(WyrdState.declare(edited))).toBe("WYRD_REGISTRY_409_SPEC_DRIFT");

  expect(
    await refusedCode(
      cards.declare(
        financeRelease(
          "4.0.0",
          localPolicy("4.0.0", join(scratch, "missing.pdf")).file(POLICY_FILE),
          releaseAccess("4.0.0"),
        ),
      ),
    ),
  ).toBe("WYRD_LOADER_400_INVALID_ENVELOPE");
  await expect(cards.get({ kind: "Service", name: "finance-release", version: "4.0.0" })).rejects.toBeInstanceOf(
    WyrdError,
  );

  const judge = policyJudge("report-reviewer", "3.0.0");
  const orphaned = new Service("finance-release", { version: "3.0.0" })
    .addComponent(
      new Component("report-reviewer", judge).verifiedBy([
        reportPolicy("3.0.0", judge, localPolicy("3.0.0").file(POLICY_FILE)),
      ]),
    )
    .addComponent(
      new Component("release-controls", CardRef.agent("unregistered-controls", "1.0.0")).verifiedBy([
        releaseAccess("3.0.0"),
      ]),
    );
  expect(await refusedCode(cards.declare(orphaned))).toBe("WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY");
  expect(await policyFiles(cards, "3.0.0"), "the published policy outlives the refused graph").toContain(POLICY_FILE);
  const recovered = await cards.declare(financeRelease("3.0.0", registeredPolicy("3.0.0"), releaseAccess("3.0.0")));
  expect(declaredKinds(recovered)).not.toContain("Artifact");
  expect(await mountedPolicy(cards, "3.0.0")).toBe("3.0.0");

  const root = await cards.declare(localPolicy("5.0.0"));
  expect(root.root).toMatchObject({ name: "company-sharing-policy", version: "5.0.0" });
  expect(root.root.uid).toBeTruthy();
  expect(declaredKinds(root)).toEqual(["Artifact"]);

  const reader = WyrdClient.connect({ credential: server.scopedApiKey("card_reader", ["cards:read"]) });
  expect(await refusedCode((await Cards.connect({ client: reader })).declare(authored))).toBe(
    "WYRD_PERMISSION_403_DENIED_RBAC",
  );
  expect(await refusedCode(WyrdState.declare(authored, { client: reader }))).toBe("WYRD_PERMISSION_403_DENIED_RBAC");

  const directory = join(scratch, "policy-directory");
  mkdirSync(join(directory, "appendix"), { recursive: true });
  cpSync(POLICY, join(directory, "company-sharing-policy.pdf"));
  writeFileSync(join(directory, "appendix/retention.md"), "Retain release records for seven years.\n");
  const expected = ["company-sharing-policy.pdf", POLICY_FILE, "appendix/retention.md"];
  const fromDirectory = await cards.declare(
    financeRelease("6.0.0", localPolicy("6.0.0", directory).file(POLICY_FILE), releaseAccess("6.0.0")),
  );
  expect(declaredKinds(fromDirectory)).not.toContain("Artifact");
  expect([...(await policyFiles(cards, "6.0.0"))]).toEqual(expect.arrayContaining(expected));
  expect(await mountedPolicy(cards, "6.0.0")).toBe("6.0.0");
  const directoryState = await WyrdState.declare(
    financeRelease("7.0.0", localPolicy("7.0.0", directory).file(POLICY_FILE), releaseAccess("7.0.0")),
  );
  try {
    expect([...(await policyFiles(cards, "7.0.0"))]).toEqual(expect.arrayContaining(expected));
    expect(await mountedPolicy(cards, "7.0.0")).toBe("7.0.0");
    const [report, access] = await judgedBoth(directoryState, permitted());
    expect(report.verifier.version).toBe("7.0.0");
    expect(citesPolicy(report)).toBe(true);
    expect(releases(report, access)).toBe(true);
  } finally {
    await directoryState.shutdown();
  }
});
