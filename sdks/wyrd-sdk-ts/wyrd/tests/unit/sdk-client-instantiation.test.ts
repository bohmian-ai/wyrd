import { expect, test } from "vitest";

import { Bifrost, Cards, Gateway, OperatorConnections, Principals } from "wyrd-ai";

// Constructors are private, so a static `connect` is the only way to open a
// server-facing handle, and it is async because it may exchange a credential.
test("exports only the common handle construction forms", () => {
  for (const handle of [Bifrost, Cards, Gateway, OperatorConnections, Principals]) {
    const statics = Object.getOwnPropertyNames(handle).filter(
      (name) => !["length", "name", "prototype"].includes(name),
    );
    expect(statics, handle.name).toEqual(["connect"]);
    expect(handle.connect.constructor.name, handle.name).toBe("AsyncFunction");
  }
});
