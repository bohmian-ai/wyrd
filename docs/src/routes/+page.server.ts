import { highlight, sliceRegion, tokenLines } from '$lib/mdsvex/highlight.js';

// The landing page shows the Quickstart's finance release example, which the
// SDK journeys run: its programs and API come from examples/finance-release.
// Vite inlines them at build time, so the built server reads no files.
const sources: Record<string, string> = import.meta.glob('../../../examples/finance-release/*.{py,rs,ts}', {
  query: '?raw',
  import: 'default',
  eager: true
});

const finance = (file: string) => sources[`../../../examples/finance-release/${file}`];
const region = (file: string, name: string) => sliceRegion(finance(file), name, `examples/finance-release/${file}`);

// What the Quickstart program prints, as the journeys assert it.
const OUTPUT = `quarterly-report: passed (release-check@0.1.0) -> released
report-with-bank-account: failed (release-check@0.1.0) -> held for review
  follows-policy failed: the report names a bank account identifier`;

export function load() {
  const programs = [
    { lang: 'python', file: 'finance_release.py', title: 'finance_release.py' },
    { lang: 'rust', file: 'finance_release.rs', title: 'src/main.rs' },
    { lang: 'typescript', file: 'finance_release.ts', title: 'finance_release.ts' }
  ] as const;
  // The finance API's import and handler, with the blank line Python puts between them.
  const api = `${region('api.py', 'import')}\n\n${region('api.py', 'handler')}`;
  return {
    flow: tokenLines(api, 'python'),
    declare: { title: 'finance_release.py', html: highlight(region('finance_release.py', 'requirements'), 'python') },
    verify: programs.map((p) => ({ lang: p.lang, title: p.title, html: highlight(finance(p.file), p.lang) })),
    decide: { title: 'output', html: highlight(OUTPUT, 'text') }
  };
}
