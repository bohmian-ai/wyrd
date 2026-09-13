import { describe, expect, it } from "vitest";

import { WyrdError, type WyrdErrorCode } from "@wyrd/sdk";

describe("WyrdError", () => {
  it("carries a catalog code from the generated WyrdErrorCode union", () => {
    const code: WyrdErrorCode = "WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND";
    const error = new WyrdError(code, 404, "Running query not found", "gone");

    expect(error).toBeInstanceOf(Error);
    expect(error.code).toBe(code);
    expect(error.status).toBe(404);
  });

  it("rejects codes outside the catalog at compile time", () => {
    // @ts-expect-error — not a stable Wyrd catalog code.
    const code: WyrdErrorCode = "WYRD_NOT_A_REAL_CODE";
    expect(code).toBe("WYRD_NOT_A_REAL_CODE");
  });
});
