import { highlight } from '$lib/mdsvex/highlight.js';

// The landing page's Declare → Observe → Verify example: one Service, one
// Verifier, carried through all three steps. Written for this page, so the
// site depends on no application source, and highlighted at build time.

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

const VERIFIER_CARD = `# A code check: sandboxed Python reads the ticket and names every field the
# help desk needs that is missing. No model.
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: ticket-complete
  version: 1.0.0
  space: support
spec:
  description: A support ticket carries the fields the help desk needs.
  inputs:
    ticket: {type: json}
  requirements:
    - id: ticket-complete
      statement: The ticket has an answer and an order id
      check:
        kind: code
        entrypoint: check
        inputs: {ticket: inputs.ticket}
        source:
          inline: |
            def check(ticket):
                missing = [field for field in ("answer", "order_id") if not ticket.get(field)]
                return {
                    "outcome": "failed" if missing else "passed",
                    "reason": "missing " + ", ".join(missing) if missing else "all fields present",
                    "citations": [{"slot": "ticket", "locator": {"json_pointer": ""}}],
                }`;

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

// The wire shapes are `VerificationResult` and `VerificationExecutionState`
// in crates/wyrd-spec/src/verification/execution.rs. The ticket is the one
// above without an order ID; the outcome, reason, and citation are what
// VERIFIER_CARD's check returns for it. IDs are illustrative.
const VERIFIER = `{"kind": "Verifier", "name": "ticket-complete", "version": "1.0.0", "space": "support", "uid": "019a3c2e-6f12-7c4d-8e31-5a9b0c7d2e64"}`;
const SUBJECT = `{"kind": "Service", "name": "support-desk", "version": "1.0.0", "space": "default", "uid": "019a3c2e-6f0a-7b22-9d4e-1c8f3a5b6d70"}`;
const CHECK_SOURCE = VERIFIER_CARD.match(/inline: \|\n((?: {12}.*\n?)+)/)?.[1].replace(/^ {12}/gm, '') ?? '';
if (!CHECK_SOURCE) throw new Error('landing: no inline check source in the Verifier');

/** The `VerificationResult` JSON for one execution of ticket-complete, at `indent`. */
function result(executionId: string, indent: string): string {
  const body = JSON.stringify(
    {
      execution_id: executionId,
      run_id: '019a3c2e-7a90-7f11-8b3d-2e5c9d1a6f47',
      verifier: '@VERIFIER@',
      subject: '@SUBJECT@',
      outcome: {
        verdict: 'failed',
        requirements: [
          {
            id: 'ticket-complete',
            statement: 'The ticket has an answer and an order id',
            outcome: 'failed',
            reason: 'missing order_id',
            citations: [{ slot: 'ticket', locator: { json_pointer: '' } }],
            check: { kind: 'code', source: { inline: CHECK_SOURCE }, entrypoint: 'check', inputs: { ticket: 'inputs.ticket' } }
          }
        ],
        steps: [],
        evidence: {
          ticket: { kind: 'json', value: { question: 'Where is my order?', answer: 'Your order A-1001 ships tomorrow.', order_id: null } }
        }
      }
    },
    null,
    2
  )
    .replace('"@VERIFIER@"', VERIFIER)
    .replace('"@SUBJECT@"', SUBJECT);
  return body.replace(/\n/g, `\n${indent}`);
}

// `verify` returns the result itself.
const RESULT = result('019a3c2e-7b41-7d2a-9c1e-4f6b8a2d0e13', '');

// After `submit`, `execution.status()` wraps the same result in the
// execution's lifecycle.
const STATUS = `{
  "execution_id": "019a3c2e-7c08-71b5-a2f4-9d3e6b1c8a50",
  "status": "completed",
  "result": ${result('019a3c2e-7c08-71b5-a2f4-9d3e6b1c8a50', '  ')},
  "dispatches": []
}`;

export function load() {
  const langs = ['python', 'rust', 'typescript'] as const;
  return {
    declare: {
      service: { title: 'support-desk.yaml', html: highlight(SERVICE, 'yaml') },
      verifier: { title: 'ticket-complete.yaml', html: highlight(VERIFIER_CARD, 'yaml') }
    },
    observe: langs.map((lang) => ({ lang, title: OBSERVE[lang].title, html: highlight(OBSERVE[lang].code, lang) })),
    verify: {
      realtime: { title: 'verify → result', html: highlight(RESULT, 'json') },
      background: { title: 'submit → execution.status()', html: highlight(STATUS, 'json') }
    }
  };
}
