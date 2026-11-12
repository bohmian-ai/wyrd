import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

import { context, trace } from "@opentelemetry/api";
import { BasicTracerProvider } from "@opentelemetry/sdk-trace-base";
import { expect, test, vi } from "vitest";

import { WyrdClient, WyrdState } from "wyrd-ai";

import { StorageContextManager } from "../support/otel-context.js";

const CREDENTIAL = "wyrd_test_actor";

test("on behalf of an unknown audience is refused locally", async () => {
  const client = WyrdClient.connect({ serverUrl: "http://127.0.0.1:9", credential: CREDENTIAL });

  // @ts-expect-error `storage` is outside the closed audience set; JavaScript callers can still pass it.
  await expect(client.onBehalfOf("subject-token", { audience: "storage" })).rejects.toMatchObject({
    code: "WYRD_SPEC_400_VALIDATION",
  });
});

test("grpc endpoint derives from the server url unless overridden", () => {
  const derived = WyrdClient.connect({ serverUrl: "https://wyrd.example.com/", credential: CREDENTIAL });
  const overridden = WyrdClient.connect({
    serverUrl: "https://wyrd.example.com",
    credential: CREDENTIAL,
    grpcUrl: "https://grpc.example.com:443",
  });

  expect([derived.serverUrl, derived.grpcUrl]).toEqual(["https://wyrd.example.com", "https://wyrd.example.com:50051"]);
  expect(overridden.grpcUrl).toBe("https://grpc.example.com:443");
});

// The Python SDK's offline complete Service bundle; loading it reads no server.
const BUNDLE = fileURLToPath(new URL("../../../../wyrd-sdk-python/tests/fixtures/bundles/complete", import.meta.url));

/** The Wyrd context headers one request reached `fetch` with. */
function sentContext(init: RequestInit | undefined): Record<string, string> {
  const headers = new Headers(init?.headers);
  const names = [
    "authorization",
    "x-api-key",
    "x-wyrd-access-token",
    "x-wyrd-workload-uid",
    "x-wyrd-deployment-environment",
    "wyrd-card-uid",
    "wyrd-run-id",
  ];
  return Object.fromEntries(names.flatMap((name) => (headers.has(name) ? [[name, headers.get(name) ?? ""]] : [])));
}

test("state get projects only the full Card of the alias's kind", () => {
  const { NativeWyrdState } = createRequire(import.meta.url)("../../index.cjs") as typeof import("../../index.cjs");
  const state = WyrdState.fromPath(BUNDLE);
  const card = vi.spyOn(NativeWyrdState.prototype, "card");
  const agent = vi.spyOn(NativeWyrdState.prototype, "agent");
  const prompt = vi.spyOn(NativeWyrdState.prototype, "prompt");
  const projected = () => [card, agent, prompt].map((spy) => spy.mock.calls.length);
  try {
    const triage = state.get("agent_triage");
    expect(projected()).toEqual([0, 1, 0]);
    expect(triage.kind).toBe("Agent");
    expect(triage.kind === "Agent" && triage.prompt().kind).toBe("Prompt");
    Object.assign(triage.metadata, { name: "changed" });
    expect(state.get("agent_triage").metadata.name).toBe("triage");

    expect(state.get("triage_prompt").kind).toBe("Prompt");
    expect(projected()).toEqual([0, 2, 1]);

    expect(state.get("model").kind).toBe("Model");
    expect(projected()).toEqual([1, 2, 1]);

    expect(() => state.get("missing")).toThrow(expect.objectContaining({ code: "WYRD_SDK_404_UNKNOWN_ALIAS" }));
    expect(projected()).toEqual([1, 2, 1]);
  } finally {
    vi.restoreAllMocks();
  }
});

test("state httpAuth carries its own service, run, and environment", async () => {
  const sent: RequestInit[] = [];
  vi.stubGlobal("fetch", (_input: unknown, init?: RequestInit) => {
    sent.push(init ?? {});
    return Promise.resolve(new Response("{}"));
  });
  context.setGlobalContextManager(new StorageContextManager());
  try {
    const configured = (environment: string): WyrdState => {
      const state = WyrdState.fromPath(BUNDLE, {
        client: WyrdClient.connect({ serverUrl: "http://127.0.0.1:9", credential: "access-token-1" }),
      });
      state.configure({ deploymentEnvironment: environment });
      return state;
    };
    const [prod, staging] = [configured("prod"), configured("staging")];
    const service = prod.rootRef.uid ?? "";
    const agent = prod.cardRef("agent_triage").uid ?? "";
    const [prodFetch, stagingFetch] = [prod.httpAuth, staging.httpAuth];
    const placeholders = { authorization: "Bearer wyrd", "x-api-key": "wyrd" };
    const base = { "x-wyrd-access-token": "Bearer access-token-1", "x-wyrd-workload-uid": service };

    await prodFetch("http://wyrd/v1/models", { headers: placeholders });
    expect(sentContext(sent.pop())).toEqual({
      ...base,
      "x-wyrd-deployment-environment": "prod",
      "wyrd-card-uid": service,
    });

    // Another state's Run nested inside never relabels this state's request,
    // and the request keeps the Run it started in while its token is fetched.
    const run = prod.run("agent_triage");
    await run.scope(() =>
      staging
        .run()
        .scope(() =>
          Promise.all([
            prodFetch("http://wyrd/v1/models", { headers: placeholders }),
            stagingFetch("http://wyrd/v1/models"),
          ]),
        ),
    );
    const contexts = sent.splice(0).map(sentContext);
    const own = contexts.find((sentHeaders) => sentHeaders["x-wyrd-deployment-environment"] === "prod");
    const other = contexts.find((sentHeaders) => sentHeaders["x-wyrd-deployment-environment"] === "staging");
    expect(own).toEqual({
      ...base,
      "x-wyrd-deployment-environment": "prod",
      "wyrd-card-uid": agent,
      "wyrd-run-id": run.runId,
    });
    expect(other?.["x-wyrd-deployment-environment"]).toBe("staging");
    expect(other?.["wyrd-run-id"]).not.toBe(run.runId);

    const explicit = "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01";
    const tracer = new BasicTracerProvider().getTracer("test");
    const active = tracer.startSpan("call");
    await context.with(trace.setSpan(context.active(), active), async () => {
      await prodFetch("http://wyrd/v1/models");
      await prodFetch("http://wyrd/v1/models", { headers: { traceparent: explicit } });
    });
    const [propagated, kept] = sent.splice(0).map((init) => new Headers(init.headers).get("traceparent"));
    expect(propagated).toContain(active.spanContext().traceId);
    expect(kept).toBe(explicit);

    await prodFetch("http://wyrd/v1/models", { headers: { "wyrd-card-uid": service } });
    for (const conflict of [
      { "wyrd-card-uid": agent },
      { "wyrd-run-id": run.runId },
      { "x-wyrd-workload-uid": agent },
    ]) {
      await expect(prodFetch("http://wyrd/v1/models", { headers: conflict })).rejects.toMatchObject({
        code: "WYRD_CLIENT_400_CONFIG_INVALID",
      });
    }
    expect(sent).toHaveLength(1);

    // A differing deployment environment is a correlation tag, never a conflict.
    await prodFetch("http://wyrd/v1/models", { headers: { "x-wyrd-deployment-environment": "staging" } });
    expect(sentContext(sent.pop())["x-wyrd-deployment-environment"]).toBe("prod");
  } finally {
    context.disable();
    vi.unstubAllGlobals();
  }
});

test("a browser-session client is validated locally and has no bearer", async () => {
  const session = WyrdClient.forBrowserSession({
    serverUrl: "http://127.0.0.1:9",
    cookie: "wyrd_session_acme=opaque.1",
    origin: "https://ui.example.com",
  });

  expect(session.serverUrl).toBe("http://127.0.0.1:9");
  await expect(session.accessToken()).rejects.toMatchObject({ code: "WYRD_CLIENT_400_CONFIG_INVALID" });
  for (const cookie of ["theme=dark", "wyrd_session_acme=a; wyrd_session_beta=b"]) {
    expect(() =>
      WyrdClient.forBrowserSession({ serverUrl: "http://127.0.0.1:9", cookie, origin: "https://ui.example.com" }),
    ).toThrow(expect.objectContaining({ code: "WYRD_CLIENT_400_CONFIG_INVALID" }));
  }
});
