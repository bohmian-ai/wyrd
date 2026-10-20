import { highlight } from '$lib/mdsvex/highlight.js';
import verifier from '../../../examples/support-desk-app/service/ticket-complete.yaml?raw';

// The landing page's Declare → Observe → Verify example: one Service, one
// Verifier, carried through all three steps. The Verifier is the example app's
// own file; the rest is written for this page and highlighted at build time.

const SERVICE = `apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-desk
  version: 1.0.0
  space: default
spec:
  service_type: agent
  verified_by:
    - verifier: ./ticket-complete.yaml`;

const OBSERVE = {
  python: {
    title: 'desk.py',
    code: `from pydantic import BaseModel


class Ticket(BaseModel):
    question: str
    answer: str
    order_id: str | None = None


run = state.run()
with run:
    ticket = Ticket(question=question, answer=answer, order_id=order_id)

    # Wait for the verdict.
    result = run.observe.verify("ticket-complete", ticket=ticket)

    # Or queue it and return at once.
    execution = run.observe.submit("ticket-complete", ticket=ticket)`
  },
  rust: {
    title: 'desk.rs',
    code: `let run = state.run();
let ticket = json!({ "question": question, "answer": answer, "order_id": order_id });

// Wait for the verdict.
let result = run.observe().verify("ticket-complete", ticket.clone()).await?;

// Or queue it and return at once.
let submission = run.observe().submit("ticket-complete", ticket).await?;`
  },
  typescript: {
    title: 'desk.ts',
    code: `const run = state.run();
const ticket = { question, answer, order_id: orderId };

// Wait for the verdict.
const result = await run.observe.verify("ticket-complete", { ticket });

// Or queue it and return at once.
const submitted = await run.observe.submit("ticket-complete", { ticket });`
  }
} as const;

// The ticket above without an order ID. The outcome and reason are what
// ticket-complete.yaml's check returns; steps and evidence are trimmed.
const RESULT = `{
  "execution_id": "019a3c2e-7b41-7d2a-9c1e-4f6b8a2d0e13",
  "run_id": "019a3c2e-7a90-7f11-8b3d-2e5c9d1a6f47",
  "verifier": {"kind": "Verifier", "space": "support", "name": "ticket-complete", "version": "1.0.0"},
  "subject": {"kind": "Service", "space": "default", "name": "support-desk", "version": "1.0.0"},
  "outcome": {
    "verdict": "failed",
    "requirements": [{
      "id": "ticket-complete",
      "statement": "The ticket has an answer and an order id",
      "outcome": "failed",
      "reason": "missing order_id",
      "citations": [{"slot": "ticket", "locator": {"json_pointer": ""}}]
    }]
  }
}`;

// The same check after \`submit\`, read back once terminal: the verdict sits
// under \`result\`, beside the execution's lifecycle.
const STATUS = `{
  "execution_id": "019a3c2e-7c08-71b5-a2f4-9d3e6b1c8a50",
  "status": "completed",
  "result": {
    "run_id": "019a3c2e-7a90-7f11-8b3d-2e5c9d1a6f47",
    "verifier": {"kind": "Verifier", "space": "support", "name": "ticket-complete", "version": "1.0.0"},
    "subject": {"kind": "Service", "space": "default", "name": "support-desk", "version": "1.0.0"},
    "outcome": {"verdict": "failed", "requirements": [{"id": "ticket-complete", "outcome": "failed", "reason": "missing order_id"}]}
  }
}`;

export function load() {
  const langs = ['python', 'rust', 'typescript'] as const;
  return {
    declare: {
      service: { title: 'support-desk.yaml', html: highlight(SERVICE, 'yaml') },
      verifier: { title: 'ticket-complete.yaml', html: highlight(verifier.trimEnd(), 'yaml') }
    },
    observe: langs.map((lang) => ({ lang, title: OBSERVE[lang].title, html: highlight(OBSERVE[lang].code, lang) })),
    verify: {
      realtime: { title: 'verify → result', html: highlight(RESULT, 'json') },
      background: { title: 'submit → execution.status()', html: highlight(STATUS, 'json') }
    }
  };
}
