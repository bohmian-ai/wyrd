// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type {
  CreateOperatorConnectionRequest,
  OperatorConnectionView,
  UpdateOperatorConnectionRequest,
} from "@wyrd/sdk";

const ORIGIN = "https://hooks.example.com";

// Every provider-tagged create, update, and view shape is accepted.
expectTypeOf([
  { provider: "slack", name: "s", workspace_id: "T1", bot_token: "x" },
  { provider: "pager_duty", name: "p", integration_key: "x" },
  { provider: "http", name: "h", origin: ORIGIN, auth: { scheme: "bearer", token: "x" } },
  { provider: "http", name: "h", origin: ORIGIN, auth: { scheme: "basic", username: "u", password: "p" } },
  { provider: "http", name: "h", origin: ORIGIN, auth: { scheme: "header", name: "X-Api-Key", value: "x" } },
] as const).toExtend<readonly CreateOperatorConnectionRequest[]>();
expectTypeOf([
  { provider: "slack" },
  { provider: "slack", workspace_id: "T2", bot_token: "x", status: "disabled" },
  { provider: "pager_duty", integration_key: "x", status: "active" },
  { provider: "http", origin: ORIGIN },
  { provider: "http", auth: { scheme: "bearer", token: "x" } },
] as const).toExtend<readonly UpdateOperatorConnectionRequest[]>();

// Invalid provider combinations are refused.
expectTypeOf({ provider: "slack", name: "s", bot_token: "x" } as const).not.toExtend<CreateOperatorConnectionRequest>();
expectTypeOf({
  provider: "http",
  name: "h",
  origin: ORIGIN,
  auth: { scheme: "basic", username: "u" },
} as const).not.toExtend<CreateOperatorConnectionRequest>();
expectTypeOf({ provider: "email", name: "e" } as const).not.toExtend<CreateOperatorConnectionRequest>();
const pagerDuty: CreateOperatorConnectionRequest = {
  provider: "pager_duty",
  name: "p",
  integration_key: "x",
  // @ts-expect-error PagerDuty does not take a Slack bot token.
  bot_token: "x",
};
// @ts-expect-error Updates never rename a connection.
const renamed: UpdateOperatorConnectionRequest = { provider: "slack", name: "renamed" };
// @ts-expect-error An HTTP origin is not a Slack update field.
const slackOrigin: UpdateOperatorConnectionRequest = { provider: "slack", origin: ORIGIN };
// @ts-expect-error PagerDuty updates carry no workspace.
const pagerWorkspace: UpdateOperatorConnectionRequest = { provider: "pager_duty", workspace_id: "T1" };
void [pagerDuty, renamed, slackOrigin, pagerWorkspace];

// Views flatten provider config, and provider fields exist only after narrowing.
expectTypeOf<OperatorConnectionView>().not.toHaveProperty("config");
expectTypeOf<OperatorConnectionView>().not.toHaveProperty("workspace_id");
expectTypeOf<Extract<OperatorConnectionView, { provider: "slack" }>["workspace_id"]>().toEqualTypeOf<string>();
