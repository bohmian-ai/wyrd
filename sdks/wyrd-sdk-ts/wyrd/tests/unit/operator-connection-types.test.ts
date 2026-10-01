import { describe, expect, it } from "vitest";

import type {
  CreateOperatorConnectionRequest,
  OperatorConnectionView,
  UpdateOperatorConnectionRequest,
} from "@wyrd/sdk";

describe("Operator connection types", () => {
  it("accept every provider-tagged create, update, and view shape", () => {
    const creates: readonly CreateOperatorConnectionRequest[] = [
      { provider: "slack", name: "s", workspace_id: "T1", bot_token: "x" },
      { provider: "pager_duty", name: "p", integration_key: "x" },
      {
        provider: "http",
        name: "h",
        origin: "https://hooks.example.com",
        auth: { scheme: "bearer", token: "x" },
      },
      {
        provider: "http",
        name: "h",
        origin: "https://hooks.example.com",
        auth: { scheme: "basic", username: "u", password: "p" },
      },
      {
        provider: "http",
        name: "h",
        origin: "https://hooks.example.com",
        auth: { scheme: "header", name: "X-Api-Key", value: "x" },
      },
    ];
    const updates: readonly UpdateOperatorConnectionRequest[] = [
      { provider: "slack" },
      { provider: "slack", workspace_id: "T2", bot_token: "x", status: "disabled" },
      { provider: "pager_duty", integration_key: "x", status: "active" },
      { provider: "http", origin: "https://hooks.example.com" },
      { provider: "http", auth: { scheme: "bearer", token: "x" } },
    ];
    const base = {
      connection_id: "id",
      name: "n",
      status: "active",
      created_at: "t",
      updated_at: "t",
    } as const;
    const views: readonly OperatorConnectionView[] = [
      { ...base, provider: "slack", workspace_id: "T1" },
      { ...base, provider: "pager_duty" },
      { ...base, provider: "http", origin: "o", auth: { scheme: "bearer" } },
      { ...base, provider: "http", origin: "o", auth: { scheme: "basic" } },
      {
        ...base,
        provider: "http",
        origin: "o",
        auth: { scheme: "header", name: "X-Api-Key" },
      },
    ];
    const view = views[0];
    if (view?.provider === "slack") {
      expect(view.workspace_id).toBe("T1");
    }
    expect([creates.length, updates.length, views.length]).toEqual([5, 5, 5]);
  });

  it("reject invalid provider combinations at compile time", () => {
    const rejected: readonly unknown[] = [
      // @ts-expect-error Slack creates require a workspace.
      { provider: "slack", name: "s", bot_token: "x" } satisfies CreateOperatorConnectionRequest,
      // @ts-expect-error PagerDuty does not take a Slack bot token.
      { provider: "pager_duty", name: "p", integration_key: "x", bot_token: "x" } satisfies CreateOperatorConnectionRequest,
      {
        provider: "http",
        name: "h",
        origin: "https://hooks.example.com",
        // @ts-expect-error Basic auth requires a password.
        auth: { scheme: "basic", username: "u" },
      } satisfies CreateOperatorConnectionRequest,
      // @ts-expect-error Unknown providers are outside the closed set.
      { provider: "email", name: "e" } satisfies CreateOperatorConnectionRequest,
      // @ts-expect-error Updates never rename a connection.
      { provider: "slack", name: "renamed" } satisfies UpdateOperatorConnectionRequest,
      // @ts-expect-error An HTTP origin is not a Slack update field.
      { provider: "slack", origin: "https://hooks.example.com" } satisfies UpdateOperatorConnectionRequest,
      // @ts-expect-error PagerDuty updates carry no workspace.
      { provider: "pager_duty", workspace_id: "T1" } satisfies UpdateOperatorConnectionRequest,
    ];
    const view = {} as OperatorConnectionView;
    // @ts-expect-error Views flatten provider config; there is no nested `config`.
    expect(view.config).toBeUndefined();
    // @ts-expect-error `workspace_id` exists only after narrowing to Slack.
    expect(view.workspace_id).toBeUndefined();
    expect(rejected).toHaveLength(7);
  });
});
