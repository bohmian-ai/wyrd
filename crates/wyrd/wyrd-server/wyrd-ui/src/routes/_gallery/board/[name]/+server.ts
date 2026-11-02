import { dev } from '$app/environment';
import { error } from '@sveltejs/kit';
import { readFile } from 'node:fs/promises';

/** The approved canvas boards, read from the change packet in this checkout. */
const BOARDS = new URL('../../../../../../../../../changes/active/wyrd-ui-main-integration/references/canvas/png/', import.meta.url);

/** One canvas board PNG for the gallery's checkpoint labels; development only. */
export async function GET({ params }) {
  if (!dev || !/^[A-Za-z]+$/.test(params.name)) error(404);
  const image = await readFile(new URL(`${params.name}.png`, BOARDS)).catch(() => error(404));
  return new Response(new Uint8Array(image), { headers: { 'content-type': 'image/png' } });
}
