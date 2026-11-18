import { trace } from "@opentelemetry/api";
import { verify, WyrdState } from "wyrd-ai";

const { Check, InputSlot, Requirement, Service, Verifier } = verify;

// Synthetic order records the agent's lookup tool reads.
type Order = { delivered: string; refundable: boolean };
const ORDERS: Record<string, Order> = { "1042": { delivered: "2 May", refundable: true } };

// ANCHOR: verifier
const refundWorkflow = new Verifier("refund-workflow", {
  inputs: { reply: InputSlot.json({ type: "string" }), trace: InputSlot.trace() },
  requirements: [
    // Judges the answer only: a reply that skipped the lookup passes it too.
    new Requirement(
      "reply-names-order",
      "The reply names the customer's order.",
      Check.assert("inputs.reply", "contains", "Order 1042"),
    ),
    // Judges what ran: the agent's trace must hold a look-up-order span.
    new Requirement(
      "looked-up-order",
      "The agent looked up the order before replying.",
      Check.trace("inputs.trace", "$.spans[*].name", "is_superset", ["look-up-order"]),
    ),
  ],
});
// ANCHOR_END: verifier

const state = await WyrdState.declare(new Service("refund-agent", { verified_by: [refundWorkflow] }));
await state.startTelemetry(); // export the agent's spans to Wyrd
await state.startBifrost(); // so flush can wait until they are stored
const tracer = trace.getTracer("refund-agent");

// ANCHOR: agent
function lookUpOrder(orderId: string): Order {
  return tracer.startActiveSpan("look-up-order", { attributes: { order_id: orderId } }, (span) => {
    span.end();
    return ORDERS[orderId] as Order;
  });
}

// Answer a refund request; return the reply and the trace ID of the work behind it.
function refundAgent(orderId: string, remembered?: Order): [string, string] {
  return tracer.startActiveSpan("answer-refund", (span): [string, string] => {
    // The bug: an order remembered from earlier in the conversation skips the lookup.
    const order = remembered ?? lookUpOrder(orderId);
    const reply = tracer.startActiveSpan("draft-reply", (draft) => {
      draft.end();
      return `Order ${orderId} was delivered on ${order.delivered}, so it qualifies for a refund.`;
    });
    span.end();
    return [reply, span.spanContext().traceId];
  });
}
// ANCHOR_END: agent

// ANCHOR: verify
const complete = refundAgent("1042");
const skipped = refundAgent("1042", ORDERS["1042"]);
await state.flush(); // store the spans before they are checked

const cases: [string, Record<string, string>][] = [
  ["complete", { reply: complete[0], trace: complete[1] }],
  ["skipped", { reply: skipped[0], trace: skipped[1] }],
  // The same reply sent without its trace: nothing shows what ran.
  ["no trace", { reply: complete[0] }],
];
for (const [label, evidence] of cases) {
  const result = await state.run().observe.verify("refund-workflow", evidence);
  console.log(`${label}: ${result.outcome.verdict} · ${evidence.reply}`);
  for (const requirement of result.outcome.requirements) {
    console.log(`  ${requirement.id}: ${requirement.outcome}`);
  }
}
await state.shutdown();
// ANCHOR_END: verify
