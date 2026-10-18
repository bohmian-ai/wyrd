/**
 * Register the Cards in `service/` and hydrate the Service into the bundle
 * the API loads. Registering the Service creates its `tickets` table.
 *
 *   npm run deploy
 */

import { rmSync } from "node:fs";
import { join } from "node:path";

import { Cards, Gateway, WyrdClient } from "wyrd-ai";

import { BUNDLE_DIR, SERVICE_DIR } from "./wyrd.ts";

/** Register and hydrate the Service as `client`, and capture gateway call metadata. */
export async function deploy(client: WyrdClient, bundle: string = BUNDLE_DIR) {
  // ANCHOR: deploy
  const cards = await Cards.connect({ client });
  const receipt = await cards.declareFromPath(join(SERVICE_DIR, "support-desk.yaml"));
  rmSync(bundle, { recursive: true, force: true });
  await cards.hydrate(receipt.root, bundle);
  // Record each gateway call's metadata, never its content, in vala.gateway.calls.
  await (await Gateway.connect({ client })).putCapturePolicy({ mode: "metadata", payload_fields: [] });
  // ANCHOR_END: deploy
  return receipt;
}

if (import.meta.main) {
  const receipt = await deploy(WyrdClient.connect());
  for (const { card_ref: ref } of receipt.outcomes) console.log(`registered ${ref.kind} ${ref.space}/${ref.name} ${ref.version}`);
  console.log(`hydrated into ${BUNDLE_DIR}`);
}
