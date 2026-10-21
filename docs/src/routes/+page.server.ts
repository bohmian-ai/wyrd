import { highlight } from '$lib/mdsvex/highlight.js';

// The landing page's Declare → Observe → Verify example is the Quickstart:
// its Cards and programs come from examples/quickstart, which the SDK journeys
// run. Vite inlines them at build time, so the built server reads no files.
const sources: Record<string, string> = import.meta.glob('../../../examples/quickstart/*', {
  query: '?raw',
  import: 'default',
  eager: true
});

const quickstart = (file: string) => sources[`../../../examples/quickstart/${file}`];

// The `VerificationResult` that `verify` returns for the Quickstart's failing
// answer. The wire shape is in crates/wyrd-spec/src/verification/execution.rs;
// the reason and actual value are what the journeys assert. IDs are illustrative.
const RESULT = JSON.stringify(
  {
    execution_id: '019a3c2e-7b41-7d2a-9c1e-4f6b8a2d0e13',
    run_id: '019a3c2e-7a90-7f11-8b3d-2e5c9d1a6f47',
    verifier: { kind: 'Verifier', name: 'no-refund-promise', version: '1.0.0', space: 'default', uid: '019a3c2e-6f12-7c4d-8e31-5a9b0c7d2e64' },
    subject: { kind: 'Service', name: 'support-reply', version: '1.0.0', space: 'default', uid: '019a3c2e-6f0a-7b22-9d4e-1c8f3a5b6d70' },
    outcome: {
      verdict: 'failed',
      requirements: [
        {
          id: 'no-refund-promise',
          statement: 'The answer never promises a guaranteed refund',
          outcome: 'failed',
          reason: 'operator NotContains returned false',
          actual: 'You have a guaranteed refund.'
        }
      ]
    }
  },
  null,
  2
);

export function load() {
  const programs = [
    { lang: 'python', file: 'quickstart.py', title: 'quickstart.py' },
    { lang: 'rust', file: 'quickstart.rs', title: 'src/main.rs' },
    { lang: 'typescript', file: 'quickstart.ts', title: 'quickstart.ts' }
  ] as const;
  return {
    declare: {
      service: { title: 'support-reply.yaml', html: highlight(quickstart('support-reply.yaml'), 'yaml') },
      verifier: { title: 'no-refund-promise.yaml', html: highlight(quickstart('no-refund-promise.yaml'), 'yaml') }
    },
    observe: programs.map((p) => ({ lang: p.lang, title: p.title, html: highlight(quickstart(p.file), p.lang) })),
    verify: { title: 'verify → result', html: highlight(RESULT, 'json') }
  };
}
