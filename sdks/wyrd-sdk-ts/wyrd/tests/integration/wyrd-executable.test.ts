// TypeScript's own surface: the `wyrd` executable `@wyrd/sdk` installs. Not one
// of the shared stories in `fixtures/README.md`; it alone runs a child process,
// because running the installed executable is what it proves.
import { execFile } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

import { expect, vi } from "vitest";

import { Cards, type RegistrationReceipt } from "@wyrd/sdk";

import { fixture, serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const DESK = fixture("cards/register_and_hydrate/support-desk.yaml");

/** The `wyrd` executable the package installs, resolved from its `bin` entry. */
const WYRD_BIN = (() => {
  const root = resolve(import.meta.dirname, "../..");
  const { bin } = JSON.parse(readFileSync(join(root, "package.json"), "utf8")) as { bin: { wyrd: string } };
  return join(root, bin.wyrd);
})();

/** The support desk graph, registered after the artifact-bearing Model it references. */
const test = serverTest().extend<{ desk: RegistrationReceipt }>({
  desk: [
    async ({ server: _ }, use) => {
      const cards = Cards.connect();
      await cards.registerFromPath(fixture("cards/register_and_hydrate/support-model.yaml"));
      await use(await cards.registerFromPath(DESK));
    },
    { scope: "file" },
  ],
});

test("installed wyrd executable applies the graph", async ({ desk }) => {
  const { stdout } = await promisify(execFile)(process.execPath, [WYRD_BIN, "apply", DESK, "--format", "json"]);

  expect(JSON.parse(stdout)).toMatchObject({ root: desk.root });
});
