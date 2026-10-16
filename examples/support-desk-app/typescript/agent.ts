/**
 * The support agent: a plain OpenAI Agents SDK agent whose model calls go
 * through the Wyrd gateway. Its model and instructions come from the
 * `support-prompt` Card, not from code.
 */

import { Agent, OpenAIChatCompletionsModel, run, setTraceProcessors } from "@openai/agents";
import OpenAI from "openai";
import { type Run, WyrdAgentsTraceProcessor, type WyrdClient, type WyrdState } from "wyrd-ai";

// Send the Agents SDK's traces to Wyrd instead of OpenAI. Each agent and
// model call becomes an OpenTelemetry span under the request's Run.
setTraceProcessors([new WyrdAgentsTraceProcessor()]);

/**
 * An OpenAI client for the Wyrd gateway. `httpAuth` replaces the placeholder
 * key with a fresh Wyrd access token on every request, so the app never holds
 * a provider key.
 */
export function gateway(wyrd: WyrdClient): OpenAI {
  return new OpenAI({ baseURL: `${wyrd.serverUrl}/v1`, apiKey: "wyrd", fetch: wyrd.httpAuth });
}

/** Answer one customer question inside `wyrdRun`. */
export async function answer(client: OpenAI, state: WyrdState, wyrdRun: Run, question: string): Promise<string> {
  const card = state.agent("agent");
  const route = card.spec.execution_route;
  if (route.kind !== "wyrd_gateway") throw new Error("support-agent must route through the Wyrd gateway");
  const prompt = card.prompt().spec;

  // These headers tie the gateway's record of the call to the Run and Agent.
  const correlated = client.withOptions({
    defaultHeaders: { "wyrd-run-id": wyrdRun.runId, "wyrd-card-uid": state.cardRef("agent").uid ?? "" },
  });
  const agent = new Agent({
    name: card.metadata.name,
    instructions: prompt.system ?? "",
    model: new OpenAIChatCompletionsModel(correlated, `${route.deployment}/${prompt.model}`),
  });
  const result = await run(agent, question);
  return result.finalOutput ?? "";
}
