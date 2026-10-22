import { type CardRef, WyrdState } from "wyrd-ai";

// Declare the Service and the Verifier it references, and get a ready state.
const state = await WyrdState.declare("support-reply.yaml");
const run = state.run();

const id = (ref: CardRef) => `${ref.space}/${ref.kind}/${ref.name}@${ref.version}#${ref.uid}`;
for (const answer of ["Your refund request is approved.", "You have a guaranteed refund."]) {
  const result = await run.observe.verify("no-refund-promise", { answer });
  console.log(`${result.outcome.verdict}: ${result.outcome.requirements[0].reason}`);
  console.log(`  subject:  ${id(result.subject)}`);
  console.log(`  verifier: ${id(result.verifier)}`);
}
