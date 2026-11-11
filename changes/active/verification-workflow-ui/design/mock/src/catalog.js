// Mock data only: every Verifier definition, execution, review set and Repository Card this preview lists. Names,
// ids, timestamps and check spellings are samples; nothing here is a wire contract.
import { PR_REVIEW, I, O, T, NOW, UPDATES, runningExecution, completedExecution, finishRunning } from './data.js';

/* ---------- Definitions (exact name@version) ---------- */

const PR_INPUTS = PR_REVIEW.inputs;

const PR_REVIEW_120 = {
  name: 'pr-review', version: '1.2.0',
  description: 'A pull request is reviewed unit by unit for correctness, and a written report explains the verdict.',
  inputs: PR_INPUTS,
  steps: {
    extract: PR_REVIEW.steps.extract,
    review: { kind: 'agent', ref: 'correctness-reviewer@1.3.0', deps: ['extract'], fanout: 'steps.extract.output.units', reqs: [],
      desc: 'Review each unit for logic errors and missed edge cases.' },
    summarize: { kind: 'agent', ref: 'report-writer@1.0.1', deps: ['review'], reqs: [], desc: 'Write the review report from the findings.' },
  },
  reqs: {
    'non-empty': PR_REVIEW.reqs['non-empty'],
    'no-confirmed-defects': { kind: 'code', reads: 'review', desc: 'No correctness defect', check: 'python no_findings(severity="defect")' },
    'report-explains': PR_REVIEW.reqs['report-explains'],
  },
  after: ['no-confirmed-defects', 'report-explains'],
  deps: [['change', 'extract'], ['pull_request', 'extract'], ['extract', 'review', 'units'], ['review', 'summarize', 'findings'],
    ['review', 'no-confirmed-defects'], ['summarize', 'report-explains']],
};

const POLICY = {
  name: 'policy-review', version: '1.0.0',
  description: 'A support answer follows its refund policy and was looked up, not guessed.',
  inputs: {
    policy: { kind: 'media', sub: 'pdf · up to 64 KB' },
    answer: { kind: 'json', sub: 'text, refund' },
    trace: { kind: 'trace', sub: 'optional · 3 trace checks' },
  },
  steps: {
    'policy-text': { kind: 'extract', ref: 'from policy', deps: ['policy'], reqs: ['policy-has-text'], desc: 'Extract the text of the policy document.' },
    'refund-rules': { kind: 'agent', ref: 'refund-reader@1.0.0', deps: ['policy-text'], reqs: ['limit-found'], desc: 'Read the refund limit and conditions out of the policy text.' },
    'answer-facts': { kind: 'code', ref: 'python facts()', deps: ['refund-rules', 'answer'], reqs: [], desc: 'Compare the refund the answer offers with the policy rules.' },
  },
  reqs: {
    'policy-has-text': { kind: 'assert', step: 'policy-text', desc: 'The policy has extractable text', check: 'assert steps.policy-text.output.text is not empty' },
    'limit-found': { kind: 'assert', step: 'refund-rules', desc: 'The policy states a refund limit', check: 'assert steps.refund-rules.output.limit_usd exists' },
    'refund-within-limit': { kind: 'assert', reads: 'answer-facts', desc: 'The refund offered is within the policy limit', check: 'assert steps.answer-facts.output.within_limit equals true' },
    'policy-looked-up': { kind: 'trace', reads: 'trace', desc: 'The answer was produced after a lookup-policy span', check: 'trace span lookup-policy exists' },
    'answer-drafted-after-lookup': { kind: 'trace', reads: 'trace', desc: 'The answer was drafted after the policy lookup', check: 'trace span draft-answer starts after lookup-policy ends' },
    'lookup-read-refunds': { kind: 'trace', reads: 'trace', desc: 'The policy lookup read the refunds policy', check: 'trace span lookup-policy attribute policy.name equals "refunds"' },
    'follows-policy': { kind: 'agent', reads: 'refund-rules', desc: 'The answer follows the refund policy', check: 'agent policy-judge@1.0.0' },
    'names-the-policy': { kind: 'code', reads: 'answer', desc: 'The answer names the refund policy', check: 'python names_policy(inputs.answer.text)' },
  },
  after: ['refund-within-limit', 'policy-looked-up', 'answer-drafted-after-lookup', 'lookup-read-refunds', 'follows-policy', 'names-the-policy'],
  deps: [['policy', 'policy-text'], ['policy-text', 'refund-rules', 'outline'], ['refund-rules', 'answer-facts', 'rules'], ['answer', 'answer-facts'],
    ['answer-facts', 'refund-within-limit'], ['trace', 'policy-looked-up'], ['trace', 'answer-drafted-after-lookup'], ['trace', 'lookup-read-refunds'],
    ['refund-rules', 'follows-policy'], ['answer', 'names-the-policy']],
};

const DOCUMENT_FORMATS = {
  name: 'document-formats', version: '1.0.0', description: 'A support answer renders as safe Markdown.',
  inputs: { answer: { kind: 'json', sub: 'text' } },
  steps: { render: { kind: 'code', ref: 'python render_markdown()', deps: ['answer'], reqs: ['renders'], desc: 'Render the answer text as Markdown.' } },
  reqs: {
    renders: { kind: 'assert', step: 'render', desc: 'The answer renders without errors', check: 'assert steps.render.output.errors is empty' },
    'no-raw-html': { kind: 'code', reads: 'render', desc: 'The rendered answer contains no raw HTML', check: 'python no_raw_html()' },
  },
  after: ['no-raw-html'], deps: [['answer', 'render'], ['render', 'no-raw-html']],
};

const TOOL_REVIEW = {
  name: 'tool-review', version: '1.0.0', description: 'Every tool call in the trace names a known tool with schema-valid arguments.',
  inputs: { trace: { kind: 'trace', sub: 'tool spans' } },
  steps: { 'tool-calls': { kind: 'extract', ref: 'from trace', deps: ['trace'], reqs: [], desc: 'Collect the tool-call spans and their arguments.' } },
  reqs: {
    'tool-args-valid': { kind: 'code', reads: 'tool-calls', desc: 'Every tool call has schema-valid arguments', check: 'python args_match_schema()' },
    'no-unknown-tools': { kind: 'assert', reads: 'tool-calls', desc: 'Every tool call names a registered tool', check: 'assert steps.tool-calls.output.unknown is empty' },
  },
  after: ['tool-args-valid', 'no-unknown-tools'], deps: [['trace', 'tool-calls'], ['tool-calls', 'tool-args-valid'], ['tool-calls', 'no-unknown-tools']],
};

const ANSWER_CHECK = {
  name: 'answer-check', version: '1.0.0', description: 'An assistant answer is non-empty and cites a source.',
  inputs: { answer: { kind: 'json', sub: 'text, sources' } },
  steps: {},
  reqs: {
    'non-empty-answer': { kind: 'assert', reads: 'answer', desc: 'The answer has text', check: 'assert inputs.answer.text is not empty' },
    'cites-source': { kind: 'code', reads: 'answer', desc: 'The answer cites at least one source', check: 'python cites_source(inputs.answer)' },
  },
  after: ['non-empty-answer', 'cites-source'], deps: [['answer', 'non-empty-answer'], ['answer', 'cites-source']],
};

const ANSWER_JUDGED = {
  name: 'answer-judged', version: '1.0.0', description: 'An assistant answer agrees with the reference answer.',
  inputs: { answer: { kind: 'json', sub: 'text' }, reference: { kind: 'json', sub: 'reference answer' } },
  steps: { judge: { kind: 'agent', ref: 'answer-judge@1.1.0', deps: ['answer', 'reference'], reqs: [], desc: 'Compare the answer with the reference answer.' } },
  reqs: { agrees: { kind: 'assert', reads: 'judge', desc: 'The judge finds the answer agrees with the reference', check: 'assert steps.judge.output.agrees equals true' } },
  after: ['agrees'], deps: [['answer', 'judge'], ['reference', 'judge'], ['judge', 'agrees']],
};

const DEPENDENCY_AUDIT = {
  name: 'dependency-audit', version: '0.4.0', description: 'A pull request introduces no dependency with a known vulnerability.',
  inputs: { lockfile: { kind: 'file', sub: 'lockfile at head' } },
  steps: { audit: { kind: 'code', ref: 'python audit()', deps: ['lockfile'], reqs: [], desc: 'Resolve the lockfile against the advisory snapshot.' } },
  reqs: { 'no-known-vulns': { kind: 'assert', reads: 'audit', desc: 'No dependency has a known vulnerability', check: 'assert steps.audit.output.vulnerable is empty' } },
  after: ['no-known-vulns'], deps: [['lockfile', 'audit'], ['audit', 'no-known-vulns']],
};

const SESSION = {
  name: 'session-resolution', version: '1.0.0',
  description: 'A support session ends with the customer’s question resolved, without the customer repeating themselves.',
  // The pinned member executions are this Verifier's input; each execution draws them as member nodes.
  inputs: { members: { kind: 'executions', sub: 'member executions, pinned when judged', members: true } },
  steps: {},
  reqs: {
    'ended-resolved': { kind: 'agent', reads: 'members', desc: 'The session ended with the customer’s refund question resolved', check: 'agent resolution-judge@1.0.0' },
    'no-repeated-question': { kind: 'agent', reads: 'members', desc: 'The customer never had to ask the same question twice', check: 'agent resolution-judge@1.0.0' },
  },
  after: ['ended-resolved', 'no-repeated-question'], deps: [['members', 'ended-resolved'], ['members', 'no-repeated-question']],
};

const DRIFT = {
  name: 'latency-drift', version: '1.0.0', description: 'Request latency is distributed like the healthy baseline, judged once per daily window.',
  inputs: { latency: { kind: 'samples', sub: 'latency_ms, ttft_ms, output_tokens' } },
  steps: {},
  reqs: {
    'latency-stable': { kind: 'drift', reads: 'latency', desc: 'Request latency is distributed like the healthy baseline',
      check: 'drift psi(feature) <= 0.25 against latency-baseline@1.0.0', baseline: 'latency-baseline@1.0.0', method: 'PSI over 10 quantile bins of the baseline' },
  },
  after: ['latency-stable'], deps: [['latency', 'latency-stable']], windowed: true,
};

const MODEL_RELEASE = {
  name: 'model-release', version: '1.0.0',
  description: 'A candidate model is released only after its offline metrics hold, it beats the baseline on held-out evaluation, its bias audit is clean, and a release owner approves.',
  inputs: {
    candidate: { kind: 'model', sub: 'weights and model card' },
    eval_report: { kind: 'json', sub: 'held-out evaluation · arrives later' },
  },
  steps: {
    'offline-metrics': { kind: 'code', ref: 'python score_offline()', deps: ['candidate'], reqs: ['metrics-hold'], desc: 'Score the candidate on the offline suite.' },
    'bias-audit': { kind: 'agent', ref: 'fairness-auditor@1.0.0', deps: ['candidate'], reqs: [], desc: 'Audit predictions for disparity across customer segments.' },
    'compare-baseline': { kind: 'code', ref: 'python compare()', deps: ['eval_report', 'offline-metrics'], reqs: [], desc: 'Compare the held-out evaluation with the current production model.' },
    'release-approval': { kind: 'approval', ref: 'release owner', deps: ['offline-metrics'], reqs: [], desc: 'A release owner approves or rejects promotion of this exact candidate.' },
  },
  reqs: {
    'metrics-hold': { kind: 'assert', step: 'offline-metrics', desc: 'Offline F1 is at least 0.90', check: 'assert steps.offline-metrics.output.f1 >= 0.90' },
    'beats-baseline': { kind: 'code', reads: 'compare-baseline', desc: 'The candidate beats the production baseline', check: 'python beats_baseline(margin=0.01)' },
    'audit-clean': { kind: 'agent', reads: 'bias-audit', desc: 'The bias audit found no material disparity', check: 'agent audit-judge@1.0.0' },
    'release-approved': { kind: 'assert', reads: 'release-approval', desc: 'A release owner approved this candidate', check: 'assert steps.release-approval.output.decision equals "approved"' },
  },
  after: ['beats-baseline', 'audit-clean', 'release-approved'],
  deps: [['candidate', 'offline-metrics'], ['candidate', 'bias-audit'], ['eval_report', 'compare-baseline'], ['offline-metrics', 'compare-baseline', 'metrics'],
    ['offline-metrics', 'release-approval', 'metrics'], ['compare-baseline', 'beats-baseline'], ['bias-audit', 'audit-clean'], ['release-approval', 'release-approved']],
};

const SERVICE_DESK = [{ kind: 'Service', target: 'policy-desk@1.0.0', when: 'every call', state: 'active' }];
const repoHref = (card) => `#/repositories/${card}`;
const RUNS_ON = {
  'pr-review@1.3.0': [
    { kind: 'Repository', target: 'payments-api@1.4.0', when: 'pull_request', state: 'active', href: repoHref('payments-api') },
    { kind: 'Repository', target: 'ledger@1.1.0', when: 'pull_request', state: 'inactive · no GitHub access', href: repoHref('ledger') },
  ],
  'pr-review@1.2.0': [
    { kind: 'Repository', target: 'ledger@1.0.0', when: 'pull_request', state: 'declared by a non-selected version', href: `${repoHref('ledger')}?v=1.0.0` },
    { kind: 'Repository', target: 'docs-site@0.2.0', when: 'pull_request', state: 'declared by a non-selected version', href: `${repoHref('docs-site')}?v=0.2.0` },
  ],
  'dependency-audit@0.4.0': [{ kind: 'Repository', target: 'payments-api@1.4.0', when: 'pull_request', state: 'active', href: repoHref('payments-api') }],
  'policy-review@1.0.0': SERVICE_DESK,
  'document-formats@1.0.0': SERVICE_DESK,
  'tool-review@1.0.0': SERVICE_DESK,
  'session-resolution@1.0.0': [{ kind: 'Service', target: 'policy-desk@1.0.0', when: 'when a session closes', state: 'active' }],
  'answer-check@1.0.0': [{ kind: 'Service', target: 'assistant@2.0.0', when: 'every call', state: 'active' }],
  'answer-judged@1.0.0': [{ kind: 'Service', target: 'assistant@2.0.0', when: '25% of calls', state: 'active' }],
  'latency-drift@1.0.0': [{ kind: 'Service', target: 'assistant@2.0.0', when: 'daily 00:00 UTC', state: 'active' }],
  'model-release@1.0.0': [{ kind: 'Model', target: 'refund-classifier@2.3.0', when: 'on registration', state: 'active' }],
};

export const DEFS = Object.fromEntries([PR_REVIEW, PR_REVIEW_120, POLICY, DOCUMENT_FORMATS, TOOL_REVIEW, ANSWER_CHECK, ANSWER_JUDGED, DEPENDENCY_AUDIT, SESSION, DRIFT, MODEL_RELEASE]
  .map((d) => { const key = `${d.name}@${d.version}`; return [key, { ...d, key, runsOn: RUNS_ON[key] ?? [] }]; }));
export const versionsOf = (name) => Object.values(DEFS).filter((d) => d.name === name).map((d) => d.version).sort().reverse();

/** Read-only YAML projection of a definition, for the Verifier page. Illustrative field spellings. */
export function toYaml(d) {
  const L = ['apiVersion: wyrd/v1', 'kind: Verifier', 'metadata:', `  name: ${d.name}`, `  version: ${d.version}`, `  description: ${JSON.stringify(d.description)}`, 'spec:', '  inputs:'];
  for (const [id, i] of Object.entries(d.inputs)) L.push(`    - name: ${id}`, `      kind: ${i.kind}`);
  if (Object.keys(d.steps).length) L.push('  steps:');
  for (const [id, s] of Object.entries(d.steps)) {
    L.push(`    - id: ${id}`, `      kind: ${s.kind}`, `      uses: ${JSON.stringify(s.ref)}`, `      needs: [${s.deps.join(', ')}]`);
    if (s.when) L.push(`      when: ${s.when}`);
    if (s.fanout) L.push(`      for_each: ${s.fanout}`);
    if (s.repeat) L.push('      repeat:', `        until: ${JSON.stringify(s.repeat.until)}`, `        max: ${s.repeat.max}`);
    if (s.reqs.length) L.push('      requirements:', ...s.reqs.map((r) => `        - { id: ${r}, check: ${JSON.stringify(d.reqs[r].check)} }`));
  }
  L.push('  requirements:');
  for (const r of d.after) L.push(`    - id: ${r}`, `      reads: ${d.reqs[r].reads}`, `      check: ${JSON.stringify(d.reqs[r].check)}`);
  return L.join('\n');
}

/* ---------- Repository Cards and the GitHub connection ---------- */

export const CONN = { host: 'github.com', app: 'wyrd-verify', account: 'acme', installation: '41822907' };

// One registered Card per stable provider repository; a null card means GitHub access without a Card.
export const REPOS = [
  { card: 'payments-api', full: 'acme/payments-api', id: '718204331', access: true, selected: '1.4.0',
    desc: 'Refund and payout API. Pull requests run the team-authored pr-review workflow and a dependency audit.',
    versions: {
      '1.4.0': { uid: '01a3c2d9…9e41', registered: '2026-11-09 11:20', verifiers: [['pr-review', '1.3.0'], ['dependency-audit', '0.4.0']] },
      '1.3.0': { uid: '01a3a7f0…41c8', registered: '2026-10-30 09:12', verifiers: [['pr-review', '1.3.0']] },
      '1.2.0': { uid: '01a38e11…07b2', registered: '2026-10-14 11:47', verifiers: [['pr-review', '1.2.0']] },
    } },
  { card: 'ledger', full: 'acme/ledger', id: '702551908', access: false, selected: '1.1.0',
    desc: 'Double-entry ledger writer. Access to this repository was removed on GitHub; its Card and history stay readable.',
    versions: {
      '1.1.0': { uid: '01a39b42…c3d0', registered: '2026-10-21 15:03', verifiers: [['pr-review', '1.3.0']] },
      '1.0.0': { uid: '01a3611e…a8f7', registered: '2026-09-29 10:30', verifiers: [['pr-review', '1.2.0']] },
    } },
  { card: 'docs-site', full: 'acme/docs-site', id: '733019442', access: true, selected: '0.3.0',
    desc: 'Public documentation site. The selected policy declares verifier = [], so no review starts automatically.',
    versions: {
      '0.3.0': { uid: '01a3b5e8…6f12', registered: '2026-11-02 13:55', verifiers: [] },
      '0.2.0': { uid: '01a37c90…d41e', registered: '2026-10-09 08:40', verifiers: [['pr-review', '1.2.0']] },
    } },
  { card: null, full: 'acme/web-checkout', id: '741660215', access: true },
];

/** Read-only declaration projections of a Repository Card version. Illustrative field spellings. */
export function cardYaml(r, v) {
  const vs = r.versions[v].verifiers;
  return [
    'apiVersion: wyrd/v1', 'kind: Repository', 'metadata:', `  name: ${r.card}`, `  version: ${v}`, 'spec:',
    '  provider: github', `  host: ${CONN.host}`, `  repository_id: "${r.id}"`,
    vs.length ? '  verified_by:' : '  verified_by: []',
    ...vs.flatMap(([n, ver]) => [`    - verifier: { kind: Verifier, name: ${n}, version: ${ver} }`, '      runs_on: { kind: pull_request }']),
  ].join('\n');
}
function toml(verifiers) {
  return [
    '# wyrd.toml at the repository root. provider, host and repository_id',
    '# are inferred from origin and verified by the server.',
    '[repository.verified_by]',
    verifiers.length ? 'verifier = [' : 'verifier = []',
    ...(verifiers.length ? verifiers.map((x) => `  ${x},`).concat(']') : []),
    'runs_on = { kind = "pull_request" }',
  ].join('\n');
}
export const tomlFor = (r, v) => toml(r.versions[v].verifiers.map(([n, ver]) => `{ kind = "Verifier", name = "${n}", version = "${ver}" }`));
export const UNDECLARED_TOML = toml(['"./review/pr-review.yaml"']);

/**
 * Review sets: server-recorded admission context. One review occurrence (a PR opened, a new head pushed, a check
 * rerequested) under one exact selected Repository version requests one execution per bound Verifier. Membership is
 * recorded here, never inferred from time or head.
 */
export const REVIEW_SETS = [
  { id: 'rs-483-1', card: 'payments-api', policy: '1.4.0', pr: 483, title: 'Add refund reason codes', head: '0d7c1aa', occurrence: 'opened', at: T(14, 40, 18),
    members: ['exe-a41b7703', 'exe-a41b7811'] },
  { id: 'rs-482-3', card: 'payments-api', policy: '1.4.0', pr: 482, title: 'Cache refund eligibility per merchant', head: '9c1e4b2', occurrence: 'rerequested', at: T(14, 2, 5),
    note: 'Rerequested on GitHub by m.ito after rs-482-2 errored. Same head, new review set.', members: ['exe-7f3a91c2', 'exe-2b6e90d7'] },
  { id: 'rs-482-2', card: 'payments-api', policy: '1.4.0', pr: 482, title: 'Cache refund eligibility per merchant', head: '9c1e4b2', occurrence: 'new head', at: T(13, 31, 40),
    members: ['exe-5c7e1a20', 'exe-5c7e1b44'] },
  { id: 'rs-482-1', card: 'payments-api', policy: '1.4.0', pr: 482, title: 'Cache refund eligibility per merchant', head: '4be09a1', occurrence: 'opened', at: T(12, 47, 28),
    members: ['exe-1e44c0aa', 'exe-1e44d1b3'] },
  { id: 'rs-479-1', card: 'payments-api', policy: '1.4.0', pr: 479, title: 'Round refund amounts to minor units', head: '31ab07f', occurrence: 'opened', at: T(13, 41, 2),
    members: ['exe-5d20c8e1', 'exe-5d20c9f2'] },
  { id: 'rs-476-1', card: 'payments-api', policy: '1.3.0', pr: 476, title: 'Batch refund webhooks', head: 'b82e0d3', occurrence: 'opened', at: T(9, 15, 40),
    members: ['exe-0e6f41b9'] },
  { id: 'rs-217-1', card: 'ledger', policy: '1.1.0', pr: 217, title: 'Split ledger writer', head: '6f0a9de', occurrence: 'opened', at: T(16, 20, 1, 7),
    members: ['exe-3c9e5f10'] },
  { id: 'rs-215-1', card: 'ledger', policy: '1.0.0', pr: 215, title: 'Post ledger adjustments idempotently', head: 'e14c5b8', occurrence: 'opened', at: T(11, 2, 48, 5),
    members: ['exe-9b77d2a4'] },
  { id: 'rs-88-1', card: 'docs-site', policy: '0.2.0', pr: 88, title: 'Move API reference to generated pages', head: 'a90b1c4', occurrence: 'opened', at: T(10, 31, 15, 2),
    members: ['exe-6a0d3f52'] },
  // Arrives through the simulated live feed: a new head on PR 482.
  { id: 'rs-482-4', card: 'payments-api', policy: '1.4.0', pr: 482, title: 'Cache refund eligibility per merchant', head: 'e07b3d5', occurrence: 'new head', at: T(14, 59, 55),
    members: ['exe-e07b3d51', 'exe-e07b3d62'], hidden: true },
];

/* ---------- Executions ---------- */

const DESK = { card: 'policy-desk', service: 'policy-desk', repo: null, pr: null };
const ASSIST = { card: 'assistant-agent', service: 'assistant', repo: null, pr: null, session: null };
const R = (id, key, at, status, verdict, ctx, extra = {}) => ({ id, def: key, at, status, verdict, ...ctx, ...extra });

/** Expands a plain record into recorded step and requirement state from its definition. */
function derive(r) {
  const d = DEFS[r.def];
  const order = Object.keys(d.steps);
  const stop = r.stopped ? order.indexOf(r.stopped) : r.status === 'pending' || r.status === 'cancelled' ? 0 : order.length;
  const steps = {};
  order.forEach((id, k) => {
    const s = d.steps[id];
    const one = (state, reason) => [I(s.fanout ? 0 : null, s.repeat ? 0 : null, 1, state, reason)];
    if (k < stop) steps[id] = { action: 'succeeded', disposition: s.when ? 'admitted' : 'none', inst: one('succeeded', null) };
    else if (k === stop && (r.status === 'errored' || r.status === 'timed_out')) {
      const state = r.status === 'errored' ? 'failed' : 'timed_out';
      steps[id] = { action: state, disposition: s.when ? 'pending' : 'none', inst: one(state, r.error) };
    } else steps[id] = { action: r.status === 'cancelled' ? 'cancelled' : 'waiting', disposition: 'none', inst: [] };
  });
  const reqs = {};
  for (const [id, q] of Object.entries(d.reqs)) {
    if (q.step ? steps[q.step].action !== 'succeeded' : r.status !== 'completed') continue;
    const judge = q.kind === 'agent' ? q.check.replace('agent ', '') : undefined;
    if (r.failed?.includes(id)) reqs[id] = { outcome: 'failed', reason: r.reasons?.[id], actual: r.actual?.[id] ?? 'false', judge };
    else if (r.verdict === 'inconclusive') reqs[id] = { outcome: 'inconclusive', reason: r.why, actual: 'no evidence', judge };
    else reqs[id] = { outcome: 'passed', reason: r.reasons?.[id] ?? `${q.check} held.`, actual: r.actual?.[id] ?? 'as expected', judge };
  }
  const { failed, reasons, actual, why, ...rest } = r;
  return { ...rest, steps, reqs };
}

const POLICY_TEXT = `Refunds policy (v4)

1. Agents may refund up to $50 per order without a manager.
2. Refunds above $50 need a manager's approval.
3. Orders older than 30 days are not refundable.`;
const RULES = { limit_usd: 50, above_limit: 'manager approval', max_age_days: 30 };
const SPANS = [
  { id: 'sp-1', name: 'agent.run', start: 0, dur: 2410, depth: 0, attrs: { 'service.name': 'policy-desk' } },
  { id: 'sp-2', name: 'lookup-policy', start: 120, dur: 310, depth: 1, attrs: { tool: 'policy_lookup', 'policy.name': 'refunds' } },
  { id: 'sp-3', name: 'draft-answer', start: 480, dur: 1860, depth: 1, attrs: { 'gen_ai.request.model': 'assistant-llm' } },
];

/** A policy-review execution with its recorded publications, trace spans and requirement judgments. */
function policyExec(r, { refund, text, trace, follows }) {
  const fail = (id) => r.failed?.includes(id);
  const x = derive({ ...r, def: 'policy-review@1.0.0' });
  x.inputs = { answer: { kind: 'json', value: { text, refund } }, policy: { kind: 'media', value: 'refund-policy.pdf · 41 KB' }, trace: { kind: 'trace', value: trace } };
  x.spans = SPANS;
  if (r.status !== 'completed') return x;
  x.steps['policy-text'].inst[0].outs = [O('policy.txt', 'file', POLICY_TEXT)];
  x.steps['refund-rules'].inst[0].outs = [O('rules.json', 'json', RULES)];
  x.steps['answer-facts'].inst[0].outs = [O('facts.json', 'json', { refund_usd: refund, limit_usd: 50, within_limit: refund <= 50 })];
  const judged = (id, outcome, reason, actual, extra = {}) => { x.reqs[id] = { outcome, reason, actual, ...extra }; };
  judged('policy-has-text', 'passed', 'steps.policy-text.output.text has 189 characters', '189 characters');
  judged('limit-found', 'passed', 'steps.refund-rules.output.limit_usd is 50', '50');
  judged('refund-within-limit', fail('refund-within-limit') ? 'failed' : 'passed',
    `steps.answer-facts.output.within_limit is ${refund <= 50}${refund <= 50 ? '' : '; expected equals true'}`, String(refund <= 50), { cites: [['answer-facts', '-/-/1', 'facts.json']] });
  judged('policy-looked-up', 'passed', `Span lookup-policy (sp-2) exists in trace ${trace}.`, 'sp-2', { cites: [{ span: 'sp-2' }] });
  judged('answer-drafted-after-lookup', 'passed', 'draft-answer starts at +480 ms, after lookup-policy ends at +430 ms.', '+50 ms', { cites: [{ span: 'sp-2' }, { span: 'sp-3' }] });
  judged('lookup-read-refunds', 'passed', 'lookup-policy has policy.name = "refunds".', '"refunds"', { cites: [{ span: 'sp-2' }] });
  judged('follows-policy', fail('follows-policy') ? 'failed' : 'passed', follows ?? '“The answer offers a refund within the $50 limit and names the refunds policy.”',
    fail('follows-policy') ? 'failed' : 'passed', { judge: 'policy-judge@1.0.0', cites: [['refund-rules', '-/-/1', 'rules.json']] });
  judged('names-the-policy', 'passed', 'The answer text mentions “refunds policy”.', 'true');
  return x;
}

const SESSION_DESK = { ...DESK, run: null };
const answerCite = (exe) => ({ exe, input: 'answer', pointer: '/text' });

function sessionExec(r, members, reqs) { return { ...r, def: 'session-resolution@1.0.0', steps: {}, members, reqs }; }

// Fourteen daily windows, oldest first. Scores and per-feature verdicts are what the sample server recorded.
const DRIFT_SCORES = {
  latency_ms: [0.04, 0.06, 0.05, 0.08, 0.07, 0.11, 0.09, 0.27, 0.12, 0.10, 0.14, 0.18, 0.22, 0.31],
  ttft_ms: [0.03, 0.04, 0.05, 0.04, 0.06, 0.05, 0.07, 0.12, 0.08, 0.07, 0.09, 0.11, 0.13, 0.17],
  output_tokens: [0.02, 0.02, 0.03, 0.02, 0.03, 0.04, 0.03, 0.05, 0.04, 0.03, 0.04, 0.05, 0.04, 0.06],
};
const THRESHOLD = 0.25;
const DRIFT_DAYS = [[9, 27], [9, 28], [9, 29], [9, 30], [9, 31], [10, 1], [10, 2], [10, 3], [10, 4], [10, 5], [10, 6], [10, 7], [10, 8], [10, 9]];
function driftExec(k) {
  const [month, d] = DRIFT_DAYS[k];
  const end = T(0, 0, 0, d, month);
  const features = Object.entries(DRIFT_SCORES).map(([name, s]) => ({ name, score: s[k], threshold: THRESHOLD, verdict: s[k] > THRESHOLD ? 'drift' : 'stable' }));
  const drifted = features.filter((f) => f.verdict === 'drift');
  const top = features.reduce((a, b) => (b.score > a.score ? b : a));
  return {
    id: `exe-d1f7a0${(k + 1).toString(16).padStart(2, '0')}`, def: 'latency-drift@1.0.0', at: end + 4000, status: 'completed', verdict: drifted.length ? 'failed' : 'passed',
    card: 'latency-model', service: 'assistant', run: null, session: null, repo: null, pr: null, window: [end - 86400000, end], steps: {},
    reqs: { 'latency-stable': {
      outcome: drifted.length ? 'failed' : 'passed',
      reason: drifted.length ? drifted.map((f) => `${f.name} drifted: PSI ${f.score.toFixed(2)} is above ${THRESHOLD}`).join('; ')
        : `No feature drifted: the highest PSI is ${top.score.toFixed(2)} (${top.name}), at or below ${THRESHOLD}.`,
      actual: `PSI ${top.score.toFixed(2)} (${top.name})`, drift: { features, samples: 40800 + k * 137 },
    } },
  };
}

const AUDIT_NOTES = `# Bias audit (in progress)

Scored 9,200 of 18,400 held-out predictions across four customer segments.

- Approval-rate gap so far: 1.8 points (limit 3.0).
- Remaining: segments *new account* and *high value*.`;

function releaseExec() {
  return {
    id: 'exe-4d1e9a07', def: 'model-release@1.0.0', at: T(14, 31, 15), status: 'running', verdict: null,
    card: 'refund-classifier', service: null, run: null, session: null, repo: null, pr: null,
    inputs: {
      candidate: { kind: 'model', value: 'refund-classifier@2.3.0 · weights sha256 9e1c…44af' },
      eval_report: { kind: 'json', value: null, awaited: 'Not provided yet. The execution started without it; compare-baseline starts when it arrives.' },
    },
    steps: {
      'offline-metrics': { action: 'succeeded', disposition: 'none', inst: [I(null, null, 1, 'succeeded', null, [O('metrics.json', 'json', { f1: 0.912, precision: 0.934, recall: 0.891, rows: 18400 })])] },
      'bias-audit': { action: 'running', disposition: 'none', inst: [I(null, null, 1, 'started', null, [O('notes.md', 'file', AUDIT_NOTES)])] },
      'compare-baseline': { action: 'awaiting_input', disposition: 'none', inst: [], waitingFor: ['eval_report'] },
      'release-approval': { action: 'awaiting_approval', disposition: 'none', inst: [I(null, null, 1, 'awaiting_approval', null)],
        approval: { occurrence: 'apo-3f81c0d2', state: 'pending', requested: T(14, 33, 2), expires: T(15, 33, 2),
          message: 'Promote refund-classifier 2.3.0 to production traffic? Offline F1 is 0.912 (needs 0.90). The bias audit and baseline comparison have not finished.' } },
    },
    reqs: { 'metrics-hold': { outcome: 'passed', reason: 'steps.offline-metrics.output.f1 is 0.912; expected at least 0.90', actual: '0.912' } },
  };
}

/* ---------- Deterministic background population ---------- */

// A seeded generator, so every load shows the same records spread across the whole 24-hour range.
function rng(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Ordinary traffic: policy-desk runs (three Verifiers each, some in judged sessions) and assistant calls. */
function background(taken) {
  const r = rng(20261109);
  const hex = (n) => Array.from({ length: n }, () => Math.floor(r() * 16).toString(16)).join('');
  const uid = () => { let id; do id = `exe-${hex(8)}`; while (taken.has(id)); taken.add(id); return id; };
  const out = [];
  const pick = (xs) => xs[Math.floor(r() * xs.length)];
  // Policy desk: one Application Run every 5–14 minutes, slower overnight, up to an hour before the sample clock.
  let session = null;
  for (let t = NOW - 86400000 + 4 * 60000; t < NOW - 62 * 60000;) {
    const run = `run-${hex(6)}`;
    if (!session && r() < 0.32) session = { id: `s-${hex(4)}`, left: 2 + Math.floor(r() * 3), members: [] };
    const ctx = { ...DESK, run, session: session?.id ?? null };
    const roll = r();
    const refund = roll < 0.05 ? pick([60, 80, 120]) : pick([10, 15, 20, 25, 30, 35, 40, 45]);
    const failed = roll < 0.05 ? ['refund-within-limit', 'follows-policy'] : roll < 0.09 ? ['follows-policy'] : [];
    const text = failed.length === 1 ? 'Under the refunds policy I can’t refund this order; please contact us again later.'
      : `Under the refunds policy, you can get a refund of $${refund}.`;
    const p = policyExec(R(uid(), null, t + 2000, 'completed', failed.length ? 'failed' : 'passed', ctx, { failed }),
      { refund: failed.length === 1 ? 0 : refund, text, trace: `${hex(4)}…${hex(4)}`,
        follows: failed.length ? (failed.length === 1 ? '“The policy allows this refund without a manager; the answer refuses it.”' : `“The answer offers $${refund} without a manager; the refunds policy caps agent refunds at $50.”`) : undefined });
    out.push(p);
    const fmt = r();
    out.push(derive(R(uid(), 'document-formats@1.0.0', t + 1000, fmt < 0.012 ? 'cancelled' : 'completed', fmt < 0.012 ? null : fmt < 0.05 ? 'failed' : 'passed', ctx,
      fmt < 0.05 && fmt >= 0.012 ? { failed: ['no-raw-html'], reasons: { 'no-raw-html': 'The rendered answer contains a raw <a> element from the knowledge base.' }, actual: { 'no-raw-html': '1 raw element' } } : {})));
    const tool = r();
    out.push(derive(R(uid(), 'tool-review@1.0.0', t, tool < 0.01 ? 'errored' : 'completed', tool < 0.01 ? null : tool < 0.04 ? 'inconclusive' : 'passed', ctx,
      tool < 0.01 ? { stopped: 'tool-calls', error: 'The trace store returned 503 while reading tool spans.' }
        : tool < 0.04 ? { why: 'The trace was sampled out: no tool spans were retained, so this could not be judged.' } : {})));
    if (session) {
      session.members.push(p.id);
      if (--session.left === 0) {
        const bad = r() < 0.18;
        const last = session.members.at(-1);
        out.push(sessionExec(R(uid(), null, t + 40000, 'completed', bad ? 'failed' : 'passed', { ...SESSION_DESK, session: session.id }), session.members, {
          'ended-resolved': bad
            ? { outcome: 'failed', reason: '“The session ended without a refund decision.”', actual: 'failed', judge: 'resolution-judge@1.0.0', cites: [answerCite(last)] }
            : { outcome: 'passed', reason: '“The customer accepted the refund and closed the chat.”', actual: 'passed', judge: 'resolution-judge@1.0.0', cites: [answerCite(last)] },
          'no-repeated-question': { outcome: 'passed', reason: '“Each customer message asked something new.”', actual: 'passed', judge: 'resolution-judge@1.0.0' },
        }));
        session = null;
      }
    }
    const h = new Date(t).getUTCHours();
    t += (5 + r() * 9) * 60000 * (h < 6 ? 1.9 : 1);
  }
  // Assistant: answer-check on every sampled call, answer-judged on a quarter of them.
  for (let t = NOW - 86400000 + 9 * 60000; t < NOW - 60 * 60000; t += (14 + r() * 16) * 60000) {
    const run = `run-${hex(6)}`;
    const ok = r() > 0.06;
    out.push(derive(R(uid(), 'answer-check@1.0.0', t, 'completed', ok ? 'passed' : 'failed', { ...ASSIST, run },
      ok ? {} : { failed: ['cites-source'], reasons: { 'cites-source': 'inputs.answer.sources is empty.' }, actual: { 'cites-source': '0 sources' } })));
    if (r() < 0.25) {
      const agrees = r() > 0.2;
      out.push(derive(R(uid(), 'answer-judged@1.0.0', t + 1000, 'completed', agrees ? 'passed' : 'failed', { ...ASSIST, run },
        agrees ? {} : { failed: ['agrees'], reasons: { agrees: 'steps.judge.output.agrees is false; expected equals true.' } })));
    }
  }
  return out;
}

const PR = (card, pr, title, head, set, policy) => ({ card, service: null, run: null, session: null, repo: `acme/${card}`, pr, title, head, set, policy });

function build() {
  const list = [
    sessionExec(R('exe-c81f2a90', null, T(14, 59, 12), 'completed', 'passed', { ...SESSION_DESK, session: 's-81f2' }), ['exe-7c1e02a4', 'exe-7c1e02b1', 'exe-7c1e02c7'], {
      'ended-resolved': { outcome: 'passed', reason: '“The customer accepted the refund offer and closed the chat.”', actual: 'passed', judge: 'resolution-judge@1.0.0', cites: [answerCite('exe-7c1e02a4')] },
      'no-repeated-question': { outcome: 'passed', reason: '“The customer asked one question.”', actual: 'passed', judge: 'resolution-judge@1.0.0' },
    }),
    policyExec(R('exe-7c1e02a4', null, T(14, 58, 41), 'completed', 'failed', { ...DESK, run: 'run-7c1e02', session: 's-81f2' }, { failed: ['refund-within-limit', 'follows-policy'] }),
      { refund: 80, text: 'Under the refunds policy, you can get a refund of $80.', trace: '7f2a…c013',
        follows: '“The answer offers $80 without a manager; the refunds policy caps agent refunds at $50.”' }),
    derive(R('exe-7c1e02b1', 'document-formats@1.0.0', T(14, 58, 40), 'completed', 'passed', { ...DESK, run: 'run-7c1e02', session: 's-81f2' })),
    derive(R('exe-7c1e02c7', 'tool-review@1.0.0', T(14, 58, 39), 'completed', 'passed', { ...DESK, run: 'run-7c1e02', session: 's-81f2' })),
    derive(R('exe-7c1d77a2', 'answer-check@1.0.0', T(14, 58, 30), 'completed', 'passed', { ...ASSIST, run: 'run-7c1d77' })),
    derive(R('exe-7c1d77b8', 'answer-judged@1.0.0', T(14, 58, 31), 'completed', 'failed', { ...ASSIST, run: 'run-7c1d77' },
      { failed: ['agrees'], reasons: { agrees: 'steps.judge.output.agrees is false; expected equals true. Judge: “The answer says 3 days; the reference says 5–7 business days.”' } })),
    sessionExec(R('exe-77a0e1c3', null, T(14, 58, 2), 'completed', 'passed', { ...SESSION_DESK, session: 's-77a0' }), ['exe-7a0b11e6', 'exe-7a0b55d1'], {
      'ended-resolved': { outcome: 'passed', reason: '“The customer confirmed the $20 refund resolved the question.”', actual: 'passed', judge: 'resolution-judge@1.0.0', cites: [answerCite('exe-7a0b11e6')] },
      'no-repeated-question': { outcome: 'passed', reason: '“No question was repeated.”', actual: 'passed', judge: 'resolution-judge@1.0.0' },
    }),
    derive(R('exe-7a0b55d1', 'tool-review@1.0.0', T(14, 57, 2), 'completed', 'inconclusive', { ...DESK, run: 'run-7a0b55', session: 's-77a0' },
      { why: 'The trace was sampled out: no tool spans were retained, so this could not be judged.' })),
    policyExec(R('exe-7a0b11e6', null, T(14, 55, 40), 'completed', 'passed', { ...DESK, run: 'run-7a0b11', session: 's-77a0' }),
      { refund: 20, text: 'Under the refunds policy, I have refunded $20 to your card.', trace: '6e01…a7d2' }),
    releaseExec(),
    derive(R('exe-a41b7703', 'pr-review@1.3.0', T(14, 40, 20), 'pending', null, PR('payments-api', 483, 'Add refund reason codes', '0d7c1aa', 'rs-483-1', '1.4.0'))),
    derive(R('exe-a41b7811', 'dependency-audit@0.4.0', T(14, 40, 20), 'pending', null, PR('payments-api', 483, 'Add refund reason codes', '0d7c1aa', 'rs-483-1', '1.4.0'))),
    sessionExec(R('exe-6c19e5f0', null, T(14, 14, 50), 'completed', 'failed', { ...SESSION_DESK, session: 's-6c19' }), ['exe-78e4b16b', 'exe-7920ef38', 'exe-7951d2c4', 'exe-7983aa10'], {
      'ended-resolved': { outcome: 'failed', reason: '“The last answer refused the refund the earlier answer had offered, and the session ended without a refund decision.”', actual: 'failed',
        judge: 'resolution-judge@1.0.0', cites: [answerCite('exe-7951d2c4'), answerCite('exe-7983aa10')] },
      'no-repeated-question': { outcome: 'passed', reason: '“Each customer message asked something new.”', actual: 'passed', judge: 'resolution-judge@1.0.0' },
    }),
    policyExec(R('exe-7983aa10', null, T(14, 12, 5), 'completed', 'failed', { ...DESK, run: 'run-7983aa', session: 's-6c19' }, { failed: ['follows-policy'] }),
      { refund: 0, text: 'Under the refunds policy I can’t refund this order today; please ask again tomorrow.', trace: '7983…1f0e',
        follows: '“The policy allows this $40 refund without a manager; the answer refuses it and asks the customer to return later.”' }),
    policyExec(R('exe-7951d2c4', null, T(14, 10, 40), 'completed', 'passed', { ...DESK, run: 'run-7951d2', session: 's-6c19' }),
      { refund: 40, text: 'Under the refunds policy, you can get a refund of $40.', trace: '7951…08bc' }),
    policyExec(R('exe-7920ef38', null, T(14, 8, 16), 'completed', 'passed', { ...DESK, run: 'run-7920ef', session: 's-6c19' }),
      { refund: 25, text: 'Under the refunds policy, your $25 refund for the late delivery is on its way.', trace: '7920…3e55' }),
    policyExec(R('exe-78e4b16b', null, T(14, 6, 33), 'completed', 'passed', { ...DESK, run: 'run-78e4b1', session: 's-6c19' }),
      { refund: 30, text: 'Under the refunds policy, orders under 30 days qualify; I have started a $30 refund.', trace: '78e4…c901' }),
    { ...runningExecution(), set: 'rs-482-3', policy: '1.4.0' },
    derive(R('exe-2b6e90d7', 'dependency-audit@0.4.0', T(14, 2, 9), 'completed', 'passed', PR('payments-api', 482, 'Cache refund eligibility per merchant', '9c1e4b2', 'rs-482-3', '1.4.0'))),
    derive(R('exe-5c7e1a20', 'pr-review@1.3.0', T(13, 31, 44), 'errored', null, PR('payments-api', 482, 'Cache refund eligibility per merchant', '9c1e4b2', 'rs-482-2', '1.4.0'),
      { stopped: 'validate', error: 'The finding-validator provider returned 503 after 3 attempts.' })),
    derive(R('exe-5c7e1b44', 'dependency-audit@0.4.0', T(13, 31, 43), 'completed', 'passed', PR('payments-api', 482, 'Cache refund eligibility per merchant', '9c1e4b2', 'rs-482-2', '1.4.0'))),
    { ...completedExecution(), set: 'rs-479-1', policy: '1.4.0' },
    derive(R('exe-5d20c9f2', 'dependency-audit@0.4.0', T(13, 41, 4), 'completed', 'passed', PR('payments-api', 479, 'Round refund amounts to minor units', '31ab07f', 'rs-479-1', '1.4.0'))),
    derive(R('exe-1e44c0aa', 'pr-review@1.3.0', T(12, 47, 30), 'completed', 'failed', PR('payments-api', 482, 'Cache refund eligibility per merchant', '4be09a1', 'rs-482-1', '1.4.0'),
      { failed: ['no-confirmed-defects'], reasons: { 'no-confirmed-defects': '1 validated finding is a correctness defect: C1 refunds/cache.py:27 compares seconds with milliseconds.' },
        actual: { 'no-confirmed-defects': '1 confirmed (C1)' } })),
    derive(R('exe-1e44d1b3', 'dependency-audit@0.4.0', T(12, 47, 31), 'completed', 'failed', PR('payments-api', 482, 'Cache refund eligibility per merchant', '4be09a1', 'rs-482-1', '1.4.0'),
      { failed: ['no-known-vulns'], reasons: { 'no-known-vulns': 'requests 2.31.0 matches advisory GHSA-9wx4-h78v-vm56 at head 4be09a1.' }, actual: { 'no-known-vulns': '1 vulnerable' } })),
    derive(R('exe-0e6f41b9', 'pr-review@1.3.0', T(9, 15, 44), 'timed_out', null, PR('payments-api', 476, 'Batch refund webhooks', 'b82e0d3', 'rs-476-1', '1.3.0'),
      { stopped: 'validate', error: 'validate exceeded its 30 min limit in iteration 0.' })),
    derive(R('exe-91ab0c3e', 'answer-check@1.0.0', T(13, 20, 0), 'cancelled', null, { ...ASSIST, run: 'run-79ff10' })),
    derive(R('exe-8c02f7d1', 'answer-judged@1.0.0', T(11, 5, 12), 'errored', null, { ...ASSIST, run: 'run-7b3e90' }, { stopped: 'judge', error: 'The judge provider returned 503 after 3 attempts.' })),
    derive(R('exe-3c9e5f10', 'pr-review@1.3.0', T(16, 20, 3, 7), 'errored', null, PR('ledger', 217, 'Split ledger writer', '6f0a9de', 'rs-217-1', '1.1.0'),
      { stopped: 'extract', error: 'Checkout failed: the installation token was rejected after access to acme/ledger was removed.' })),
    derive(R('exe-9b77d2a4', 'pr-review@1.2.0', T(11, 2, 51, 5), 'completed', 'passed', PR('ledger', 215, 'Post ledger adjustments idempotently', 'e14c5b8', 'rs-215-1', '1.0.0'))),
    derive(R('exe-6a0d3f52', 'pr-review@1.2.0', T(10, 31, 17, 2), 'completed', 'passed', PR('docs-site', 88, 'Move API reference to generated pages', 'a90b1c4', 'rs-88-1', '0.2.0'))),
    ...DRIFT_DAYS.map((_, k) => driftExec(k)),
    // Arrive through the simulated live feed.
    { ...policyExec(R('exe-80a1f3c9', null, T(14, 59, 50), 'completed', 'passed', { ...DESK, run: 'run-80a1f3', session: null }),
      { refund: 35, text: 'Under the refunds policy, you can get a refund of $35.', trace: '80a1…e3f7' }), hidden: true },
    { ...derive(R('exe-e07b3d51', 'pr-review@1.3.0', T(14, 59, 57), 'pending', null, PR('payments-api', 482, 'Cache refund eligibility per merchant', 'e07b3d5', 'rs-482-4', '1.4.0'))), hidden: true },
    { ...derive(R('exe-e07b3d62', 'dependency-audit@0.4.0', T(14, 59, 57), 'pending', null, PR('payments-api', 482, 'Cache refund eligibility per merchant', 'e07b3d5', 'rs-482-4', '1.4.0'))), hidden: true },
  ];
  const taken = new Set(list.map((x) => x.id));
  return Object.fromEntries([...list, ...background(taken)].map((x) => [x.id, x]));
}

/** Fresh, mutable sample executions keyed by exact id. */
export const buildExecutions = build;

/** Simulated live feed: data only, never topology. Each event is skipped once its execution has settled. */
export const EVENTS = [
  { label: UPDATES[0].label, apply: (X) => X['exe-7f3a91c2'].status === 'running' && UPDATES[0].apply(X['exe-7f3a91c2']) },
  { label: UPDATES[1].label, apply: (X) => X['exe-7f3a91c2'].status === 'running' && UPDATES[1].apply(X['exe-7f3a91c2']) },
  { label: 'a new policy-review execution arrives', apply: (X) => { X['exe-80a1f3c9'].hidden = false; return ['exe-80a1f3c9']; } },
  { label: 'PR 482 gets a new head: review set rs-482-4', apply: (X, sets) => {
    for (const id of ['exe-e07b3d51', 'exe-e07b3d62']) X[id].hidden = false;
    sets.find((s) => s.id === 'rs-482-4').hidden = false;
    return ['exe-e07b3d51', 'exe-e07b3d62'];
  } },
];
export { finishRunning };
