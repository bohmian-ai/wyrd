// A deployment without a configured GitHub App refuses every connection call
// with one stable code. Separate from `github-connection` because a story
// file shares one server.
import { expect, vi } from "vitest";

import { GithubConnection } from "wyrd-ai";

import { serverTest } from "../support/server.js";

vi.setConfig({ testTimeout: 60_000 });

const test = serverTest();

test("an unconfigured deployment refuses to connect, read or disconnect", async ({ server: _ }) => {
  const github = await GithubConnection.connect();

  for (const call of [() => github.begin(), () => github.get(), () => github.disconnect()]) {
    await expect(call()).rejects.toMatchObject({ code: "WYRD_GITHUB_503_NOT_CONFIGURED", status: 503 });
  }
});
