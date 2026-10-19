import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, expect, test, vi } from "vitest";

import { Cards, Gateway, type WyrdClient } from "wyrd-ai";

/** The quickstart's TypeScript deploy script, loaded at runtime so it type-checks under its own project. */
const DEPLOY = new URL("../../../../../examples/support-desk-app/typescript/deploy.ts", import.meta.url).href;

afterEach(() => vi.restoreAllMocks());

// Declaration and pull are stand-ins, so only the published `deploy.ts` caller is
// under test: the shared pull owner's staged replacement has its own proof.
test("a refused repeat pull keeps the prior bundle and a successful one replaces it", async () => {
  const { deploy } = (await import(DEPLOY)) as { deploy: (client: WyrdClient, bundle: string) => Promise<unknown> };
  const bundle = join(mkdtempSync(join(tmpdir(), "support-desk-deploy-")), "bundle");
  mkdirSync(bundle);
  writeFileSync(join(bundle, "metadata.yaml"), "prior");
  const pulls = [
    async () => {
      throw new Error("pull refused");
    },
    async (destination: string) => writeFileSync(join(destination, "metadata.yaml"), "replaced"),
  ];
  vi.spyOn(Cards, "connect").mockResolvedValue({
    declareFromPath: async () => ({ root: { kind: "Service", name: "support-desk" }, outcomes: [] }),
    pull: async (_root: unknown, destination: string) => pulls.shift()?.(destination),
  } as unknown as Cards);
  vi.spyOn(Gateway, "connect").mockResolvedValue({ putCapturePolicy: async () => ({}) } as unknown as Gateway);
  const client = {} as WyrdClient;

  await expect(deploy(client, bundle)).rejects.toThrow("pull refused");
  expect(readFileSync(join(bundle, "metadata.yaml"), "utf8")).toBe("prior");
  await deploy(client, bundle);
  expect(readFileSync(join(bundle, "metadata.yaml"), "utf8")).toBe("replaced");
});
