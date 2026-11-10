// A tenant administrator connects the deployment's GitHub App from Wyrd,
// completes it in a browser signed in to Wyrd, reads and disconnects it; the
// callback is one-use, only the beginning principal's browser can complete
// it, and a viewer can neither connect nor read.
import { expect, vi } from "vitest";

import { GithubConnection, type GithubConnectionView, WyrdClient, WyrdError } from "wyrd-ai";
import { type NativeBrowserAnswer, type NativeGithubAppMock, signInGithubBrowser } from "@wyrd/testing";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** Installation `101`'s repositories; the authorizing user cannot reach `acme/vault`. */
const ACME = [
  { id: 1, fullName: "acme/api" },
  { id: 2, fullName: "acme/web" },
  { id: 3, fullName: "acme/vault" },
] as const;

/** The repositories the authorizing user can reach, as Wyrd reports them. */
const ACCESSIBLE = [
  { repository_id: "1", full_name: "acme/api" },
  { repository_id: "2", full_name: "acme/web" },
] as const;

/** The administrator's handle and browser, signed in as one principal, and the scripted GitHub. */
const test = serverTest({ githubApp: true }).extend<{
  github: NativeGithubAppMock;
  admin: { sdk: GithubConnection; key: string };
}>({
  github: [
    async ({ server }, use) => {
      const github = server.githubApp;
      github.addInstallation(101, [...ACME]);
      github.addUser("octo", [101]);
      github.hideRepository("octo", 3);
      github.signIn("octo");
      github.installTarget(101);
      await use(github);
    },
    { scope: "file" },
  ],
  admin: [
    async ({ server }, use) => {
      const key = server.bootstrapService(["admin"], "github_admin");
      await use({ sdk: await GithubConnection.connect({ client: WyrdClient.connect({ credential: key }) }), key });
    },
    { scope: "file" },
  ],
});

/** Fail unless `answer` is a refusal with `status`, `code` and `details.reason`. */
function expectRefused(answer: NativeBrowserAnswer, status: number, code: string, reason: string): void {
  expect(answer).toMatchObject({ status, body: { code, details: { reason } } });
}

test("admin connects, reads and disconnects the GitHub App", async ({ server, github, admin }) => {
  const browser = signInGithubBrowser(server.baseUrl, server.tenantSlug, admin.key);

  const receipt = await admin.sdk.begin();
  const answer = browser.open(receipt.authorization_url);
  expect(answer.status).toBe(200);
  const view = answer.body as GithubConnectionView;
  expect(view).toMatchObject({
    host: github.host,
    app: { id: 4242, slug: "wyrd-test-app" },
    state: "active",
    installation_id: 101,
    repositories: ACCESSIBLE,
  });
  expect(view.connected_at).toEqual(expect.any(String));
  expect(await admin.sdk.get()).toEqual(view);

  expect(answer.callbackUrl).toEqual(expect.any(String));
  const replay = browser.open(answer.callbackUrl as string);
  expectRefused(replay, 403, "WYRD_GITHUB_403_CONNECTION_REFUSED", "transaction_invalid");
  expect(await admin.sdk.get()).toEqual(view);

  const released = await admin.sdk.disconnect();
  expect(released).toMatchObject({ state: "not_connected", repositories: [] });
  expect(released.installation_id ?? null).toBeNull();
  expect(await admin.sdk.get()).toMatchObject({ state: "not_connected" });
  const again = admin.sdk.disconnect();
  await expect(again).rejects.toBeInstanceOf(WyrdError);
  await expect(again).rejects.toMatchObject({ code: "WYRD_GITHUB_404_NOT_CONNECTED", status: 404 });

  const text = JSON.stringify([
    receipt,
    answer.body,
    replay.body,
    released,
    await again.catch((error: unknown) => error),
  ]);
  for (const secret of github.secrets()) {
    expect(text).not.toContain(secret);
  }
});

test("another principal's or tenant's browser and a viewer are refused", async ({ server, github: _, admin }) => {
  const viewer = await GithubConnection.connect({
    client: WyrdClient.connect({ credential: server.bootstrapService(["viewer"], "github_viewer") }),
  });
  await expect(viewer.begin()).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });
  await expect(viewer.get()).rejects.toMatchObject({ code: "WYRD_PERMISSION_403_DENIED_RBAC" });

  const otherKey = server.bootstrapService(["admin"], "github_other_admin");
  const tenantB = server.seedTenant("github-b");
  const tenantBKey = server.bootstrapServiceInTenant(tenantB, ["admin"], "github_b_admin");
  const intruders = [
    signInGithubBrowser(server.baseUrl, server.tenantSlug, otherKey),
    signInGithubBrowser(server.baseUrl, "github-b", tenantBKey),
  ];
  for (const intruder of intruders) {
    const receipt = await admin.sdk.begin();
    expectRefused(
      intruder.open(receipt.authorization_url),
      403,
      "WYRD_GITHUB_403_CONNECTION_REFUSED",
      "transaction_invalid",
    );
  }
  expect(await admin.sdk.get()).toMatchObject({ state: "not_connected" });
  const tenantBSdk = await GithubConnection.connect({ client: WyrdClient.connect({ credential: tenantBKey }) });
  expect(await tenantBSdk.get()).toMatchObject({ state: "not_connected" });
});
