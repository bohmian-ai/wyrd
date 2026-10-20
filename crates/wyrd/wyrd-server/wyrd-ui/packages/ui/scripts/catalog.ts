// Generates src/lib/catalog.json from the component schemas in src/lib/schemas.ts, and
// src/lib/problems.json from the Rust error catalog's browser-safe problem projection.
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
const problems = readFileSync(new URL('../../../../../../wyrd-spec/schemas/ui_problem_examples.json', import.meta.url), 'utf8');
const outputs: [URL, string][] = [
  [path, `${JSON.stringify(catalog, null, 2)}\n`],
  [new URL('../src/lib/problems.json', import.meta.url), problems]
];

for (const [file, content] of outputs) {
  if (process.argv.includes('--check')) {
    let current = '';
    try {
      current = readFileSync(file, 'utf8');
    } catch {}
    if (current !== content) {
      console.error(`${file.pathname.split('/').pop()} drift - run \`pnpm --filter wyrd-ai-ui catalog\` and commit`);
      process.exit(1);
    }
  } else {
    writeFileSync(file, content);
  }
}
