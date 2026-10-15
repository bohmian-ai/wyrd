import type { DiagramNode, DiagramSpec, Row, SequenceSpec, Tone } from './types';

/** Real-time verification versus a Trigger-started run, same participants. */
export const verificationPaths: SequenceSpec = {
  type: 'sequence',
  answers: 'Who waits for a verdict, and when do Operators run?',
  lives: 'understand/how-verification-works',
  width: 860,
  participants: [
    { id: 'svc', kind: 'client', x: 110, title: 'Your service', sub: 'SDK in a run' },
    { id: 'srv', kind: 'component', x: 340, title: 'Verifier runtime', sub: 'wyrd-server' },
    { id: 'bif', kind: 'store', x: 570, title: 'Bifrost', sub: 'Scribe · Oracle' },
    { id: 'ops', kind: 'declare', x: 770, title: 'Operators', sub: 'on_failure' }
  ],
  flows: [
    {
      name: 'Real time',
      rest: 'Your code calls the Verifier and waits for the verdict, up to 60 seconds. The result is stored afterward. Operators are never called; your code decides what to do. Press Play to step through it.',
      messages: [
        {
          from: 'svc',
          to: 'srv',
          kind: 'control',
          label: 'observe.verify("no-refund-promise", input)',
          say: 'Your code calls a Verifier bound to this component, by name, with the input to judge.'
        },
        {
          from: 'srv',
          to: 'srv',
          kind: 'control',
          label: 'judge · up to 60 s',
          say: 'The server loads the exact Verifier version and judges the input inside the same request. Past 60 seconds the call fails with <code>WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT</code>.'
        },
        {
          from: 'srv',
          to: 'svc',
          kind: 'ref',
          label: 'Judgment · verdict, passed',
          say: 'The verdict comes back as an ordinary value. A <code>failed</code> verdict is not an exception; your code checks <code>passed</code> and decides.'
        },
        {
          from: 'srv',
          to: 'bif',
          kind: 'best',
          label: 'result row · binding_id null',
          mark: { at: 'ops', text: '✕ not called' },
          say: 'After replying, the server stores the result in <code>vala.verification.results</code> with your <code>run_id</code>. No Operator runs for a real-time call.'
        }
      ]
    },
    {
      name: 'Triggered',
      rest: 'Your code only records. A Trigger starts a run on the server, which reads the observation, stores a verdict, and pages Operators on failure. Shown here with <code>observations_ready</code>; a schedule starts the same run from a cron time instead. Press Play.',
      messages: [
        { from: 'svc', to: 'bif', kind: 'data', label: 'observe.eval(record)', say: 'Your service records an eval observation and moves on. It never waits for a check.' },
        {
          from: 'bif',
          to: 'srv',
          kind: 'control',
          label: 'row committed · Trigger fires',
          say: 'Once Bifrost commits the row, the <code>observations_ready</code> Trigger starts a run for each matching binding owned by the Service version that wrote it.'
        },
        {
          from: 'srv',
          to: 'srv',
          kind: 'control',
          label: 'run · pending → running',
          say: 'The run is queued, then claimed. It has 3 attempts of up to 5 minutes each. Its status is separate from its verdict.'
        },
        { from: 'srv', to: 'bif', kind: 'control', label: 'read the observation', say: 'The Verifier reads the stored observation, the same data you can query yourself.' },
        { from: 'bif', to: 'srv', kind: 'ref', label: 'record', say: 'The record comes back to the runtime to be judged.' },
        {
          from: 'srv',
          to: 'bif',
          kind: 'data',
          label: 'result row · verdict',
          say: 'The verdict is written to <code>vala.verification.results</code>, with the binding, Verifier version, and the <code>record_id</code> it judged.'
        },
        {
          from: 'srv',
          to: 'ops',
          kind: 'control',
          label: 'only if failed → dispatch',
          say: 'Only a <code>failed</code> verdict dispatches each Operator in <code>on_failure</code>. Each is delivered on its own, at least once, up to 3 attempts.'
        }
      ]
    }
  ]
};

const badge = (tone: string, glyph: string, word: string) =>
  `<span class="wd-badge${tone ? ' ' + tone : ''}">${glyph} ${word}</span>`;
const run = (status: string, result: string, ops: string, todo: string): Row[] => [
  ['get_run shows', status],
  ['result_id', result],
  ['Operators', ops],
  ['What to do', todo]
];
const verdict = (v: string, means: string, ops: string, todo: string): Row[] => [
  ['Verdict', v],
  ['Means', means],
  ['Operators', ops],
  ['What to do', todo]
];
const dispatch = (s: string, means: string): Row[] => [
  ['Dispatch status', s],
  ['Means', means],
  ['Where', 'Each entry in <code>dispatches</code> on <code>verification.get_run</code>. Delivery never changes the verdict.']
];
const state = (
  id: string,
  x: number,
  y: number,
  tone: Tone,
  glyph: string,
  detail: Row[],
  w = 120,
  h = 46,
  title = id
): DiagramNode => ({ id, kind: 'state', x, y, w, h, tone, glyph, title, detail });

/** Run execution states, as named by `VerificationExecutionStatus`. */
export const RUN_STATES = ['pending', 'running', 'retrying', 'completed', 'cancelled', 'timed_out', 'errored'];
/** Verdicts, as named by `VerificationVerdict`. */
export const VERDICTS = ['passed', 'failed', 'inconclusive'];
/** Operator dispatch states, as named by `OperatorDispatchStatus`. */
export const DISPATCH_STATES = ['pending', 'running', 'retrying', 'delivered', 'failed'];

/** A verification run: execution, then verdict, then Operator dispatch. */
export const runLifecycle: DiagramSpec = {
  type: 'diagram',
  answers: 'What does this run status mean, and what happens next?',
  lives: 'understand/how-verification-works',
  narrLabel: 'Now',
  detailHint: 'Click a state to see what you would see and what to do.',
  views: [
    {
      size: [860, 470],
      rest: 'A run first has to execute, then it has a verdict, then a failed verdict pages Operators. Click a state, or play a scenario.',
      groups: [
        { x: 16, y: 36, w: 300, h: 224, label: 'In progress' },
        { x: 376, y: 322, w: 468, h: 136, label: 'Each Operator, failed binding runs only' }
      ],
      texts: [
        { x: 508, y: 170, text: '‹ an attempt ran past 5 minutes', anchor: 'start' },
        { x: 508, y: 220, text: '‹ not retryable, or 3 attempts used', anchor: 'start' },
        { x: 508, y: 270, text: '‹ stopped before a result', anchor: 'start' }
      ],
      nodes: [
        state('pending', 36, 74, 'progress', '○', run(badge('', '○', 'pending'), 'null', 'None yet', 'Wait. The run is queued and no runner has claimed it.')),
        state('running', 176, 74, 'progress', '▶', run(badge('', '▶', 'running'), 'null', 'None yet', 'Wait. Each attempt has up to 5 minutes; a run gets 3 attempts.')),
        state('retrying', 176, 184, 'progress', '↻', run(badge('', '↻', 'retrying'), 'null', 'None yet', 'Read <code>error</code> to see why the last attempt failed. It will run again.')),
        state('completed', 380, 74, 'ok', '✓', run(badge('ok', '✓', 'completed'), 'Set', 'Depends on the verdict', 'Query <code>vala.verification.results</code> by <code>result_id</code>. The run itself never carries the verdict.')),
        state('timed_out', 380, 146, 'warn', '!', run(badge('warn', '!', 'timed_out'), 'null', 'Never', 'An attempt ran past 5 minutes, often a slow LLM judge. No verdict exists.'), 120, 40),
        state('errored', 380, 196, 'danger', '✕', run(badge('danger', '✕', 'errored'), 'null', 'Never', 'Read <code>error.code</code> and <code>error.message</code>. This is a failure to check, not a failure of your service.'), 120, 40),
        state('cancelled', 380, 246, 'muted', '–', run(badge('', '–', 'cancelled'), 'null', 'Never', 'The run stopped before producing a result. Nothing to judge.'), 120, 40),
        {
          id: 'verdict',
          kind: 'decision',
          x: 556,
          y: 70,
          w: 92,
          h: 54,
          title: 'verdict',
          detail: verdict('Read from the result row', 'The verdict lives in Bifrost, in <code>vala.verification.results.verdict</code>.', 'Only on <code>failed</code>', 'Query by <code>result_id</code>.')
        },
        state('passed', 700, 14, 'ok', '✓', verdict(badge('ok', '✓', 'passed'), 'The behavior met the standard.', 'None', 'Nothing.'), 140, 44),
        state(
          'inconclusive',
          700,
          74,
          'muted',
          '●',
          verdict(badge('', '●', 'inconclusive'), 'No judgment was possible. Never a pass.', 'None', 'Common causes: no <code>pass_gate</code>, every task skipped, sampling dropped the record, or too little drift data.'),
          140,
          44
        ),
        state(
          'failed',
          700,
          134,
          'danger',
          '✕',
          verdict(badge('danger', '✕', 'failed'), 'The behavior violated the standard.', 'Every <code>on_failure</code> Operator, for a binding run', 'Read the per-task or per-feature rows to see why. A run that names a Verifier directly pages no one.'),
          140,
          44
        ),
        state('d_pending', 392, 350, 'progress', '○', dispatch(badge('', '○', 'pending'), 'Created when the run settled as failed; waiting to be sent.'), 100, 40, 'pending'),
        state('d_running', 508, 350, 'progress', '▶', dispatch(badge('', '▶', 'running'), 'Being sent to Slack, PagerDuty, or your URL.'), 100, 40, 'running'),
        state('d_delivered', 624, 350, 'ok', '✓', dispatch(badge('ok', '✓', 'delivered'), 'The provider accepted it. Delivery is at least once, so duplicates are possible.'), 104, 40, 'delivered'),
        state('d_retrying', 508, 408, 'progress', '↻', dispatch(badge('', '↻', 'retrying'), 'A network error, 408, 429, or 5xx. It will be sent again.'), 100, 40, 'retrying'),
        state(
          'd_failed',
          740,
          408,
          'danger',
          '✕',
          dispatch(badge('danger', '✕', 'failed'), 'Gave up after 3 attempts within 5 minutes, or a terminal refusal. Read its <code>error</code>; usually the operator connection.'),
          92,
          40,
          'failed'
        )
      ],
      edges: [
        { id: 'p_r', from: 'pending', to: 'running', kind: 'control', pts: [[156, 97], [176, 97]] },
        { id: 'r_c', from: 'running', to: 'completed', kind: 'control', pts: [[296, 90], [380, 90]] },
        { id: 'r_rt', from: 'running', to: 'retrying', kind: 'control', pts: [[216, 120], [216, 184]], label: 'retryable error', lx: 210, ly: 156, anchor: 'end' },
        { id: 'rt_r', from: 'retrying', to: 'running', kind: 'control', pts: [[256, 184], [256, 120]], label: 'next attempt', lx: 262, ly: 156, anchor: 'start' },
        { id: 'r_to', from: 'running', to: 'timed_out', kind: 'control', pts: [[296, 100], [336, 100], [336, 166], [380, 166]] },
        { id: 'r_e', from: 'running', to: 'errored', kind: 'control', pts: [[296, 108], [348, 108], [348, 216], [380, 216]] },
        { id: 'r_x', from: 'running', to: 'cancelled', kind: 'control', pts: [[296, 116], [360, 116], [360, 266], [380, 266]] },
        { id: 'c_v', from: 'completed', to: 'verdict', kind: 'data', pts: [[500, 97], [556, 97]], label: 'result', lx: 528, ly: 89 },
        { id: 'v_p', from: 'verdict', to: 'passed', kind: 'data', pts: [[602, 70], [602, 36], [700, 36]] },
        { id: 'v_i', from: 'verdict', to: 'inconclusive', kind: 'data', pts: [[648, 97], [700, 97]] },
        { id: 'v_f', from: 'verdict', to: 'failed', kind: 'data', pts: [[602, 124], [602, 156], [700, 156]] },
        { id: 'f_d', from: 'failed', to: 'd_pending', kind: 'control', pts: [[770, 178], [770, 300], [442, 300], [442, 350]], label: 'dispatch each Operator', lx: 606, ly: 292 },
        { id: 'dp_dr', from: 'd_pending', to: 'd_running', kind: 'control', pts: [[492, 370], [508, 370]] },
        { id: 'dr_dd', from: 'd_running', to: 'd_delivered', kind: 'control', pts: [[608, 370], [624, 370]] },
        { id: 'dr_dt', from: 'd_running', to: 'd_retrying', kind: 'control', pts: [[548, 390], [548, 408]] },
        { id: 'dt_dr', from: 'd_retrying', to: 'd_running', kind: 'control', pts: [[578, 408], [578, 390]] },
        { id: 'dt_df', from: 'd_retrying', to: 'd_failed', kind: 'control', pts: [[608, 428], [740, 428]], label: '3 attempts in 5 min', lx: 674, ly: 420 }
      ]
    }
  ],
  scenarios: [
    {
      name: 'Passes',
      path: ['pending', 'running', 'completed', 'verdict', 'passed'],
      say: ['A Trigger created the run.', 'A runner claimed it and is judging.', 'It produced a result, so it has a verdict.', 'The verdict is read from the result row.', '<b>Passed.</b> Nothing else happens.']
    },
    {
      name: 'Retries, then pages',
      path: ['pending', 'running', 'retrying', 'running', 'completed', 'verdict', 'failed', 'd_pending', 'd_running', 'd_delivered'],
      say: [
        'A Trigger created the run.',
        'First attempt.',
        'The attempt hit a retryable error.',
        'Second attempt succeeds.',
        'Completed: a verdict exists.',
        'Read the verdict.',
        '<b>Failed.</b> This is a binding run, so its Operators fire.',
        'One dispatch per Operator.',
        'Sending to PagerDuty.',
        '<b>Delivered.</b> The on-call is paged.'
      ]
    },
    {
      name: 'Errors out',
      path: ['pending', 'running', 'retrying', 'running', 'retrying', 'running', 'errored'],
      say: [
        'A Trigger created the run.',
        'First attempt.',
        'Retryable error.',
        'Second attempt.',
        'Retryable error again.',
        'Third and last attempt.',
        '<b>Errored.</b> All 3 attempts are used. No verdict and no page: a failure to check, not a failure of your service.'
      ]
    }
  ]
};

const table = (id: string, x: number, y: number, title: string, cols: DiagramNode['cols'] = []): DiagramNode => ({
  id,
  kind: 'table',
  x,
  y,
  w: 214,
  h: 26 + cols.length * 20 + 6,
  title,
  cols
});
const rowY = (y: number, i: number) => y + 26 + 20 * i + 10;

/** The columns that join an observation to its verdict and the version that ran. */
export const evidenceJoins: DiagramSpec = {
  type: 'diagram',
  answers: 'Which columns connect an observation to its verdict and to the version that ran?',
  lives: 'understand/how-verification-works#results-are-evidence',
  detailHint: 'Pick a join above, or click a line in the drawing.',
  views: [
    {
      size: [860, 420],
      rest: 'Pick a join to see the SQL that follows it.',
      nodes: [
        table('eobs', 30, 20, 'vala.eval.observations', [{ name: 'record_id', key: true }, { name: 'run_id' }, { name: 'card_uid' }, { name: 'session_id' }, { name: 'context' }]),
        table('res', 322, 20, 'vala.verification.results', [
          { name: 'result_id', key: true },
          { name: 'source_record_id', fk: true },
          { name: 'run_id' },
          { name: 'binding_id' },
          { name: 'card_uid', fk: true },
          { name: 'verifier_card_uid', fk: true },
          { name: 'verdict' },
          { name: 'verifier_version' }
        ]),
        table('items', 630, 20, 'vala.eval.result_items', [{ name: 'result_id', fk: true }, { name: 'task_id' }, { name: 'passed' }, { name: 'actual' }, { name: 'message' }]),
        table('feat', 630, 210, 'vala.drift.result_features', [{ name: 'result_id', fk: true }, { name: 'feature' }, { name: 'score' }, { name: 'threshold' }, { name: 'verdict' }]),
        table('dobs', 30, 230, 'vala.drift.observations', [{ name: 'run_id' }, { name: 'card_uid' }, { name: 'series' }, { name: 'num_value' }, { name: 'created_at' }]),
        { id: 'cards', kind: 'declare', x: 322, y: 300, w: 214, h: 56, title: 'Card versions', sub: 'registry · cards.get' }
      ],
      edges: [
        { id: 'j1', kind: 'data', pts: [[244, rowY(20, 0)], [282, rowY(20, 0)], [282, rowY(20, 1)], [322, rowY(20, 1)]], label: 'record_id', lx: 286, ly: rowY(20, 0) - 6, anchor: 'start' },
        { id: 'j2', kind: 'data', pts: [[536, rowY(20, 0)], [630, rowY(20, 0)]], label: 'result_id', lx: 583, ly: rowY(20, 0) - 6 },
        { id: 'j3', kind: 'data', pts: [[536, rowY(20, 0)], [588, rowY(20, 0)], [588, rowY(210, 0)], [630, rowY(210, 0)]] },
        { id: 'j4', kind: 'best', pts: [[244, rowY(230, 1)], [296, rowY(230, 1)], [296, rowY(20, 4)], [322, rowY(20, 4)]], label: 'card + window', lx: 250, ly: rowY(230, 1) - 6, anchor: 'start' },
        { id: 'j5', kind: 'ref', pts: [[429, rowY(20, 6) + 16], [429, 300]], label: 'exact version', lx: 435, ly: 272, anchor: 'start' },
        { id: 'j6', kind: 'ref', pts: [[30, rowY(20, 1)], [14, rowY(20, 1)], [14, rowY(230, 0)], [30, rowY(230, 0)]] }
      ]
    }
  ],
  steps: [
    {
      label: 'verdict for a record',
      edgeFor: 'j1',
      nodes: ['eobs', 'res'],
      edges: ['j1'],
      caption: 'An eval result names the observation it judged in <code>source_record_id</code>.',
      sql: 'SELECT o.context, r.verdict, r.verifier_version\nFROM vala.eval.observations AS o\nJOIN vala.verification.results AS r ON r.source_record_id = o.record_id\nWHERE o.session_id = $1'
    },
    {
      label: 'why it failed',
      edgeFor: 'j2',
      nodes: ['res', 'items'],
      edges: ['j2'],
      caption: 'One row per eval task, keyed by <code>result_id</code>.',
      sql: "SELECT i.task_id, i.passed, i.actual, i.message\nFROM vala.verification.results AS r\nJOIN vala.eval.result_items AS i ON i.result_id = r.result_id\nWHERE r.verdict = 'failed'"
    },
    {
      label: 'which feature drifted',
      edgeFor: 'j3',
      nodes: ['res', 'feat'],
      edges: ['j3'],
      caption: 'One row per scored drift feature, keyed by <code>result_id</code>.',
      sql: 'SELECT f.feature, f.score, f.threshold, f.verdict\nFROM vala.verification.results AS r\nJOIN vala.drift.result_features AS f ON f.result_id = r.result_id\nWHERE r.result_id = $1'
    },
    {
      label: 'what drift saw',
      edgeFor: 'j4',
      nodes: ['dobs', 'res'],
      edges: ['j4'],
      caption: 'A scheduled drift result has no <code>run_id</code>. Match observations by subject Card and the result window instead.',
      sql: 'SELECT o.series, o.num_value, o.created_at\nFROM vala.verification.results AS r\nJOIN vala.drift.observations AS o\n  ON o.card_uid = r.card_uid\n AND o.created_at >= r.window_start AND o.created_at < r.window_end\nWHERE r.result_id = $1'
    },
    {
      label: 'which version ran',
      edgeFor: 'j5',
      nodes: ['res', 'cards'],
      edges: ['j5'],
      caption: 'Card UIDs point at one exact version in the registry. That lookup is <code>cards.get</code>, not SQL.',
      note: 'Read <code>card_uid</code> or <code>verifier_card_uid</code> from the row, then fetch that exact Card version with <code>Cards().get(...)</code> or the <code>cards.get</code> MCP tool.'
    },
    {
      label: 'one invocation',
      edgeFor: 'j6',
      nodes: ['eobs', 'dobs'],
      edges: ['j6'],
      caption: 'Every observation from one run shares its <code>run_id</code>, across tables.',
      sql: "SELECT 'eval' AS kind, created_at FROM vala.eval.observations WHERE run_id = $1\nUNION ALL\nSELECT 'drift', created_at FROM vala.drift.observations WHERE run_id = $1"
    }
  ]
};
