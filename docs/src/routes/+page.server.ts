import { highlight, sliceRegion, stripAnchors, tokenLines } from '$lib/mdsvex/highlight.js';

// The landing page shows the Quickstart's finance release example, which the
// SDK journeys run: its declaration, programs, and API come from
// examples/finance-release. Vite inlines them at build time, so the built
// server reads no files.
const sources: Record<string, string> = import.meta.glob('../../../examples/finance-release/*.{py,rs,ts}', {
  query: '?raw',
  import: 'default',
  eager: true
});

const finance = (file: string) => sources[`../../../examples/finance-release/${file}`];
const region = (file: string, name: string) => sliceRegion(finance(file), name, `examples/finance-release/${file}`);

// The `VerificationResult` that `verify` returns for the Quickstart's second
// release, which names a bank account: the wire shape of
// crates/wyrd-spec/src/verification/execution.rs, with illustrative IDs.
const RESULT = JSON.stringify(
  {
    execution_id: '019a3c2e-7b41-7d2a-9c1e-4f6b8a2d0e13',
    run_id: '019a3c2e-7a90-7f11-8b3d-2e5c9d1a6f47',
    verifier: { kind: 'Verifier', name: 'release-check', version: '0.1.0', space: 'default', uid: '019a3c2e-6f12-7c4d-8e31-5a9b0c7d2e64' },
    subject: { kind: 'Service', name: 'finance-release', version: '0.1.0', space: 'default', uid: '019a3c2e-6f0a-7b22-9d4e-1c8f3a5b6d70' },
    outcome: {
      verdict: 'failed',
      requirements: [
        { id: 'recipient-authorized', statement: 'The recipient is authorized to receive the report.', outcome: 'passed', reason: 'operator Equals returned true' },
        { id: 'follows-policy', statement: 'The company sharing policy permits this release.', outcome: 'failed', reason: 'the report names a bank account identifier' },
        { id: 'destination-restricted', statement: 'The destination restricts access to the report.', outcome: 'passed', reason: 'operator Equals returned true' }
      ]
    }
  },
  null,
  2
);

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
    declare: LANGS.map((l) => ({ lang: l.lang, title: l.declare, html: highlight(stripAnchors(finance(`finance_service.${l.ext}`)), l.lang) })),
    observe: LANGS.map((l) => ({ lang: l.lang, title: l.observe, html: highlight(stripAnchors(finance(`finance_release.${l.ext}`)), l.lang) })),
    verify: { title: 'verify → result', html: highlight(RESULT, 'json') }
  };
}
