import { verify, WyrdState } from "wyrd-ai";
import { z } from "zod";

const { Agent, Check, Prompt, Requirement, Service, Verifier } = verify;

const POLICY = "Refunds are possible within 30 days of purchase. Never promise a refund.";

// The evidence the application sends for each reply.
const Reply = z.object({ question: z.string(), answer: z.string() });

// The judge: a model that reads the reply and decides whether it follows the
// policy. Wyrd runs it through the `openai` gateway deployment.
const policyJudge = new Agent("policy-judge", {
  prompt: Prompt.openaiChat("gpt-4o-mini", {
    messages: `Policy: ${POLICY} Does this support answer follow the policy? Question: {{question}} Answer: {{answer}}`,
    responseFormat: { type: "verdict" },
  }),
  execution_route: { kind: "wyrd_gateway", deployment: "openai" },
});

// The Verifier: two requirements every reply must meet.
const replyCheck = new Verifier("reply-check", {
  inputs: Reply,
  requirements: [
    new Requirement(
      "no-refund-promise",
      "The answer never promises a guaranteed refund.",
      Check.assert("inputs.answer", "not_contains", "guaranteed refund"),
    ),
    new Requirement(
      "follows-policy",
      "The answer follows the refund policy.",
      Check.agent(policyJudge).inputs({ question: "inputs.question", answer: "inputs.answer" }),
    ),
  ],
});

// Declare the Cards and get a ready state.
const state = await WyrdState.declare(new Service("support", { verified_by: [replyCheck] }));

// Your support agent: any model call.
async function supportAgent(question: string): Promise<string> {
  const response = await fetch(`${process.env.OPENAI_BASE_URL ?? "https://api.openai.com/v1"}/chat/completions`, {
    method: "POST",
    headers: { authorization: `Bearer ${process.env.OPENAI_API_KEY}`, "content-type": "application/json" },
    body: JSON.stringify({
      model: "gpt-4o-mini",
      messages: [
        { role: "system", content: `You are a support agent. ${POLICY}` },
        { role: "user", content: question },
      ],
    }),
  });
  const completion = (await response.json()) as { choices: { message: { content: string | null } }[] };
  return completion.choices[0]?.message.content ?? "";
}

const question = "I bought this 45 days ago. Can I get my money back?";
// The agent's reply, and a reply that breaks the policy.
for (const answer of [await supportAgent(question), "Yes, you have a guaranteed refund."]) {
  const result = await state.run().observe.verify("reply-check", { question, answer });
  console.log(`${result.outcome.verdict}: ${answer}`);
  for (const requirement of result.outcome.requirements) {
    if (requirement.outcome !== "passed") {
      console.log(`  ${requirement.id}: ${requirement.reason}`);
    }
  }
}
await state.shutdown();
