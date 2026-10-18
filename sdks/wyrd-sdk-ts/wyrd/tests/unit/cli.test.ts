import { expect, test } from "vitest";

import { cli } from "@wyrd/testing";

test("cli declare check of a missing card tree raises its catalog code", () => {
  expect(() => cli.declareCheck("/nonexistent/wyrd-card.yaml")).toThrow(
    expect.objectContaining({ code: "WYRD_LOADER_400_INVALID_ENVELOPE" }),
  );
});

test("the published sdk exports no in-process cli", async () => {
  expect("cli" in (await import("wyrd-ai"))).toBe(false);
});
