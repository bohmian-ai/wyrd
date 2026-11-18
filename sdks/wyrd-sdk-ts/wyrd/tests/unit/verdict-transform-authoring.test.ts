// Offline Jev decision checks through the public `verify` namespace: an
// invalid `verdict_transform` is refused where it is written or when its
// graph is validated, naming the field, and the corrected definition
// validates. Nothing here reaches a server; validation is the shared Rust
// validation.
import { expect, test } from "vitest";

import { verify, WyrdError } from "wyrd-ai";

const { Agent, Check, ChoiceQuestion, Component, NoulQuestion, Prompt, Requirement, ScoreQuestion, Service, Verifier } =
  verify;

const STATE = `\${context}`;
const CHECK = "requirements[0].check.verdict_transform";

const QUESTIONS = {
  refund: new ChoiceQuestion({
    instructions: "Does the reply follow the refund policy?",
    criteria: { compliant: "It follows the policy.", violation: "It breaks the policy." },
  }),
  helpful: new NoulQuestion({ instructions: "Does the reply help the customer?" }),
  compliance: new ScoreQuestion({ instructions: "How compliant is the reply?", criteria: ["Low", "Medium", "High"] }),
};

function judge(prompt: verify.Prompt = Prompt.jev("jev-latest", { state: STATE, questions: QUESTIONS })): verify.Agent {
  return new Agent("jev-judge", {
    version: "1.0.0",
    prompt,
    execution_route: { kind: "wyrd_gateway", deployment: "jev" },
    run_config: { max_iterations: 1 },
  });
}

function check(transform: unknown, agent: verify.Agent = judge()): verify.Check {
  return Check.agent(agent, { inputs: { context: "inputs.reply" }, verdict_transform: transform as never });
}

function desk(judged: verify.Check, agent: verify.Agent = judge()): verify.Service {
  const verifier = new Verifier("desk-policy", {
    version: "1.0.0",
    inputs: { reply: { type: "string" } },
    requirements: [new Requirement("judged", "The reply is judged.", judged)],
  });
  return new Service("desk", { version: "1.0.0" }).addComponent(
    new Component("jev-judge", agent).verifiedBy([verifier]),
  );
}

function thrown(action: () => unknown): WyrdError {
  try {
    action();
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return error as WyrdError;
  }
  throw new Error("nothing was refused");
}

function refusal(service: verify.Service): string {
  const error = thrown(() => service.validate());
  expect(error.code).toBe("WYRD_LOADER_400_INVALID_ENVELOPE");
  const { diagnostics } = error.details as { readonly diagnostics: readonly { readonly message: string }[] };
  return diagnostics.map(({ message }) => message).join(" ");
}

test.for([
  [
    { kind: "probability", question: "helpful", expected: true, pass_at: 0.2, fail_at: 0.8 },
    "verdict_transform.fail_at",
  ],
  [
    { kind: "probability", question: "helpful", expected: true, pass_at: 1.5, fail_at: 0.1 },
    "verdict_transform.fail_at",
  ],
  [{ kind: "score", question: "compliance", operator: "contains", expected: 2 }, "verdict_transform.operator"],
  [{ kind: "choice", question: " " }, "verdict_transform.question"],
  [{ kind: "verdict", question: "helpful" }, "verdict_transform"],
] as const)("verdict_transform refuses %j where it is written", ([transform, field]) => {
  const error = thrown(() => check(transform));
  expect(error.code).toBe("WYRD_SPEC_400_VALIDATION");
  expect(error.status).toBe(400);
  expect(error.details).toEqual({ rule: "verdict_transform", path: field });
  expect(error.detail).toContain(field);
  expect(error.remediation).toBeTruthy();
});

test("a corrected policy is accepted where the refused one was written", () => {
  expect(
    thrown(() => check({ kind: "probability", question: "helpful", expected: true, pass_at: 0.2, fail_at: 0.8 })).code,
  ).toBe("WYRD_SPEC_400_VALIDATION");
  desk(check({ kind: "probability", question: "helpful", expected: true, pass_at: 0.8, fail_at: 0.2 })).validate();
});

test("a null verdict_transform is the omitted policy: the check judges directly", () => {
  const chat = judge(Prompt.openaiChat("gpt-4o", { messages: "Judge {{context}}" }));
  const direct = (options: verify.AgentCheckOptions): verify.Check =>
    Check.agent(chat, { inputs: { context: "inputs.reply" }, ...options });
  desk(direct({}), chat).validate();
  desk(direct({ verdict_transform: null }), chat).validate();
});

test.for([
  [{ kind: "choice", question: "tone" }, `${CHECK}.question`],
  [{ kind: "probability", question: "refund", expected: true, pass_at: 0.9, fail_at: 0.1 }, `${CHECK}.kind`],
  [{ kind: "score", question: "helpful", operator: "equals", expected: 1 }, `${CHECK}.kind`],
  [{ kind: "choice", question: "refund" }, `${CHECK}.outcomes`],
  [
    { kind: "choice", question: "refund", outcomes: { compliant: "passed", violation: "failed", other: "failed" } },
    `${CHECK}.outcomes.other`,
  ],
] as const)("validation refuses %j its Jev prompt cannot answer", ([transform, field]) => {
  expect(refusal(desk(check(transform)))).toContain(field);
});

test("validation refuses a policy on a prompt without decisions", () => {
  const chat = judge(Prompt.openaiChat("gpt-4o", { messages: "Judge {{context}}" }));
  expect(refusal(desk(check({ kind: "choice", question: "refund" }, chat), chat))).toContain(
    "give the Agent a Jev system_one Prompt",
  );
});

test("a corrected policy validates after its refusal", () => {
  expect(refusal(desk(check({ kind: "choice", question: "refund" })))).toContain(`${CHECK}.outcomes`);
  desk(
    check({ kind: "choice", question: "refund", outcomes: { compliant: "passed", violation: "failed" } }),
  ).validate();
});
