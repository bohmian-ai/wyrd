/** Paths and names shared by the API, the scripts, and the tests. */

import { resolve } from "node:path";

/** The Cards this repository declares. */
export const SERVICE_DIR = resolve(import.meta.dirname, "../service");

/** Where `pnpm run deploy` pulls the declared Service; `BUNDLE_DIR` overrides it. */
export const BUNDLE_DIR = process.env.BUNDLE_DIR ?? resolve(import.meta.dirname, "../.wyrd-bundle");

/** The Bifrost table the Service declares. */
export const TICKETS = "bifrost.datasets.tickets";
