import { describe, expect, it } from "vitest";

import { cli, WyrdError } from "@wyrd/sdk";

describe("cli", () => {
  it("throws a catalog error instead of exiting for a missing Card tree", () => {
    const error = (() => {
      try {
        cli.plan("/nonexistent/wyrd-card.yaml");
      } catch (reason: unknown) {
        return reason;
      }
    })();
    expect(error).toBeInstanceOf(WyrdError);
    expect((error as WyrdError).code).toBe("WYRD_LOADER_400_INVALID_ENVELOPE");
  });
});
