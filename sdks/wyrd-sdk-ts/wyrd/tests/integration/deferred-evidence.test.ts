// CI publishes a deferred input after review starts; the waiting execution
// resumes. `verify.Evidence.publish` names the repository and commit
// explicitly or through `WYRD_REPOSITORY` / `WYRD_COMMIT`, and the receipt is
// durable and replayable.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import {
  Cards,
  GithubConnection,
  Principals,
  type VerificationExecutionState,
  WyrdClient,
  WyrdError,
  verify,
} from "wyrd-ai";
import { signInGithubBrowser } from "@wyrd/testing";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const HEAD = "a1".repeat(20);

/** A deployment that runs Verifiers, whose tenant connected installation `101` with `acme/web`. */
const test = serverTest({ githubApp: true, perTest: true, verificationRuntime: true }).extend<{ admin: WyrdClient }>({
  admin: async ({ server }, use) => {
    const github = server.githubApp;
    github.addInstallation(101, [
      { id: 1, fullName: "acme/api" },
      { id: 2, fullName: "acme/web" },
    ]);
    github.addUser("octo", [101]);
    github.signIn("octo");
    github.installTarget(101);
    const key = server.bootstrapService(["admin"], "evidence_admin");
    const admin = WyrdClient.connect({ credential: key });
    const receipt = await (await GithubConnection.connect({ client: admin })).begin();
    expect(signInGithubBrowser(server.baseUrl, server.tenantSlug, key).open(receipt.authorization_url).status).toBe(
      200,
    );
    await use(admin);
  },
});

/** Poll `execution` until it reaches `status`, for at most 30 seconds. */
async function until(execution: verify.Execution, status: string): Promise<VerificationExecutionState> {
  const deadline = Date.now() + 30_000;
  for (;;) {
    const state = await execution.status();
    if (state.status === status) {
      return state;
    }
    expect(Date.now(), JSON.stringify(state)).toBeLessThan(deadline);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

test("published evidence resumes the waiting execution", async ({ server, admin }) => {
  const cards = await Cards.connect({ client: admin });
  const receipt = await cards.declareFromPath(fixture("cards/deferred_evidence/repository.yaml"));
  const [binding] = (await cards.get(receipt.root)).status?.verification?.binding_ids ?? [];
  const github = server.githubApp;
  github.setPullRequest(2, 7, "b1".repeat(20), HEAD);
  expect(github.deliverPullRequest(server.baseUrl, 101, 2, 7, "opened")).toBe(202);
  const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${binding}`, {
    headers: { "x-wyrd-access-token": `Bearer ${await admin.accessToken()}` },
  });
  const { last_execution_id: id } = (await response.json()) as { last_execution_id: string };
  const execution = await verify.Execution.connect(id, { client: admin });
  expect((await until(execution, "waiting")).waiting).toEqual(["ci"]);

  const evidence = await verify.Evidence.connect({ client: admin });
  const accepted = await evidence.publish("ci", { status: "green" }, { repository: "acme/web", commit: HEAD });
  expect(accepted).toMatchObject({
    status: "accepted",
    matched_subscriptions: 1,
    repository: { repository_id: "2" },
    tags: { commit: HEAD },
  });
  expect((await execution.wait({ timeoutMs: 30_000 })).outcome.verdict).toBe("passed");

  const cwd = process.cwd();
  vi.stubEnv("WYRD_REPOSITORY", "acme/web");
  vi.stubEnv("WYRD_COMMIT", HEAD);
  process.chdir(mkdtempSync(join(tmpdir(), "wyrd-ts-no-checkout-")));
  try {
    const replayed = await evidence.publish("ci", { status: "green" });
    expect([replayed.status, replayed.evidence_id, replayed.recorded_at]).toEqual([
      "replayed",
      accepted.evidence_id,
      accepted.recorded_at,
    ]);
  } finally {
    process.chdir(cwd);
    vi.unstubAllEnvs();
  }

  for (const [value, repository, code] of [
    [{ status: "red" }, "acme/web", "WYRD_VERIFICATION_409_EVIDENCE_CONFLICT"],
    [{ status: "green" }, "acme/vault", "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE"],
  ] as const) {
    const refused = await evidence.publish("ci", value, { repository, commit: HEAD }).catch((error: unknown) => error);
    expect(refused).toBeInstanceOf(WyrdError);
    expect((refused as WyrdError).code).toBe(code);
  }
});

test("another principal approves the pending signoff and the same execution passes", async ({ server, admin }) => {
  const cards = await Cards.connect({ client: admin });
  const receipt = await cards.declareFromPath(fixture("cards/approvals/repository.yaml"));
  const [binding] = (await cards.get(receipt.root)).status?.verification?.binding_ids ?? [];
  const github = server.githubApp;
  github.setPullRequest(2, 7, "b1".repeat(20), HEAD);
  expect(github.deliverPullRequest(server.baseUrl, 101, 2, 7, "opened")).toBe(202);
  const response = await fetch(`${server.baseUrl}/v1/verification/bindings/${binding}`, {
    headers: { "x-wyrd-access-token": `Bearer ${await admin.accessToken()}` },
  });
  const { last_execution_id: id } = (await response.json()) as { last_execution_id: string };
  const execution = await verify.Execution.connect(id, { client: admin });
  const waiting = await until(execution, "waiting");
  expect(waiting.waiting).toEqual(["ci"]);
  expect(waiting.approvals).toHaveLength(1);
  const [pending] = waiting.approvals ?? [];
  expect(pending).toMatchObject({ step_id: "signoff", message: "Ship this change?", status: "pending" });

  const evidence = await verify.Evidence.connect({ client: admin });
  await evidence.publish("ci", { status: "green" }, { repository: "acme/web", commit: HEAD });
  const deadline = Date.now() + 30_000;
  for (;;) {
    const state = await execution.status();
    if (state.status === "waiting" && (state.waiting ?? []).length === 0) {
      break;
    }
    expect(Date.now(), JSON.stringify(state)).toBeLessThan(deadline);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  const as = async (name: string, role: string): Promise<verify.Execution> =>
    verify.Execution.connect(id, { client: WyrdClient.connect({ credential: server.bootstrapService([role], name) }) });
  const refused = await (await as("signoff_viewer", "viewer"))
    .decide(pending.approval_id, "approved")
    .catch((error: unknown) => error);
  expect(refused).toBeInstanceOf(WyrdError);
  expect((refused as WyrdError).code).toBe("WYRD_PERMISSION_403_DENIED_RBAC");

  const editorRefused = await (await as("signoff_editor", "editor"))
    .decide(pending.approval_id, "approved")
    .catch((error: unknown) => error);
  expect(editorRefused).toBeInstanceOf(WyrdError);
  expect((editorRefused as WyrdError).code).toBe("WYRD_PERMISSION_403_DENIED_RBAC");

  const principals = await Principals.connect({ client: admin });
  const created = await principals.createServicePrincipal({
    name: "signoff-approver",
    assignments: [{ role: "approver", scope: { card: { family: { kind: "Verifier", name: "signoff" } } } }],
  });
  const approver = await verify.Execution.connect(id, {
    client: WyrdClient.connect({ credential: created.credential }),
  });
  await expect(approver.decide(pending.approval_id, "approved")).rejects.toMatchObject({
    code: "WYRD_PERMISSION_403_DENIED_RBAC",
  });

  await principals.bind({
    subject: { principal: { id: created.principal_id, kind: "service" } },
    role: "viewer",
    scope: { card: { family: { kind: "Repository", name: "web" } } },
  });
  const decided = await approver.decide(pending.approval_id, "approved", "reviewed the diff");
  expect(decided).toMatchObject({ approval_id: pending.approval_id, status: "approved", reason: "reviewed the diff" });
  expect(decided.actor_id).toBeTruthy();
  expect(decided.decided_at).toBeTruthy();
  expect(await approver.decide(pending.approval_id, "approved", "reviewed the diff")).toEqual(decided);

  expect((await execution.wait({ timeoutMs: 30_000 })).outcome.verdict).toBe("passed");
  const [settled] = (await execution.status()).approvals ?? [];
  expect(settled).toMatchObject({ status: "approved", actor_id: decided.actor_id });
});
