/**
 * Declare the Cards in `service/` and pull the Service into the bundle
 * the API loads. Registering the Service creates its `tickets` table. A
 * repeat deploy replaces the bundle only once the new pull is complete, so a
 * refused pull keeps the previous one.
 *
 *   pnpm run deploy
 */

import { join } from "node:path";

import { Cards, Gateway, WyrdClient } from "wyrd-ai";

import { BUNDLE_DIR, SERVICE_DIR } from "./wyrd.ts";

/** Declare and pull the Service as `client`, and capture gateway call metadata. */
export async function deploy(client: WyrdClient, bundle: string = BUNDLE_DIR) {
  // ANCHOR: deploy
  const cards = await Cards.connect({ client });
  const receipt = await cards.declareFromPath(join(SERVICE_DIR, "support-desk.yaml"));
  await cards.pull(receipt.root, bundle);
  // Record each gateway call's metadata, never its content, in vala.gateway.calls.
  await (await Gateway.connect({ client })).putCapturePolicy({ mode: "metadata", payload_fields: [] });
  // ANCHOR_END: deploy
  return receipt;
}

if (import.meta.main) {
  const receipt = await deploy(WyrdClient.connect());
  for (const { card_ref: ref } of receipt.outcomes) console.log(`declared ${ref.kind} ${ref.space}/${ref.name} ${ref.version}`);
  console.log(`pulled into ${BUNDLE_DIR}`);
}
