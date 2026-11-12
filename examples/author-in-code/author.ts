// Author a Service, its Agent, and its Verifier in code, declare them, verify
// two answers, and record each answer in the Service's table.
import { type CardRef, verify, WyrdState } from "wyrd-ai";

const { Agent, Check, Component, Prompt, Requirement, Service, Table, Verifier } = verify;

// The Verifier: one requirement over the evidence slot named `answer`.
const noRefundGuarantee = new Verifier("no-refund-guarantee", {
  inputs: { answer: { type: "string" } },
  requirements: [
    new Requirement(
      "no-refund-guarantee",
      "The answer never promises a guaranteed refund.",
      Check.assert("inputs.answer", "not_contains", "guaranteed refund"),
    ),
  ],
});

// The component: an Agent and the Prompt it runs.
const refundAgent = new Agent("refund-agent", {
  prompt: Prompt.openaiChat("gpt-4o", { messages: "Answer the refund question: {{question}}" }),
  execution_route: { kind: "wyrd_gateway", deployment: "openai" },
});

// The Service: the Agent under the alias `agent`, verified by the Verifier,
// and a table for the answers it gives.
const service = new Service("refund-replies", {
  tables: [
    // One column per property, in order.
    new Table("refund_replies", {
      type: "object",
      properties: { answer: { type: "string" }, verdict: { type: "string" } },
      required: ["answer", "verdict"],
    }),
  ],
  components: [new Component("agent", refundAgent, { verified_by: [noRefundGuarantee] })],
});

// No Card names a version: declaration selects each one and returns them.
const state = await WyrdState.declare(service);
const id = (ref: CardRef) => `${ref.kind}/${ref.name}@${ref.version}#${ref.uid}`;
console.log(`service: ${id(state.rootRef)}`);
console.log(`agent:   ${id(state.cardRef("agent"))}`);

await state.startBifrost(); // the writer; the one declared table is active
try {
  for (const answer of ["Your refund request is approved.", "You have a guaranteed refund."]) {
    // A Run whose view is the Agent, where the Verifier is bound.
    const run = state.run("agent");
    const result = await run.observe.verify("no-refund-guarantee", { answer });
    // Buffered, not yet stored: shutdown() drains it.
    await run.bifrost.write({ answer, verdict: result.outcome.verdict });
    console.log(`${result.outcome.verdict}: ${answer}`);
  }
} finally {
  await state.shutdown();
}
