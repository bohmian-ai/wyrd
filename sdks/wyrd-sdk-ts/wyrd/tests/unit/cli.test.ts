import { createRequire } from "node:module";

import { expect, test } from "vitest";

import { cli } from "@wyrd/testing";

test("cli plan of a missing card tree raises its catalog code", () => {
  expect(() => cli.plan("/nonexistent/wyrd-card.yaml")).toThrow(
    expect.objectContaining({ code: "WYRD_LOADER_400_INVALID_ENVELOPE" }),
  );
});

test("the published sdk exports no in-process cli", async () => {
  expect("cli" in (await import("@wyrd/sdk"))).toBe(false);
});

test("the installed cli projects wyrd server install", async () => {
  const { runWyrdCli } = createRequire(import.meta.url)("../../index.cjs") as typeof import("../../index.cjs");
  expect(await runWyrdCli(["wyrd", "server", "install", "--help"])).toBe(0);
});
