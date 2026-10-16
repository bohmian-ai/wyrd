/** Paths and names shared by the API, the scripts, and the tests. */

import { resolve } from "node:path";

/** The Cards this repository registers. */
export const SERVICE_DIR = resolve(import.meta.dirname, "../service");

/** The refund policy every answer is checked against. */
export const POLICY = resolve(SERVICE_DIR, "refund-policy.pdf");

/** Where `npm run deploy` hydrates the registered Service; `BUNDLE_DIR` overrides it. */
export const BUNDLE_DIR = process.env.BUNDLE_DIR ?? resolve(import.meta.dirname, "../.wyrd-bundle");

/** The Bifrost table the Service declares. */
export const TICKETS = "vala.datasets.tickets";
