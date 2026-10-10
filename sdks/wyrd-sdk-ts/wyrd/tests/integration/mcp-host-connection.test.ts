import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { expect, vi } from "vitest";

import { cli } from "@wyrd/testing";

import { serverTest } from "../support/server.js";

const VSCODE_PROFILE = process.platform === "darwin" ? "Library/Application Support/Code/User" : ".config/Code/User";

/** Every host's configuration file in an isolated home, each with an `other` entry. */
const test = serverTest().extend<{ hosts: Record<string, string> }>({
  hosts: async ({ server: _ }, use) => {
    const home = mkdtempSync(join(tmpdir(), "wyrd-ts-mcp-home-"));
    vi.stubEnv("HOME", home);
    for (const name of ["XDG_CONFIG_HOME", "CODEX_HOME", "COPILOT_HOME"]) {
      vi.stubEnv(name, undefined);
    }
    const hosts = {
      codex: join(home, ".codex/config.toml"),
      "claude-code": join(home, ".claude.json"),
      "copilot-cli": join(home, ".copilot/mcp-config.json"),
      vscode: join(home, VSCODE_PROFILE, "mcp.json"),
    };
    mkdirSync(join(home, ".claude"));
    for (const path of Object.values(hosts)) {
      mkdirSync(dirname(path), { recursive: true });
    }
    writeFileSync(hosts.codex, '# mine\n[mcp_servers.other]\ncommand = "other"\n');
    writeFileSync(hosts["claude-code"], '{"mcpServers":{"other":{"command":"other"}}}');
    writeFileSync(hosts["copilot-cli"], '{"mcpServers":{"other":{"command":"other"}}}');
    writeFileSync(hosts.vscode, '{"servers":{"other":{"command":"other"}}}');
    await use(hosts);
  },
});

/** Every host file's current text. */
function snapshot(hosts: Record<string, string>): Record<string, string> {
  return Object.fromEntries(Object.entries(hosts).map(([name, path]) => [name, readFileSync(path, "utf8")]));
}

test("install changes only the selected hosts", ({ hosts, server }) => {
  const before = snapshot(hosts);

  const report = cli.mcpInstall(["claude-code", "vscode"], { server: server.baseUrl });

  expect(report.hosts.map(({ host, status }) => [host, status])).toEqual([
    ["claude-code", "added"],
    ["vscode", "added"],
  ]);
  const after = snapshot(hosts);
  expect(after.codex).toBe(before.codex);
  expect(after["copilot-cli"]).toBe(before["copilot-cli"]);
  const claude = JSON.parse(after["claude-code"]).mcpServers;
  expect(claude.other).toEqual({ command: "other" });
  expect(claude.wyrd).toMatchObject({
    type: "stdio",
    args: ["mcp", "proxy", "--server", server.baseUrl],
    env: { WYRD_CONFIG_HOME: process.env.WYRD_CONFIG_HOME },
  });
  expect(JSON.parse(after.vscode).servers.other).toEqual({ command: "other" });
  for (const text of Object.values(after)) {
    expect(text).not.toContain(server.apiKey);
  }

  expect(cli.mcpInstall(["claude-code"]).hosts[0].status).toBe("updated");
  const settled = snapshot(hosts);
  expect(cli.mcpInstall(["claude-code"]).hosts[0].status).toBe("unchanged");
  expect(snapshot(hosts)).toEqual(settled);
});

test("conflicting and undetected hosts are reported and left alone", ({ hosts }) => {
  writeFileSync(hosts["copilot-cli"], '{"mcpServers":{"wyrd":{"command":"someone-else"}}}');
  rmSync(dirname(hosts.vscode), { recursive: true });
  const before = readFileSync(hosts["copilot-cli"], "utf8");

  const [conflict, undetected, added] = cli.mcpInstall(["copilot-cli", "vscode", "codex"]).hosts;

  expect(conflict).toMatchObject({ status: "conflict", path: hosts["copilot-cli"] });
  expect(conflict.detail).toContain("rename or remove it");
  expect(undetected).toMatchObject({ status: "not_detected", path: null });
  expect(added.status).toBe("added");
  expect(readFileSync(hosts["copilot-cli"], "utf8")).toBe(before);
  expect(existsSync(dirname(hosts.vscode))).toBe(false);
});

test("invalid selections and servers raise without changing a host", ({ hosts }) => {
  const before = snapshot(hosts);

  expect(() => cli.mcpInstall([])).toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }));
  expect(() => cli.mcpInstall(["cursor"])).toThrow(expect.objectContaining({ code: "WYRD_SPEC_400_VALIDATION" }));
  expect(() => cli.mcpInstall(["codex"], { server: "http://wyrd.example" })).toThrow(
    expect.objectContaining({ code: "WYRD_CLIENT_400_CONFIG_INVALID" }),
  );
  expect(snapshot(hosts)).toEqual(before);
});
