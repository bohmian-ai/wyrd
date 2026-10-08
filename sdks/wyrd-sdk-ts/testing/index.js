// Test-only entry of @wyrd/testing: the in-process server harness and the
// in-process `wyrd` CLI. The native binding is generated into native.cjs.
import { createRequire } from "node:module";

import { WyrdError } from "@wyrd/sdk";

const native = createRequire(import.meta.url)("./native.cjs");

export const { NativeWyrdTestServer, startTestServer } = native;

/** Read the explicit client a command runs as, or none for the ambient chain. */
async function connection(options) {
  const client = options?.client;
  if (client === undefined) {
    return undefined;
  }
  return { serverUrl: client.serverUrl, grpcUrl: client.grpcUrl, accessToken: await client.accessToken() };
}

/** Return a command's value, or throw its catalog problem as a `WyrdError`. */
function value(outcome) {
  if (outcome.problemJson != null) {
    const problem = JSON.parse(outcome.problemJson);
    throw new WyrdError(
      problem.code,
      problem.status,
      problem.title,
      problem.detail,
      problem.remediation,
      problem.details,
    );
  }
  return JSON.parse(outcome.valueJson);
}

export const cli = {
  plan(path) {
    return value(native.cliPlan(path));
  },
  async apply(path, options = {}) {
    return value(await native.cliApply(path, await connection(options)));
  },
  async get(selector, outputDir, options = {}) {
    return value(await native.cliGet(selector, outputDir, options.metadataOnly, await connection(options)));
  },
  async load(selector, options = {}) {
    return value(await native.cliLoad(selector, options.path, await connection(options)));
  },
  async issueKey(card, options = {}) {
    return value(await native.cliIssueKey(card, await connection(options)));
  },
  async grantRole(grant, options = {}) {
    return value(await native.cliGrantRole(grant, await connection(options)));
  },
  async putProviderCredential(write, options = {}) {
    return value(await native.cliPutProviderCredential(write, await connection(options)));
  },
  async revokeProviderCredential(name, options = {}) {
    return value(await native.cliRevokeProviderCredential(name, await connection(options)));
  },
  async deleteProviderCredential(name, options = {}) {
    value(await native.cliDeleteProviderCredential(name, await connection(options)));
  },
};
