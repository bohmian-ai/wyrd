import { expect, test } from "vitest";

import { cli } from "@wyrd/sdk";

test("cli plan of a missing card tree raises its catalog code", () => {
  expect(() => cli.plan("/nonexistent/wyrd-card.yaml")).toThrow(
    expect.objectContaining({ code: "WYRD_LOADER_400_INVALID_ENVELOPE" }),
  );
});
