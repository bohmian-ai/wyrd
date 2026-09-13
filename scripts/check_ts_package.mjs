import { cp, lstat, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const exec = promisify(execFile);
const repository = resolve(fileURLToPath(new URL("..", import.meta.url)));
const sdkDir = join(repository, "sdks", "wyrd-sdk-ts", "wyrd");
const target = `${process.platform}-${process.arch}`;
const targetPackage = {
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "linux-arm64": "linux-arm64-gnu",
  "linux-x64": "linux-x64-gnu",
  "win32-x64": "win32-x64-msvc",
}[target];

if (targetPackage === undefined) {
  throw new Error(`unsupported current host for package smoke test: ${target}`);
}

const binary = `wyrd.${targetPackage}.node`;
const sourceBinary = join(sdkDir, binary);
const platformDir = join(repository, "sdks", "wyrd-sdk-ts", `wyrd-${targetPackage}`);
const platformManifest = JSON.parse(
  await readFile(join(platformDir, "package.json"), "utf8"),
);

function collectStrings(value, strings = []) {
  if (typeof value === "string") {
    strings.push(value);
  } else if (Array.isArray(value)) {
    for (const item of value) collectStrings(item, strings);
  } else if (value !== null && typeof value === "object") {
    for (const item of Object.values(value)) collectStrings(item, strings);
  }
  return strings;
}

function assertPublishedPaths(manifest, label) {
  for (const path of collectStrings(manifest)) {
    if (
      /(?:^|[\\/])src(?:[\\/]|$)/.test(path) ||
      path.includes("workspace:") ||
      path.includes("link:") ||
      (path.endsWith(".ts") && !path.endsWith(".d.ts"))
    ) {
      throw new Error(`${label} contains an unpublished source path: ${path}`);
    }
  }
}

function assertNoWorkspaceMetadata(manifest, label) {
  const serialized = JSON.stringify(manifest);
  if (/workspace:|link:|^file:|"resolved"\s*:\s*"(?:workspace|link|file):/.test(serialized)) {
    throw new Error(`${label} contains workspace/link/file resolution metadata`);
  }
}

function assertArchiveInventory(packResult, label, allowedPaths) {
  const paths = packResult.files.map(({ path }) => path).sort();
  const allowed = [...allowedPaths].sort();
  if (JSON.stringify(paths) !== JSON.stringify(allowed)) {
    throw new Error(`${label} inventory mismatch: ${paths.join(", ")}`);
  }
  for (const path of paths) {
    if (path.includes("src/") || path.endsWith(".ts") && !path.endsWith(".d.ts") && !path.endsWith(".d.cts")) {
      throw new Error(`${label} contains source TypeScript: ${path}`);
    }
  }
}

async function walkFiles(root) {
  const files = [];
  async function visit(directory, relative = "") {
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const child = join(directory, entry.name);
      const childRelative = relative === "" ? entry.name : join(relative, entry.name);
      const stats = await lstat(child);
      if (stats.isSymbolicLink()) throw new Error(`installed package contains symlink: ${childRelative}`);
      if (entry.isDirectory()) await visit(child, childRelative);
      else files.push(childRelative.replaceAll("\\", "/"));
    }
  }
  await visit(root);
  return files.sort();
}

const rootAllowed = new Set([
  "dist/error-codes.d.ts",
  "dist/error-codes.js",
  "dist/index.d.ts",
  "dist/index.js",
  "index.cjs",
  "index.d.cts",
  "index.d.ts",
  "package.json",
]);
const addonAllowed = new Set(["package.json", binary]);

assertPublishedPaths(platformManifest, "platform addon manifest");
assertNoWorkspaceMetadata(platformManifest, "platform addon manifest");
const staging = await mkdtemp(join(tmpdir(), "wyrd-ts-pack-"));
const platformStaging = join(staging, "platform");

try {
  await cp(platformDir, platformStaging, { recursive: true });
  await cp(sourceBinary, join(platformStaging, binary));
  const { stdout: sdkPackOutput } = await exec("npm", ["pack", "--json", "--pack-destination", staging], {
    cwd: sdkDir,
  });
  const sdkPack = JSON.parse(sdkPackOutput)[0];
  assertArchiveInventory(sdkPack, "SDK tarball", rootAllowed);
  const sdkTarball = join(staging, sdkPack.filename);
  const { stdout: addonPackOutput } = await exec("npm", ["pack", "--json", "--pack-destination", staging], {
    cwd: platformStaging,
  });
  const addonPack = JSON.parse(addonPackOutput)[0];
  assertArchiveInventory(addonPack, "addon tarball", addonAllowed);
  const addonTarball = join(staging, addonPack.filename);

  await writeFile(
    join(staging, "package.json"),
    JSON.stringify({ type: "module", private: true }, null, 2),
  );
  await exec("npm", [
    "install",
    "--ignore-scripts",
    "--no-audit",
    "--no-fund",
    "--package-lock=false",
    sdkTarball,
    addonTarball,
  ], { cwd: staging });

  const { stdout: importOutput } = await exec(
    process.execPath,
    ["--input-type=module", "-e", `
      import { Bifrost, TableConfig } from "@wyrd/sdk";
      if (typeof Bifrost.connect !== "function") throw new Error("Bifrost facade missing");
      if (typeof TableConfig !== "function") throw new Error("TableConfig missing");
      const resolved = import.meta.resolve("@wyrd/sdk");
      if (resolved.includes(".ts")) throw new Error("published export resolves to TypeScript: " + resolved);
      console.log(resolved);
    `],
    { cwd: staging },
  );

  const installedManifest = JSON.parse(
    await readFile(join(staging, "node_modules", "@wyrd", "sdk", "package.json"), "utf8"),
  );
  assertPublishedPaths(installedManifest, "installed SDK manifest");
  assertNoWorkspaceMetadata(installedManifest, "installed SDK manifest");
  const installedSdkFiles = await walkFiles(join(staging, "node_modules", "@wyrd", "sdk"));
  if (JSON.stringify(installedSdkFiles) !== JSON.stringify([...rootAllowed].sort())) {
    throw new Error(`installed SDK inventory mismatch: ${installedSdkFiles.join(", ")}`);
  }
  const installedAddonRoot = join(staging, "node_modules", "@wyrd", `sdk-${targetPackage}`);
  const installedAddonManifest = JSON.parse(await readFile(join(installedAddonRoot, "package.json"), "utf8"));
  assertNoWorkspaceMetadata(installedAddonManifest, "installed addon manifest");
  const installedAddonFiles = await walkFiles(installedAddonRoot);
  if (JSON.stringify(installedAddonFiles) !== JSON.stringify([...addonAllowed].sort())) {
    throw new Error(`installed addon inventory mismatch: ${installedAddonFiles.join(", ")}`);
  }
  if (!importOutput.includes("dist/index.js")) {
    throw new Error(`installed SDK did not resolve to compiled facade: ${importOutput}`);
  }
  if (platformManifest.main.endsWith(".ts")) {
    throw new Error("platform addon manifest points at TypeScript");
  }
  console.log(`packed @wyrd/sdk with ${platformManifest.name} and imported compiled facade`);
} finally {
  await rm(staging, { recursive: true, force: true });
}
