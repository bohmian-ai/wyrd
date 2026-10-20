// Generates src/lib/catalog.json from the component schemas in src/lib/schemas.ts.
// Usage: node scripts/catalog.ts [--check]   (--check exits non-zero on drift)
import { readFileSync, writeFileSync } from 'node:fs';
import { z } from 'zod';
import { CATALOG_ID, components, ViewDocumentSchema } from '../src/lib/schemas.ts';

const path = new URL('../src/lib/catalog.json', import.meta.url);
const catalog = {
  catalogId: CATALOG_ID,
  view: z.toJSONSchema(ViewDocumentSchema),
  components: Object.fromEntries(
    Object.entries(components).map(([name, spec]) => [
      name,
      {
        description: spec.description,
        input: z.toJSONSchema(spec.input),
        states: spec.states,
        emits: Object.entries(spec.emits).map(([action, emitted]) => ({
          name: action,
          description: emitted.description,
          input: z.toJSONSchema(emitted.input),
          completion: emitted.completion
        }))
      }
    ])
  )
};
const content = `${JSON.stringify(catalog, null, 2)}\n`;

if (process.argv.includes('--check')) {
  let current = '';
  try {
    current = readFileSync(path, 'utf8');
  } catch {}
  if (current !== content) {
    console.error('catalog.json drift - run `pnpm --filter wyrd-ai-ui catalog` and commit');
    process.exit(1);
  }
} else {
  writeFileSync(path, content);
}
