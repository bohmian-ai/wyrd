#!/usr/bin/env node
// The installed `wyrd` executable: the same Rust dispatcher and renderer as
// every other `wyrd` binary, exiting with its process code.
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const { runWyrdCli } = require("../index.cjs") as typeof import("../index.cjs");

process.exitCode = await runWyrdCli(["wyrd", ...process.argv.slice(2)]);
