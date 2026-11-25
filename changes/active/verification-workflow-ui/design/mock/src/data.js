// Mock data only. A team-authored sample Verifier; step ids, references and publication names are
// illustrative and imply no platform kind, schema, or naming convention.

export const PR_REVIEW = {
  name: 'pr-review', version: '1.3.0',
  description: 'A pull request is reviewed by the specialists its change calls for; every finding is independently validated before a written report explains the verdict.',
  inputs: {
    change: { kind: 'directory', sub: 'PR checkout at head' },
    pull_request: { kind: 'json', sub: 'title, body, base, head' },
  },
  steps: {
    extract: { kind: 'code', ref: 'python split_units()', deps: ['change', 'pull_request'], reqs: ['non-empty'],
      desc: 'Split the changed files into review units and tag the paths each unit touches.' },
    'security-review': { kind: 'agent', ref: 'security-reviewer@2.1.0', deps: ['extract'], when: 'steps.extract.output.touches_auth', reqs: [],
      desc: 'Look for authentication, authorization, and secret-handling defects in units that touch auth paths.' },
    'correctness-review': { kind: 'agent', ref: 'correctness-reviewer@1.4.0', deps: ['extract'], when: 'steps.extract.output.has_code', fanout: 'steps.extract.output.units', reqs: [],
      desc: 'Review each unit for logic errors and missed edge cases.' },
    validate: { kind: 'agent', ref: 'finding-validator@1.1.0', deps: ['security-review', 'correctness-review'], reqs: ['converged'],
      repeat: { until: 'steps.validate.output.unconfirmed == 0', max: 3 },
      desc: 'Reproduce or refute every finding against the checkout; repeat until no finding is left unconfirmed.' },
    summarize: { kind: 'agent', ref: 'report-writer@1.0.2', deps: ['validate'], reqs: [],
      desc: 'Write the review report from the validated findings.' },
  },
  reqs: {
    'non-empty': { kind: 'assert', step: 'extract', desc: 'The pull request changes at least one reviewable file', check: 'assert steps.extract.output.units is not empty' },
    converged: { kind: 'assert', step: 'validate', desc: 'Validation settled every finding within its iteration bound', check: 'assert steps.validate.output.unconfirmed equals 0' },
    'no-security-findings': { kind: 'code', reads: 'validate', desc: 'No confirmed security finding', check: 'python no_confirmed(severity="security")' },
    'no-confirmed-defects': { kind: 'code', reads: 'validate', desc: 'No confirmed correctness defect', check: 'python no_confirmed(severity="defect")' },
    'report-explains': { kind: 'agent', reads: 'summarize', desc: 'Report explains each confirmed finding', check: 'agent report-judge@1.0.0' },
  },
  after: ['no-security-findings', 'no-confirmed-defects', 'report-explains'],
  // Dependency edges as authored. Labels name the value a dependent reads where that helps.
  deps: [
    ['change', 'extract'], ['pull_request', 'extract'],
    ['extract', 'security-review', 'units'], ['extract', 'correctness-review', 'units'],
    ['security-review', 'validate', 'findings'], ['correctness-review', 'validate', 'findings'],
    ['validate', 'summarize'],
    ['validate', 'no-security-findings'], ['validate', 'no-confirmed-defects'], ['summarize', 'report-explains'],
  ],
};

/* ---------- Retained sample content ---------- */

const UNITS_A = { touches_auth: true, has_code: true, units: [
  { id: 'u0', paths: ['refunds/eligibility.py', 'refunds/cache.py'] },
  { id: 'u1', paths: ['refunds/merchants.py', 'refunds/policy.py'] },
  { id: 'u2', paths: ['auth/session.py'] },
] };
const MANIFEST_A = { files: [
  { path: 'auth/session.py', sha256: '4be1a0c9d2f37e815a6c0b9e2d4f71a3c8e05b6d9f1a2c3e4b5d6f708192a3b4', size_bytes: 6120 },
  { path: 'refunds/cache.py', sha256: '91c3e7f05ab2d4c6e8f0a1b3c5d7e9f1023456789abcdef0123456789abcdef', size_bytes: 3398 },
  { path: 'refunds/eligibility.py', sha256: 'c07d2e9b81f4a6c3e5d7f9a1b3c5e7d9f1a3b5c7d9e1f3a5b7c9d1e3f5a7b9c1', size_bytes: 8811 },
  { path: 'refunds/merchants.py', sha256: '2f8a6c4e0b9d7f5a3c1e9b7d5f3a1c9e7b5d3f1a9c7e5b3d1f9a7c5e3b1d9f7a', size_bytes: 4087 },
  { path: 'refunds/policy.py', sha256: 'e5b3d1f9a7c5e3b1d9f7a5c3e1b9d7f5a3c1e9b7d5f3a1c9e7b5d3f1a9c7e5b3', size_bytes: 2954 },
], directories: ['auth', 'refunds'] };
const SEC_FINDINGS_A = { findings: [
  { id: 'S1', unit: 'u2', path: 'auth/session.py', line: 41, severity: 'security', claim: 'Eligibility cache key omits the tenant id, so one tenant can read another tenant\'s cached decision.' },
] };
const SEC_NOTES_A = `# Security review notes

Reviewed unit **u2** (\`auth/session.py\`) because \`touches_auth\` is true.

- The new \`eligibility_cache_key()\` builds its key from \`merchant_id\` and \`order_id\` only.
- Merchant ids are unique per tenant, not globally, so two tenants can collide.

One finding recorded as **S1**. Validation must confirm it against the checkout.`;
const CORR_U0 = { findings: [{ id: 'C1', unit: 'u0', path: 'refunds/cache.py', line: 27, severity: 'defect', claim: 'TTL is compared in seconds against a millisecond timestamp, so entries never expire.' }] };
const CORR_U1_PARTIAL = `# Correctness notes (unit u1)

Started reading \`refunds/merchants.py\`. The \`policy_for()\` helper returns \`None\` for merchants without a refund policy; checking whether callers handle it…`;
const CORR_U1 = { findings: [{ id: 'C2', unit: 'u1', path: 'refunds/merchants.py', line: 63, severity: 'defect', claim: 'A merchant without a refund policy falls through to "eligible".' }] };
const VAL0 = { iteration: 0, confirmed: ['S1', 'C1'], refuted: [], unconfirmed: 1, pending: ['C2'] };
const VAL0_NOTES = `# Validation, iteration 0

- **S1** confirmed: a reproduction test shows a cross-tenant cache hit on head.
- **C1** confirmed: an entry outlives twice its TTL.
- **C2** unconfirmed: the fixture set has no merchant without a refund policy.

Repeating once the fixture exists.`;
const VAL1_NOTES = `# Validation, iteration 1 (in progress)

Added a merchant fixture with no refund policy and re-ran the eligibility path for **C2**.

- First run: \`is_eligible()\` returned \`True\` for the policy-less merchant.
- Checking whether \`policy.default_deny\` is applied upstream before confirming.

The PR description embeds raw markup such as <img src=x onerror="alert(1)"> and <script>alert(2)</script>; previews always show it as text.`;
const VAL1 = { iteration: 1, confirmed: ['S1', 'C1', 'C2'], refuted: [], unconfirmed: 0, pending: [] };
const DRAFT_A = `# Review report (draft)

Three confirmed findings: **S1** (security), **C1** and **C2** (correctness). Explanations follow as the writer proceeds.`;

const UNITS_B = { touches_auth: false, has_code: true, units: [
  { id: 'u0', paths: ['refunds/amounts.py'] },
  { id: 'u1', paths: ['refunds/serializers.py', 'tests/test_amounts.py'] },
] };
const DISCARDED_NOTES = `# Security review notes (partial)

Started before \`extract\` reported which paths the change touches. Reading \`refunds/amounts.py\` for secret handling…`;
const CORR_B0 = { findings: [{ id: 'C1', unit: 'u0', path: 'refunds/amounts.py', line: 88, severity: 'defect', claim: 'Rounds half-up before currency conversion, so JPY refunds can exceed the captured amount by 1.' }] };
const CORR_B1 = { findings: [{ id: 'C2', unit: 'u1', path: 'refunds/serializers.py', line: 19, severity: 'defect', claim: 'minor_units may serialize as a float.' }] };
const VAL_B0 = { iteration: 0, confirmed: ['C1'], refuted: [], unconfirmed: 1, pending: ['C2'] };
const VAL_B1 = { iteration: 1, confirmed: ['C1'], refuted: ['C2'], unconfirmed: 0, pending: [] };
const REPORT_B = `# Review report: Round refund amounts to minor units

**Verdict inputs:** 1 confirmed correctness defect, 0 security findings.

## Summary

The change moves refund arithmetic to integer minor units. One defect remains: amounts are rounded **before** currency conversion, so for zero-decimal currencies the refund can exceed what was captured.

## Confirmed findings

### C1 · refunds/amounts.py:88

\`\`\`python
amount = round_half_up(request.amount)          # line 88
minor = to_minor_units(convert(amount, currency))
\`\`\`

- Reproduction: a 1000 JPY capture accepts a 1001 JPY refund.
- Suggested fix: convert first, then round once in the settlement currency.

## Refuted findings

- **C2** (\`refunds/serializers.py:19\`): unreachable, the serializer casts with \`int()\`.

## Not reviewed

- Security review did not apply: \`extract\` found no auth paths. Its speculative work was discarded.

> Markup in the PR body such as <details><summary>x</summary></details> is shown as text, never rendered.`;

/* ---------- Executions ---------- */

export const I = (item, iter, att, state, reason, outs = []) => ({ item, iter, att, state, reason, outs });
export const O = (name, kind, content) => ({ name, kind, content });
const waiting = () => ({ action: 'waiting', disposition: 'none', inst: [] });

/** Sample clock: every timestamp in the mock is relative to this instant (Nov 9 2026 15:00:00 UTC). */
export const NOW = Date.UTC(2026, 10, 9, 15, 0, 0);
export const T = (h, m, s, day = 9, month = 10) => Date.UTC(2026, month, day, h, m, s);
const PR_SUBJECT = { def: 'pr-review@1.3.0', card: 'payments-api', service: null, run: null, session: null, repo: 'acme/payments-api' };

export function runningExecution() {
  return {
    ...PR_SUBJECT, id: 'exe-7f3a91c2', status: 'running', verdict: null, pr: 482, title: 'Cache refund eligibility per merchant', head: '9c1e4b2', at: T(14, 2, 11),
    steps: {
      extract: { action: 'succeeded', disposition: 'none', inst: [I(null, null, 1, 'succeeded', null, [O('units.json', 'json', UNITS_A), O('changed-files', 'directory', MANIFEST_A)])] },
      'security-review': { action: 'succeeded', disposition: 'admitted', inst: [I(null, null, 1, 'succeeded', null, [O('findings.json', 'json', SEC_FINDINGS_A), O('notes.md', 'file', SEC_NOTES_A)])] },
      'correctness-review': { action: 'succeeded', disposition: 'admitted', inst: [
        I(0, null, 1, 'succeeded', null, [O('findings.json', 'json', CORR_U0)]),
        I(1, null, 1, 'failed', 'Agent call timed out after 300 s; the step retried this item.', [O('notes.md', 'file', CORR_U1_PARTIAL)]),
        I(1, null, 2, 'succeeded', null, [O('findings.json', 'json', CORR_U1)]),
        I(2, null, 1, 'succeeded', null, [O('findings.json', 'json', { findings: [] })]),
      ] },
      validate: { action: 'running', disposition: 'none', inst: [
        I(null, 0, 1, 'succeeded', '1 finding left unconfirmed, so the step repeats.', [O('validated.json', 'json', VAL0), O('notes.md', 'file', VAL0_NOTES)]),
        I(null, 1, 1, 'started', null, [O('notes.md', 'file', VAL1_NOTES)]),
      ] },
      summarize: waiting(),
    },
    reqs: { 'non-empty': { outcome: 'passed', reason: 'steps.extract.output.units has 3 entries', actual: '3 units' } },
  };
}

/** Simulated live updates for the running sample: data only, never topology. */
export const UPDATES = [
  {
    label: 'validate iteration 1 settles; summarize starts',
    apply(x) {
      const it = x.steps.validate.inst[1];
      it.state = 'succeeded'; it.reason = 'unconfirmed is 0; repeat condition met.';
      it.outs.push(O('validated.json', 'json', VAL1));
      x.steps.validate.action = 'succeeded';
      x.reqs.converged = { outcome: 'passed', reason: 'unconfirmed equals 0 after iteration 1 of at most 3', actual: '0' };
      x.steps.summarize = { action: 'running', disposition: 'none', inst: [I(null, null, 1, 'started', null)] };
    },
  },
  {
    label: 'summarize accepts a draft publication',
    apply(x) { x.steps.summarize.inst[0].outs.push(O('report-draft.md', 'file', DRAFT_A)); },
  },
];

const REPORT_A = `# Review report: Cache refund eligibility per merchant

**Verdict inputs:** 1 confirmed security finding, 2 confirmed correctness defects.

## Confirmed findings

- **S1** \`auth/session.py:41\`: the eligibility cache key omits the tenant id.
- **C1** \`refunds/cache.py:27\`: TTL is compared in seconds against a millisecond timestamp.
- **C2** \`refunds/merchants.py:63\`: a merchant without a refund policy falls through to "eligible".`;

/** The recorded result the running sample reaches when it finishes before a cancellation lands. Keeps every earlier publication. */
export function finishRunning(x) {
  if (x.steps.validate.action !== 'succeeded') UPDATES[0].apply(x);
  const s = x.steps.summarize.inst[0];
  s.state = 'succeeded';
  s.outs.push(O('report.md', 'file', REPORT_A));
  x.steps.summarize.action = 'succeeded';
  Object.assign(x.reqs, {
    'no-security-findings': { outcome: 'failed', reason: '1 validated finding has severity "security": S1 auth/session.py:41.', actual: '1 confirmed (S1)', cites: [['validate', '-/1/1', 'validated.json']] },
    'no-confirmed-defects': { outcome: 'failed', reason: '2 validated findings are correctness defects: C1 and C2.', actual: '2 confirmed (C1, C2)', cites: [['validate', '-/1/1', 'validated.json']] },
    'report-explains': { outcome: 'passed', reason: '“The report explains S1, C1 and C2 with their locations.”', actual: 'passed', judge: 'report-judge@1.0.0', cites: [['summarize', '-/-/1', 'report.md']] },
  });
  x.status = 'completed';
  x.verdict = 'failed';
}

export function completedExecution() {
  return {
    ...PR_SUBJECT, id: 'exe-5d20c8e1', status: 'completed', verdict: 'failed', pr: 479, title: 'Round refund amounts to minor units', head: '31ab07f', at: T(13, 41, 5),
    steps: {
      extract: { action: 'succeeded', disposition: 'none', inst: [I(null, null, 1, 'succeeded', null, [O('units.json', 'json', UNITS_B)])] },
      'security-review': { action: 'skipped', disposition: 'discarded', reason: 'Condition was false: steps.extract.output.touches_auth is false. Speculative work was cancelled.',
        inst: [I(null, null, 1, 'skipped', 'Cancelled when its condition resolved false.', [O('notes.md', 'file', DISCARDED_NOTES)])] },
      'correctness-review': { action: 'succeeded', disposition: 'admitted', inst: [
        I(0, null, 1, 'succeeded', null, [O('findings.json', 'json', CORR_B0)]),
        I(1, null, 1, 'succeeded', null, [O('findings.json', 'json', CORR_B1)]),
      ] },
      validate: { action: 'succeeded', disposition: 'none', inst: [
        I(null, 0, 1, 'succeeded', '1 finding left unconfirmed, so the step repeated.', [O('validated.json', 'json', VAL_B0)]),
        I(null, 1, 1, 'succeeded', 'unconfirmed is 0; repeat condition met.', [O('validated.json', 'json', VAL_B1)]),
      ] },
      summarize: { action: 'succeeded', disposition: 'none', inst: [I(null, null, 1, 'succeeded', null, [O('report.md', 'file', REPORT_B)])] },
    },
    reqs: {
      'non-empty': { outcome: 'passed', reason: 'steps.extract.output.units has 2 entries', actual: '2 units' },
      converged: { outcome: 'passed', reason: 'unconfirmed equals 0 after iteration 1 of at most 3', actual: '0' },
      'no-security-findings': { outcome: 'passed', reason: 'No validated finding has severity "security".', actual: '0 confirmed' },
      'no-confirmed-defects': { outcome: 'failed', reason: '1 validated finding is a correctness defect: C1 refunds/amounts.py:88 rounds before currency conversion.', actual: '1 confirmed (C1)',
        cites: [['validate', '-/1/1', 'validated.json']] },
      'report-explains': { outcome: 'passed', reason: '“The report explains C1 with a reproduction and states why C2 was refuted.”', actual: 'passed', judge: 'report-judge@1.0.0',
        cites: [['summarize', '-/-/1', 'report.md']] },
    },
  };
}

/* ---------- Vocabulary: wire value → glyph, readable word, token class ---------- */

export const LIFECYCLE = { pending: ['○', 'queued', 'c-muted'], running: ['●', 'running', 'c-run'], completed: ['■', 'completed', 'c-text'],
  errored: ['✕', 'errored', 'c-danger'], timed_out: ['⧗', 'timed out', 'c-warn'], cancelled: ['⊘', 'cancelled', 'c-muted'] };
export const VERDICT = { passed: ['✓', 'passed', 'c-ok'], failed: ['✕', 'failed', 'c-danger'], inconclusive: ['?', 'inconclusive', 'c-muted'] };
export const ACTION = { waiting: ['○', 'not started', 'c-muted'], running: ['●', 'running', 'c-run'], started: ['●', 'running', 'c-run'],
  succeeded: ['✓', 'succeeded', 'c-ok'], failed: ['✕', 'failed', 'c-danger'], skipped: ['–', 'skipped', 'c-muted'],
  awaiting_input: ['◷', 'waiting for input', 'c-warn'], awaiting_approval: ['◷', 'waiting for approval', 'c-warn'],
  cancelled: ['⊘', 'cancelled', 'c-muted'], timed_out: ['⧗', 'timed out', 'c-warn'] };
export const OUTCOME = { passed: ['✓', 'passed', 'c-ok'], failed: ['✕', 'failed', 'c-danger'], inconclusive: ['?', 'inconclusive', 'c-muted'],
  not_applicable: ['–', 'not applicable', 'c-muted'], none: ['○', 'not judged yet', 'c-muted'] };
export const DISPOSITION = {
  none: ['not conditional', 'The step has no condition; its accepted publications are read by dependents.'],
  pending: ['pending', 'Ran before its condition resolved; contribution is withheld until the condition settles.'],
  admitted: ['admitted', 'Its condition held; its contribution is read by dependents.'],
  discarded: ['discarded', 'Its condition was false; unfinished work was cancelled and its contribution is excluded from ordinary consumers.'],
};

export const instKey = (i) => `${i.item ?? '-'}/${i.iter ?? '-'}/${i.att}`;
export const instLabel = (i) => [i.item != null && `item ${i.item}`, i.iter != null && `iteration ${i.iter}`, `attempt ${i.att}`].filter(Boolean).join(' · ');
export const bytes = (n) => (n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`);
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
const p2 = (n) => String(n).padStart(2, '0');
export const clock = (ms) => { const d = new Date(ms); return `${p2(d.getUTCHours())}:${p2(d.getUTCMinutes())}:${p2(d.getUTCSeconds())}`; };
export const day = (ms) => { const d = new Date(ms); return `${MONTHS[d.getUTCMonth()]} ${d.getUTCDate()}`; };
export const when = (ms) => `${day(ms)} ${clock(ms)}`;
export function sizeOf(o) {
  if (o.kind === 'directory') return o.content.files.reduce((n, f) => n + f.size_bytes, 0);
  return new TextEncoder().encode(typeof o.content === 'string' ? o.content : JSON.stringify(o.content)).length;
}
