import type { DiagramNode, DiagramSpec, Row, SequenceSpec, Tone } from './types';

/** Immediate `verify` versus queued `submit`, same participants. */
export const verificationPaths: SequenceSpec = {
  type: 'sequence',
  answers: 'Who waits for a verdict, and where is it stored?',
  lives: 'understand/how-verification-works',
  width: 860,
  participants: [
    { id: 'svc', kind: 'client', x: 140, title: 'Your service', sub: 'SDK in a run' },
    { id: 'srv', kind: 'component', x: 430, title: 'Verifier runtime', sub: 'wyrd-server' },
    { id: 'bif', kind: 'store', x: 720, title: 'Bifrost', sub: 'Scribe · Oracle' }
  ],
  flows: [
    {
      name: 'verify',
      rest: 'Your code sends named evidence and waits for the result, up to 60 seconds. The inputs and result are stored. Press Play to step through it.',
      messages: [
        {
          from: 'svc',
          to: 'srv',
          kind: 'control',
          label: 'observe.verify("policy-check", answer=…, policy=…)',
          say: 'The SDK projects each named value onto its slot. Files are hashed locally; only digests the server lacks are uploaded, straight to tenant storage, before the call.'
        },
        {
          from: 'srv',
          to: 'srv',
          kind: 'control',
          label: 'steps → requirements → decide',
          say: 'The server runs the exact Verifier version inside the request. Past 60 seconds the call fails with <code>WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT</code>.'
        },
        {
          from: 'srv',
          to: 'svc',
          kind: 'ref',
          label: 'VerificationResult · verdict, breakdown',
          say: 'The result comes back as an ordinary value. A <code>failed</code> verdict is not an exception; your code reads the requirement breakdown and decides.'
        },
        {
          from: 'srv',
          to: 'bif',
          kind: 'best',
          label: 'inputs + result rows',
          say: 'Before replying, the server stages the pinned inputs and the result on the Scribe outbox; Scribe writes them to <code>bifrost.verification.execution_inputs</code> and <code>bifrost.verification.results</code> afterward. No work is queued.'
        }
      ]
    },
    {
      name: 'submit',
      rest: 'Your code stages evidence and gets an execution ID back. The queue runs the same engine later, and your code reads the verdict with <code>wait</code>. Press Play.',
      messages: [
        { from: 'svc', to: 'srv', kind: 'control', label: 'observe.submit(...) · Idempotency-Key', say: 'The SDK uploads missing files, then posts the evidence once with one key it reuses on every transport retry.' },
        { from: 'srv', to: 'svc', kind: 'ref', label: '202 · execution_id', say: '202 confirms staging only. Replay and conflict are decided when the batched queue writer accepts the request.' },
        { from: 'srv', to: 'bif', kind: 'data', label: 'stage inputs', say: 'Inputs are staged on the Scribe outbox. Only after Scribe acknowledges them is the queued row created. Staged work lost before that has no durable outcome.' },
        {
          from: 'srv',
          to: 'srv',
          kind: 'control',
          label: 'execution · pending → running',
          say: 'The runner claims the queued execution and runs the same engine <code>verify</code> uses. Its status is separate from its verdict.'
        },
        {
          from: 'srv',
          to: 'bif',
          kind: 'data',
          label: 'result row · verdict',
          say: 'The result is written to <code>bifrost.verification.results</code>. <code>wait</code> polls the execution until the result is visible.'
        },
        {
          from: 'srv',
          to: 'svc',
          kind: 'ref',
          label: 'wait → VerificationResult',
          say: '<code>wait</code> returns the result once it is visible, from this process or any other holding the execution ID.'
        }
      ]
    }
  ]
};

const badge = (tone: string, glyph: string, word: string) =>
  `<span class="wd-badge${tone ? ' ' + tone : ''}">${glyph} ${word}</span>`;
const run = (status: string, result: string, ops: string, todo: string): Row[] => [
  ['Execution status', status],
  ['result', result],
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
  ['Where', 'Each Operator dispatch of the failed execution. Delivery never changes the verdict.']
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

/** Execution states, as named by `VerificationExecutionStatus`. */
export const RUN_STATES = ['pending', 'running', 'retrying', 'completed', 'cancelled', 'timed_out', 'errored'];
/** Verdicts, as named by `VerificationVerdict`. */
export const VERDICTS = ['passed', 'failed', 'inconclusive'];
/** Operator dispatch states, as named by `OperatorDispatchStatus`. */
export const DISPATCH_STATES = ['pending', 'running', 'retrying', 'delivered', 'failed'];

/** A verification execution: status, then verdict, then Operator dispatch. */
export const runLifecycle: DiagramSpec = {
  type: 'diagram',
  answers: 'What does this execution status mean, and what happens next?',
  lives: 'understand/how-verification-works',
  narrLabel: 'Now',
  detailHint: 'Click a state to see what you would see and what to do.',
  views: [
    {
      size: [860, 470],
      rest: 'An execution first has to run, then it has a verdict, then a failed verdict of a scheduled binding pages Operators. Click a state, or play a scenario.',
      groups: [
        { x: 16, y: 36, w: 300, h: 224, label: 'In progress' },
        { x: 376, y: 322, w: 468, h: 136, label: 'Each Operator, failed scheduled executions only' }
      ],
      texts: [
        { x: 508, y: 170, text: '‹ an attempt ran past 5 minutes', anchor: 'start' },
        { x: 508, y: 220, text: '‹ not retryable, or 3 attempts used', anchor: 'start' },
        { x: 508, y: 270, text: '‹ queued work Wyrd retired', anchor: 'start' }
      ],
      nodes: [
        state('pending', 36, 74, 'progress', '○', run(badge('', '○', 'pending'), 'null', 'None yet', 'Wait. The execution is queued and no runner has claimed it.')),
        state('running', 176, 74, 'progress', '▶', run(badge('', '▶', 'running'), 'null', 'None yet', 'Wait. Each attempt has up to 5 minutes; an execution gets 3 attempts.')),
        state('retrying', 176, 184, 'progress', '↻', run(badge('', '↻', 'retrying'), 'null', 'None yet', 'Read <code>error</code> to see why the last attempt failed. It will run again.')),
        state('completed', 380, 74, 'ok', '✓', run(badge('ok', '✓', 'completed'), 'Set', 'Depends on the verdict', 'Read the result from <code>status</code> or <code>wait</code>. Completed without a visible result: keep polling.')),
        state('timed_out', 380, 146, 'warn', '!', run(badge('warn', '!', 'timed_out'), 'null', 'Never', 'An attempt ran past 5 minutes, often a slow Agent step. No verdict exists.'), 120, 40),
        state('errored', 380, 196, 'danger', '✕', run(badge('danger', '✕', 'errored'), 'null', 'Never', 'Read <code>error.code</code> and <code>error.message</code>, such as an idempotency conflict. This is a failure to check, not a failure of your service.'), 120, 40),
        state('cancelled', 380, 246, 'muted', '–', run(badge('', '–', 'cancelled'), 'null', 'Never', 'Wyrd retired the queued execution before it produced a result, such as during an upgrade. Nothing to judge.'), 120, 40),
        {
          id: 'verdict',
          kind: 'decision',
          x: 556,
          y: 70,
          w: 92,
          h: 54,
          title: 'verdict',
          detail: verdict('Read from the result', 'The result’s <code>outcome.verdict</code>, also stored in <code>bifrost.verification.results</code>.', 'Only on <code>failed</code>', 'Read the requirement breakdown.')
        },
        state('passed', 700, 14, 'ok', '✓', verdict(badge('ok', '✓', 'passed'), 'The behavior met the standard.', 'None', 'Nothing.'), 140, 44),
        state(
          'inconclusive',
          700,
          74,
          'muted',
          '●',
          verdict(badge('', '●', 'inconclusive'), 'No judgment was possible. Never a pass.', 'None', 'Common causes: missing required evidence, a failed step, a provider error, a model pass without a valid citation, no applicable requirement, or too little drift data.'),
          140,
          44
        ),
        state(
          'failed',
          700,
          134,
          'danger',
          '✕',
          verdict(badge('danger', '✕', 'failed'), 'The behavior violated the standard.', 'Every <code>on_failure</code> Operator, for a scheduled binding', 'Read each requirement’s reason and citations. <code>verify</code> and <code>submit</code> page no one.'),
          140,
          44
        ),
        state('d_pending', 392, 350, 'progress', '○', dispatch(badge('', '○', 'pending'), 'Created when the execution settled as failed; waiting to be sent.'), 100, 40, 'pending'),
        state('d_running', 508, 350, 'progress', '▶', dispatch(badge('', '▶', 'running'), 'Being sent to Slack, PagerDuty, or your URL.'), 100, 40, 'running'),
        state('d_delivered', 624, 350, 'ok', '✓', dispatch(badge('ok', '✓', 'delivered'), 'The provider accepted it. Delivery is at least once, so duplicates are possible.'), 104, 40, 'delivered'),
        state('d_retrying', 508, 408, 'progress', '↻', dispatch(badge('', '↻', 'retrying'), 'A network error, 408, 429, or 5xx. It will be sent again.'), 100, 40, 'retrying'),
        state(
          'd_failed',
          740,
          408,
          'danger',
          '✕',
          dispatch(badge('danger', '✕', 'failed'), 'Gave up after 3 attempts, the 5-minute deadline, or a terminal refusal. Read its <code>error</code>; usually the operator connection.'),
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
      say: ['A schedule or <code>submit</code> queued the execution.', 'A runner claimed it and is judging.', 'It produced a result, so it has a verdict.', 'The verdict is read from the result row.', '<b>Passed.</b> Nothing else happens.']
    },
    {
      name: 'Retries, then pages',
      path: ['pending', 'running', 'retrying', 'running', 'completed', 'verdict', 'failed', 'd_pending', 'd_running', 'd_delivered'],
      say: [
        'A scheduled binding queued the execution.',
        'First attempt.',
        'The attempt hit a retryable error.',
        'Second attempt succeeds.',
        'Completed: a verdict exists.',
        'Read the verdict.',
        '<b>Failed.</b> A scheduled binding ran it, so its Operators fire.',
        'One dispatch per Operator.',
        'Sending to PagerDuty.',
        '<b>Delivered.</b> The on-call is paged.'
      ]
    },
    {
      name: 'Errors out',
      path: ['pending', 'running', 'retrying', 'running', 'retrying', 'running', 'errored'],
      say: [
        'A schedule or <code>submit</code> queued the execution.',
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
/** The parts of a Verifier and the server parts that run each step and check. */
export const verificationParts: DiagramSpec = {
  type: 'diagram',
  answers: 'What runs when a Verifier checks evidence, and where?',
  lives: 'understand/how-verification-works',
  views: [
    {
      size: [860, 450],
      rest: 'A binding attaches a Verifier to what it checks; your app sends evidence; steps and checks run in the server’s check engine, Monty pool, or Agent loop; <code>decide</code> combines the requirement verdicts. Choose a step, or press Play.',
      groups: [
        { x: 200, y: 14, w: 300, h: 340, label: 'Verifier' },
        { x: 528, y: 14, w: 320, h: 340, label: 'wyrd-server' }
      ],
      nodes: [
        { id: 'svc', kind: 'declare', x: 16, y: 40, w: 150, h: 54, title: 'What it checks', sub: 'verified_by' },
        { id: 'app', kind: 'client', x: 16, y: 200, w: 150, h: 58, title: 'Your app', sub: 'verify · submit' },
        { id: 'verdict', kind: 'evidence', x: 16, y: 300, w: 150, h: 54, title: 'Verdict', sub: 'per requirement' },
        { id: 'inputs', kind: 'component', x: 214, y: 40, w: 272, h: 58, title: 'inputs', sub: 'named evidence slots' },
        { id: 'steps', kind: 'component', x: 214, y: 120, w: 272, h: 58, title: 'steps', sub: 'optional: extract · agent · code' },
        { id: 'reqs', kind: 'component', x: 214, y: 200, w: 272, h: 58, title: 'requirements', sub: 'one check each' },
        { id: 'decide', kind: 'decision', x: 214, y: 280, w: 272, h: 58, title: 'decide', sub: 'all · pass_rate' },
        { id: 'engine', kind: 'component', x: 540, y: 40, w: 300, h: 50, title: 'Check engine', sub: 'assert · trace · drift · extract' },
        { id: 'monty', kind: 'component', x: 540, y: 110, w: 300, h: 58, title: 'Monty pool', sub: 'sandboxed Python · one worker per call' },
        { id: 'agent', kind: 'component', x: 540, y: 200, w: 150, h: 58, title: 'Agent loop', sub: 'agent step · check' },
        { id: 'tools', kind: 'component', x: 704, y: 200, w: 136, h: 58, title: 'Built-in tools', sub: 'cards · bifrost · fs' },
        { id: 'gateway', kind: 'component', x: 540, y: 290, w: 300, h: 50, title: 'LLM gateway', sub: 'your provider key' },
        { id: 'model', kind: 'client', x: 540, y: 380, w: 300, h: 54, title: 'Model provider', sub: 'OpenAI · Anthropic · …' }
      ],
      edges: [
        { id: 'bind', kind: 'ref', from: 'svc', to: 'inputs', pts: [[166, 67], [214, 67]], label: 'binding', lx: 190, ly: 59 },
        { id: 'ev', kind: 'data', from: 'app', to: 'inputs', pts: [[166, 229], [186, 229], [186, 84], [214, 84]], label: 'evidence', lx: 180, ly: 160, anchor: 'end' },
        { id: 'is', kind: 'data', from: 'inputs', to: 'steps', pts: [[350, 98], [350, 120]] },
        { id: 'sr', kind: 'data', from: 'steps', to: 'reqs', pts: [[350, 178], [350, 200]] },
        { id: 'rd', kind: 'data', from: 'reqs', to: 'decide', pts: [[350, 258], [350, 280]] },
        { id: 'dv', kind: 'data', from: 'decide', to: 'verdict', pts: [[214, 327], [166, 327]] },
        { id: 'xe', kind: 'control', to: 'engine', pts: [[500, 190], [516, 190], [516, 65], [540, 65]] },
        { id: 'xm', kind: 'control', to: 'monty', pts: [[500, 190], [516, 190], [516, 139], [540, 139]] },
        { id: 'xa', kind: 'control', to: 'agent', pts: [[500, 190], [516, 190], [516, 229], [540, 229]] },
        { id: 'at', kind: 'control', from: 'agent', to: 'tools', pts: [[690, 229], [704, 229]] },
        { id: 'tm', kind: 'control', from: 'tools', to: 'monty', pts: [[772, 200], [772, 168]], label: 'code.run', lx: 778, ly: 188, anchor: 'start' },
        { id: 'ag', kind: 'data', from: 'agent', to: 'gateway', pts: [[615, 258], [615, 290]] },
        { id: 'gm', kind: 'data', from: 'gateway', to: 'model', pts: [[690, 340], [690, 380]] }
      ]
    }
  ],
  steps: [
    {
      label: 'Bind',
      nodes: ['svc', 'inputs'],
      edges: ['bind'],
      caption:
        'A <code>verified_by</code> binding attaches the Verifier to what it checks: a Service, one of its components such as an Agent, Model, or Data, or a standalone Agent. The Verifier never names its subject, so one Verifier can check many.'
    },
    {
      label: 'Evidence',
      nodes: ['app', 'inputs', 'steps'],
      edges: ['ev', 'is'],
      caption:
        'Your app fills each named input with <code>verify</code> or <code>submit</code>. Optional steps prepare it first: <code>extract</code> reads a PDF or spreadsheet into text, and <code>code</code> or <code>agent</code> steps compute values later checks read.'
    },
    {
      label: 'Check engine',
      nodes: ['steps', 'reqs', 'engine'],
      edges: ['sr', 'xe'],
      caption:
        '<code>assert</code>, <code>trace</code>, and <code>drift</code> checks, and <code>extract</code> steps, run in the server itself. They need no model and no Python.'
    },
    {
      label: 'Monty pool',
      nodes: ['steps', 'reqs', 'monty'],
      edges: ['xm'],
      caption:
        '<code>code</code> steps and checks run sandboxed Python in the Monty pool: workers the server starts from its own binary. Each call gets a fresh worker with no host files, network, or environment, and a crash or memory overrun ends only that worker.'
    },
    {
      label: 'Agent loop',
      nodes: ['steps', 'reqs', 'agent', 'tools', 'monty', 'gateway', 'model'],
      edges: ['xa', 'at', 'tm', 'ag', 'gm'],
      caption:
        'An <code>agent</code> step or check runs an Agent whose model calls go through the LLM gateway with your provider key. Step and check Agents can call the built-in tools they declare: <code>cards.*</code>, <code>bifrost.*</code>, <code>fs.*</code> over the files mounted in their workspace, and <code>code.run</code> in the Monty pool. A check Agent’s pass must cite the evidence.'
    },
    {
      label: 'Verdict',
      nodes: ['reqs', 'decide', 'verdict'],
      edges: ['rd', 'dv'],
      caption:
        'Each requirement gets its own verdict. <code>decide</code> combines them: with <code>all</code>, any failure fails the Verifier, then any inconclusive makes it inconclusive, otherwise it passes.'
    }
  ]
};
