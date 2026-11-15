import { highlight, sliceRegion, stripAnchors, tokenLines } from '$lib/mdsvex/highlight.js';

// The landing page shows the Quickstart's finance release example, which the
// SDK journeys run: its declarations, programs, output, and API come from
// examples/finance-release. Vite inlines them at build time, so the built
// server reads no files.
const sources: Record<string, string> = import.meta.glob('../../../examples/finance-release/*.{py,rs,ts,txt,yaml}', {
  query: '?raw',
  import: 'default',
  eager: true
});

const finance = (file: string) => sources[`../../../examples/finance-release/${file}`];
const region = (file: string, name: string) => sliceRegion(finance(file), name, `examples/finance-release/${file}`);

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
      ...LANGS.map((l) => ({ lang: l.lang, title: l.declare, html: highlight(stripAnchors(finance(`finance_service.${l.ext}`)), l.lang) })),
      { lang: 'yaml' as const, title: 'release-check.yaml', html: highlight(finance('release-check.yaml'), 'yaml') }
    ],
    observe: LANGS.map((l) => ({ lang: l.lang, title: l.observe, html: highlight(stripAnchors(finance(`finance_release.${l.ext}`)), l.lang) })),
    verify: { title: 'output', html: highlight(finance('output.txt'), 'text') }
  };
}
