// CommonJS boundary for the generated napi addon. The public facade is ESM,
// but native addons remain unambiguous CommonJS modules for Node consumers.
const fs = require("node:fs");

function isMusl() {
  if (process.platform !== "linux") return false;
  try {
    return fs.readFileSync("/usr/bin/ldd", "utf8").includes("musl");
  } catch {
    return false;
  }
}

const target = (() => {
  if (process.platform === "linux") {
    if (isMusl()) return undefined;
    if (process.arch === "x64") return "linux-x64-gnu";
    if (process.arch === "arm64") return "linux-arm64-gnu";
  }
  if (process.platform === "darwin") {
    if (process.arch === "x64") return "darwin-x64";
    if (process.arch === "arm64") return "darwin-arm64";
  }
  if (process.platform === "win32" && process.arch === "x64") {
    return "win32-x64-msvc";
  }
  return undefined;
})();

if (target === undefined) {
  throw new Error(
    `@wyrd/sdk does not ship a native addon for ${process.platform}/${process.arch}`,
  );
}

let nativeBinding;
const loadErrors = [];
try {
  nativeBinding = require(`./wyrd.${target}.node`);
} catch (error) {
  loadErrors.push(error);
}
if (nativeBinding === undefined) {
  try {
    nativeBinding = require(`@wyrd/sdk-${target}`);
  } catch (error) {
    loadErrors.push(error);
  }
}
if (nativeBinding === undefined) {
  const error = new Error(`Failed to load @wyrd/sdk native addon for ${target}`);
  error.cause = loadErrors.at(-1);
  throw error;
}

module.exports = nativeBinding;
