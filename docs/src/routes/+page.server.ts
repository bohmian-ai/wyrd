import { highlight, sliceRegion, stripAnchors, tokenLines } from '$lib/mdsvex/highlight.js';

// The landing page shows slices of the Quickstart's report-release example,
// which the SDK journeys run: the finance API, the Verifier in each language
// and in YAML, the verify call, and its output. The full program lives only on
// the Quickstart. Vite inlines the sources at build time, so the built server
// reads no files.
const sources: Record<string, string> = import.meta.glob('../../../examples/finance-release/*.{py,rs,ts,txt,yaml}', {
  query: '?raw',
  import: 'default',
  eager: true
});

const finance = (file: string) => sources[`../../../examples/finance-release/${file}`];
const region = (file: string, name: string) => stripAnchors(sliceRegion(finance(file), name, `examples/finance-release/${file}`));

const LANGS = [
  { lang: 'python', ext: 'py', declare: 'finance_service.py', observe: 'finance_release.py' },
  { lang: 'rust', ext: 'rs', declare: 'src/finance_service.rs', observe: 'src/main.rs' },
  { lang: 'typescript', ext: 'ts', declare: 'finance_service.ts', observe: 'finance_release.ts' }
] as const;

export function load() {
  // The finance API's import and handler, with the blank line Python puts between them.
  const api = `${region('api.py', 'import')}\n\n${region('api.py', 'handler')}`;
  return {
    flow: tokenLines(api, 'python'),
    declare: [
      ...LANGS.map((l) => ({ lang: l.lang, title: l.declare, html: highlight(region(`finance_service.${l.ext}`, 'verifier'), l.lang) })),
      { lang: 'yaml' as const, title: 'release-check.yaml', html: highlight(region('release-check.yaml', 'verifier'), 'yaml') }
    ],
    observe: LANGS.map((l) => ({ lang: l.lang, title: l.observe, html: highlight(region(`finance_release.${l.ext}`, 'verify'), l.lang) })),
    verify: { title: 'output', html: highlight(finance('output.txt'), 'text') }
  };
}
