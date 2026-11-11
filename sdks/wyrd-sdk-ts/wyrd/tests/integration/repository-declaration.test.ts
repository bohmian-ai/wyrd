// A tenant declares its connected repository's review policy from `wyrd.toml`
// or a Repository Card: Wyrd verifies the repository against the GitHub
// connection, registers only the listed Verifiers, and selects the declared
// version only once the declaration completes.
import { copyFileSync, cpSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { expect, vi } from "vitest";

import { type CardRef, Cards, GithubConnection, type RepositoryStatus, WyrdClient } from "wyrd-ai";
import { cli, signInGithubBrowser } from "@wyrd/testing";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

/** A fresh deployment per test whose tenant connected installation `101` with `acme/api` and `acme/web`. */
const test = serverTest({ githubApp: true, perTest: true }).extend<{
  admin: WyrdClient;
  cards: Cards;
}>({
  admin: async ({ server }, use) => {
    const github = server.githubApp;
    github.addInstallation(101, [
      { id: 1, fullName: "acme/api" },
      { id: 2, fullName: "acme/web" },
    ]);
    github.addUser("octo", [101]);
    github.signIn("octo");
    github.installTarget(101);
    const key = server.bootstrapService(["admin"], "repository_admin");
    const admin = WyrdClient.connect({ credential: key });
    const receipt = await (await GithubConnection.connect({ client: admin })).begin();
    expect(signInGithubBrowser(server.baseUrl, server.tenantSlug, key).open(receipt.authorization_url).status).toBe(
      200,
    );
    await use(admin);
  },
  cards: async ({ admin }, use) => use(await Cards.connect({ client: admin })),
});

/** A copy of the story fixtures whose origin is `fullName` on the mock's host. */
function checkout(host: string, fullName: string): string {
  const root = mkdtempSync(join(tmpdir(), "wyrd-ts-checkout-"));
  cpSync(fixture("cards/repository_declaration"), root, { recursive: true });
  mkdirSync(join(root, ".git"));
  writeFileSync(join(root, ".git/config"), `[remote "origin"]\n\turl = http://${host}/${fullName}.git\n`);
  return root;
}

/** Make `policies/<variant>.toml` the checkout's `wyrd.toml` and return its path. */
function policy(root: string, variant: string): string {
  const toml = join(root, "wyrd.toml");
  copyFileSync(join(root, "policies", `${variant}.toml`), toml);
  return toml;
}

/** The served Repository selection and binding count of `ref`. */
async function repository(cards: Cards, ref: CardRef): Promise<[RepositoryStatus | null | undefined, number]> {
  const status = (await cards.get(ref)).status;
  return [status?.repository, status?.verification?.binding_ids?.length ?? 0];
}

test("toml policy is selected only once declared", async ({ server, cards, admin }) => {
  await cards.declareFromPath(fixture("cards/verify_in_real_time/answer-check.yaml"));
  const root = checkout(server.githubApp.host, "acme/api");

  const first = await cards.declareFromPath(policy(root, "minimal"));
  expect(first.root).toMatchObject({ kind: "Repository", name: "api" });
  expect(first.outcomes.map((outcome) => outcome.card_ref.name)).toEqual(["code-review", "api"]);
  expect(await repository(cards, first.root)).toEqual([
    { selected: true, selected_version: first.root.version, full_name: "acme/api" },
    1,
  ]);
  expect((await cards.declareFromPath(policy(root, "minimal"))).root).toEqual(first.root);

  const changed = await cards.declareFromPath(policy(root, "changed"));
  expect(changed.root.version).not.toBe(first.root.version);
  expect(await repository(cards, changed.root)).toEqual([
    { selected: true, selected_version: changed.root.version, full_name: "acme/api" },
    2,
  ]);
  expect((await repository(cards, first.root))[0]?.selected).toBe(false);

  await expect(cards.declareFromPath(policy(root, "pinned"))).rejects.toMatchObject({
    code: "WYRD_REGISTRY_409_SPEC_DRIFT",
  });
  await expect(cards.declareFromPath(policy(root, "missing"))).rejects.toMatchObject({
    code: "WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY",
  });
  expect((await repository(cards, changed.root))[0]?.selected).toBe(true);

  expect((await cards.declareFromPath(policy(root, "rollback"))).root).toEqual(first.root);
  expect((await repository(cards, first.root))[0]?.selected).toBe(true);

  const empty = await cards.declareFromPath(policy(root, "empty"));
  expect(await repository(cards, empty.root)).toEqual([
    { selected: true, selected_version: empty.root.version, full_name: "acme/api" },
    0,
  ]);
  expect((await (await GithubConnection.connect({ client: admin })).get()).state).toBe("active");
});

test("yaml repository declares through the same lifecycle", async ({ cards }) => {
  const receipt = await cards.declareFromPath(fixture("cards/repository_declaration/repository.yaml"));
  expect(receipt.root).toMatchObject({ kind: "Repository", name: "web" });
  const [status, bindings] = await repository(cards, receipt.root);
  expect([status?.selected, status?.full_name, bindings]).toEqual([true, "acme/web", 1]);
});

test("repository identity is verified by the connection", async ({ server, cards }) => {
  await expect(
    cards.declareFromPath(policy(checkout(server.githubApp.host, "acme/vault"), "minimal")),
  ).rejects.toMatchObject({ code: "WYRD_GITHUB_403_REPOSITORY_INACCESSIBLE" });

  const root = checkout(server.githubApp.host, "acme/api");
  await cards.declareFromPath(policy(root, "minimal"));
  await expect(cards.declareFromPath(policy(root, "named"))).rejects.toMatchObject({
    code: "WYRD_REGISTRY_409_REPOSITORY_IDENTITY_CONFLICT",
  });
});

test("cli checks offline and declares the policy", async ({ server, cards, admin }) => {
  const root = checkout(server.githubApp.host, "acme/api");

  const report = cli.declareCheck(policy(root, "minimal"));
  expect(report.cards.map((card) => card.name)).toEqual(["code-review", "api"]);
  expect(report.deferred).toEqual([
    "repository_identity",
    "registered_references",
    "authorization",
    "version_assignment",
  ]);
  const bare = mkdtempSync(join(tmpdir(), "wyrd-ts-bare-"));
  cpSync(fixture("cards/repository_declaration"), bare, { recursive: true });
  expect(() => cli.declareCheck(policy(bare, "minimal"))).toThrow(
    expect.objectContaining({
      code: "WYRD_LOADER_400_INVALID_ENVELOPE",
      details: { diagnostics: [expect.objectContaining({ code: "WYRD_REGISTRY_400_REPOSITORY_IDENTITY_MISSING" })] },
    }),
  );
  await expect(cli.declare(policy(root, "broken"))).rejects.toMatchObject({
    code: "WYRD_LOADER_400_INVALID_ENVELOPE",
  });

  const receipt = await cli.declare(policy(root, "minimal"), { client: admin });
  expect(receipt.root.kind).toBe("Repository");
  expect((await repository(cards, receipt.root))[0]?.selected).toBe(true);
});
