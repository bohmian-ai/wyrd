import type { DiagramNode, DiagramSpec, DiagramText, TimelineSpec } from './types';

/** Where each Bifrost part sits and which way data moves between them. */
export const bifrostParts: DiagramSpec = {
  type: 'diagram',
  answers: 'What are the parts of Bifrost, and which way does data move?',
  lives: 'understand/bifrost',
  views: [
    {
      size: [860, 430],
      rest: 'Every client enters through Gate. Choose Write path, Read path, or Background to follow one flow, or press Play.',
      groups: [{ x: 236, y: 14, w: 384, h: 402, label: 'wyrd-server' }],
      nodes: [
        { id: 'c1', kind: 'client', x: 16, y: 40, w: 176, h: 54, title: 'Your service', sub: 'run.observe.*' },
        { id: 'c2', kind: 'client', x: 16, y: 128, w: 176, h: 54, title: 'Bifrost client', sub: 'insert · sql' },
        { id: 'c3', kind: 'client', x: 16, y: 216, w: 176, h: 54, title: 'OpenTelemetry', sub: 'OTLP export' },
        { id: 'c4', kind: 'client', x: 16, y: 304, w: 176, h: 54, title: 'Agents', sub: 'MCP bifrost.query' },
        { id: 'gate', kind: 'component', x: 256, y: 170, w: 112, h: 90, title: 'Gate', sub: 'auth · routing' },
        { id: 'scribe', kind: 'component', x: 412, y: 44, w: 188, h: 64, title: 'Scribe', sub: 'ingest · live tail' },
        { id: 'oracle', kind: 'component', x: 412, y: 184, w: 188, h: 64, title: 'Oracle', sub: 'distributed query' },
        { id: 'forge', kind: 'component', x: 412, y: 324, w: 188, h: 64, title: 'Forge', sub: 'compaction · retention' },
        { id: 'obj', kind: 'store', x: 664, y: 36, w: 180, h: 150, title: 'Object storage', sub: 'Iceberg · Parquet' },
        { id: 'pg', kind: 'store', x: 664, y: 272, w: 180, h: 118, title: 'Postgres', sub: 'table catalog' }
      ],
      edges: [
        { id: 'e1', kind: 'data', pts: [[192, 67], [224, 67], [224, 200], [256, 200]] },
        { id: 'e2', kind: 'data', pts: [[192, 155], [230, 155], [230, 208], [256, 208]] },
        { id: 'e3', kind: 'data', pts: [[192, 243], [230, 243], [230, 218], [256, 218]] },
        { id: 'e4', kind: 'control', pts: [[192, 331], [224, 331], [224, 230], [256, 230]] },
        { id: 'gs', kind: 'data', pts: [[368, 190], [390, 190], [390, 76], [412, 76]], label: 'gRPC', lx: 396, ly: 140, anchor: 'start' },
        { id: 'go', kind: 'control', pts: [[368, 216], [412, 216]], label: 'HTTP', lx: 390, ly: 208 },
        { id: 'so', kind: 'data', pts: [[600, 76], [664, 76]], label: 'publish', lx: 632, ly: 68 },
        { id: 'tail', kind: 'best', pts: [[506, 108], [506, 184]], label: 'live tail', lx: 514, ly: 150, anchor: 'start' },
        { id: 'oo', kind: 'data', pts: [[664, 150], [636, 150], [636, 204], [600, 204]], label: 'published', lx: 630, ly: 142, anchor: 'end' },
        { id: 'fo', kind: 'data', pts: [[600, 346], [648, 346], [648, 178], [664, 178]], label: 'compact', lx: 606, ly: 338, anchor: 'start' },
        { id: 'fp', kind: 'ref', pts: [[600, 372], [664, 372]], label: 'catalog', lx: 632, ly: 364 },
        { id: 'op', kind: 'ref', pts: [[600, 232], [624, 232], [624, 300], [664, 300]] }
      ]
    }
  ],
  steps: [
    {
      label: 'Write path',
      nodes: ['c1', 'c2', 'c3', 'gate', 'scribe', 'obj'],
      edges: ['e1', 'e2', 'e3', 'gs', 'so'],
      caption:
        'Writes arrive over <b>gRPC</b> at <b>Gate</b>, which authenticates the credential, checks the permission, and routes the batch to a <b>Scribe</b>. Scribe acknowledges once the batch is in its write-ahead log, which is when <code>flush()</code> returns, then publishes the rows to object storage as Parquet.'
    },
    {
      label: 'Read path',
      nodes: ['c2', 'c4', 'gate', 'oracle', 'obj', 'scribe'],
      edges: ['e2', 'e4', 'go', 'oo', 'tail'],
      caption:
        'A query arrives over <b>HTTP</b> at <b>Gate</b>, which routes it to an <b>Oracle</b>. Oracle pins the published Iceberg snapshot and asks each Scribe for its <b>live tail</b>, the rows not published yet. Large queries fan out across peer Oracles. Results stream back and end with a terminal status: <code>success</code>, <code>degraded</code>, or <code>failed</code>.'
    },
    {
      label: 'Background',
      nodes: ['forge', 'obj', 'pg', 'oracle'],
      edges: ['fo', 'fp', 'op'],
      caption:
        '<b>Forge</b> runs on its own. One Forge leader decides which tables are due; Forge workers promote Scribe output into Iceberg snapshots, compact small files, and apply retention. Postgres holds the catalog and elects the leader. You never call Forge directly.'
    }
  ]
};

type Part = 'gate' | 'scribe' | 'oracle' | 'leader' | 'worker';
const PARTS: Part[] = ['gate', 'scribe', 'oracle', 'leader', 'worker'];
const PART_NAME: Record<Part, string> = {
  gate: 'Gate',
  scribe: 'Scribe',
  oracle: 'Oracle',
  leader: 'Forge leader',
  worker: 'Forge worker'
};

/** Every `WYRD_TARGET` value and the Bifrost parts it runs. */
export const TARGETS: { id: string; sub: string; parts: Part[]; api: boolean; peer?: boolean }[] = [
  { id: 'all', sub: 'everything · default', parts: ['gate', 'scribe', 'oracle', 'leader', 'worker'], api: true },
  { id: 'server', sub: 'API + Forge leader', parts: ['gate', 'scribe', 'oracle', 'leader'], api: true },
  { id: 'scribe', sub: 'writes · peer mode', parts: ['gate', 'scribe'], api: true, peer: true },
  { id: 'oracle', sub: 'queries · peer mode', parts: ['gate', 'oracle'], api: true, peer: true },
  { id: 'forge-worker', sub: 'compaction · no API', parts: ['worker'], api: false }
];

const runsOn = (part: Part) =>
  TARGETS.filter((t) => t.parts.includes(part))
    .map((t) => `<code>${t.id}</code>`)
    .join(', ');

function targetCards(): { nodes: DiagramNode[]; texts: DiagramText[] } {
  const nodes: DiagramNode[] = [];
  const texts: DiagramText[] = [{ x: 16, y: 22, text: 'WYRD_TARGET', anchor: 'start', cls: 'group' }];
  TARGETS.forEach((t, i) => {
    const x = 16 + i * 168;
    const y = 36;
    const w = 156;
    nodes.push({ id: t.id, kind: 'pod', x, y, w, h: 330, title: t.id, sub: t.sub, tags: t.parts });
    PARTS.forEach((p, j) => {
      const on = t.parts.includes(p);
      nodes.push({
        id: `${t.id}-${p}`,
        kind: 'chip',
        x: x + 10,
        y: y + 56 + j * 42,
        w: w - 20,
        h: 34,
        title: `${on ? '✓' : '·'}  ${PART_NAME[p]}`,
        tags: on ? [p] : [],
        off: !on,
        parent: t.id
      });
    });
    texts.push(
      { x: x + 12, y: y + 290, text: `${t.api ? '✓' : '✕'}  public API`, anchor: 'start', cls: t.api ? 'yes' : 'no' },
      {
        x: x + 12,
        y: y + 310,
        text: t.peer ? '!  needs peer mTLS' : '·  peers optional',
        anchor: 'start',
        cls: t.peer ? 'yes' : 'no'
      }
    );
  });
  return { nodes, texts };
}

const pod = (id: string, x: number, y: number, w: number, title: string, parts: Part[]): DiagramNode => ({
  id,
  kind: 'pod',
  x,
  y,
  w,
  h: 54,
  title,
  sub: parts.map((p) => PART_NAME[p]).join(' · '),
  tags: parts
});

const cards = targetCards();

/** Which pod runs which part: the `WYRD_TARGET` matrix and one example team. */
export const bifrostPods: DiagramSpec = {
  type: 'diagram',
  answers: 'Which WYRD_TARGET runs Gate, Scribe, Oracle, and Forge, and which do I scale?',
  lives: 'understand/bifrost#where-each-part-runs',
  detailHint: 'Pick Gate, Scribe, Oracle, Forge leader, or Forge worker above.',
  views: [
    {
      label: 'Targets',
      size: [860, 384],
      rest: 'Every pod is the same <code>wyrd-server</code> binary. <code>WYRD_TARGET</code> picks which parts it runs. The default, <code>all</code>, runs everything, which is what a single-pod install uses. Pick a part to see where it lives.',
      nodes: cards.nodes,
      edges: [],
      texts: cards.texts
    },
    {
      label: 'Example team',
      size: [860, 470],
      rest: 'One way a team splits Bifrost: Scribes on persistent disks for writes, autoscaled Oracles for queries, one <code>server</code> pod holding the Forge leader, and workers sized to the compaction backlog. Pick a part to follow it.',
      groups: [
        { x: 340, y: 14, w: 210, h: 166, label: 'StatefulSet · scribe ×2' },
        { x: 340, y: 214, w: 210, h: 166, label: 'Deployment + HPA · oracle ×2' },
        { x: 600, y: 14, w: 244, h: 96, label: 'server ×1' },
        { x: 600, y: 144, w: 244, h: 236, label: 'Deployment · forge-worker ×3' }
      ],
      nodes: [
        { id: 'sdk', kind: 'client', x: 16, y: 170, w: 140, h: 58, title: 'SDKs · agents', sub: 'OTLP · MCP' },
        { id: 'lb', kind: 'client', x: 186, y: 170, w: 120, h: 58, title: 'Load balancer', sub: 'yours' },
        pod('s0', 352, 40, 186, 'scribe-0', ['gate', 'scribe']),
        pod('s1', 352, 110, 186, 'scribe-1', ['gate', 'scribe']),
        pod('o0', 352, 240, 186, 'oracle-…', ['gate', 'oracle']),
        pod('o1', 352, 310, 186, 'oracle-…', ['gate', 'oracle']),
        pod('srv', 612, 40, 220, 'server-0', ['gate', 'scribe', 'oracle', 'leader']),
        pod('w0', 612, 170, 220, 'forge-worker-…', ['worker']),
        pod('w1', 612, 240, 220, 'forge-worker-…', ['worker']),
        pod('w2', 612, 310, 220, 'forge-worker-…', ['worker']),
        { id: 'obj', kind: 'store', x: 352, y: 400, w: 220, h: 58, title: 'Object storage', sub: 'Iceberg · Parquet' },
        { id: 'pg', kind: 'store', x: 612, y: 400, w: 220, h: 58, title: 'Postgres', sub: 'catalog · Forge election' }
      ],
      edges: [
        { id: 'in', kind: 'data', pts: [[156, 199], [186, 199]] },
        { id: 'w', kind: 'data', pts: [[306, 190], [322, 190], [322, 97], [352, 97]], label: 'gRPC writes', lx: 316, ly: 150, anchor: 'end' },
        { id: 'q', kind: 'control', pts: [[306, 208], [322, 208], [322, 297], [352, 297]], label: 'HTTP queries', lx: 316, ly: 256, anchor: 'end' },
        { id: 'due', kind: 'control', pts: [[810, 94], [810, 170]], label: 'due tables', lx: 804, ly: 132, anchor: 'end' }
      ],
      links: [
        {
          d: 'M538 67 L572 67 L572 337 L538 337 M538 137 L572 137 M538 267 L572 267 M572 67 L612 67',
          labels: [{ x: 575, y: 201, text: 'peer mTLS' }]
        }
      ],
      texts: [{ x: 340, y: 394, text: 'every pod shares both', anchor: 'start', cls: 'group' }]
    }
  ],
  facets: [
    {
      id: 'gate',
      label: 'Gate',
      caption:
        '<b>Gate.</b> The front door. Authenticates every request, checks the permission and records the decision in audit, then routes it: writes to a Scribe, queries to an Oracle, on the same pod or a peer.',
      rows: [
        ['Runs on', runsOn('gate')],
        ['Scale it when', 'It is not scaled on its own. It comes with every pod that serves the API.'],
        ['Needs', 'Nothing beyond the pod it runs in.'],
        ['Watch', '<code>bifrost_gate_requests_total</code>, <code>bifrost.gate.query</code> spans']
      ]
    },
    {
      id: 'scribe',
      label: 'Scribe',
      caption:
        '<b>Scribe.</b> Takes writes. Acknowledges a batch once it is in the write-ahead log, holds recent rows, serves them to queries as the live tail, and publishes them to object storage as Parquet.',
      rows: [
        ['Runs on', runsOn('scribe')],
        ['Scale it when', 'Write throughput, or ingest queueing (<code>WYRD_CLIENT_429_QUEUE_FULL</code> at clients).'],
        [
          'Needs',
          'A persistent disk for its write-ahead log; run it as a StatefulSet. While a Scribe is down, queries end <code>degraded</code> because its live tail is missing.'
        ],
        ['Watch', '<code>scribe_write_profile_nanoseconds_total</code>']
      ]
    },
    {
      id: 'oracle',
      label: 'Oracle',
      caption:
        '<b>Oracle.</b> Runs SQL. Pins a published Iceberg snapshot, asks every ready Scribe for its live tail, and fans large queries out across peer Oracles.',
      rows: [
        ['Runs on', runsOn('oracle')],
        ['Scale it when', 'Query load. Autoscale on the per-pod rate of <code>oracle_query_duration_seconds_count{outcome="success"}</code>.'],
        ['Needs', 'Scratch disk for spill. No persistent state, so it can scale up and down freely.'],
        ['Watch', '<code>oracle_query_duration_seconds</code>']
      ]
    },
    {
      id: 'leader',
      label: 'Forge leader',
      caption:
        '<b>Forge leader.</b> Decides maintenance. Exactly one is active per deployment, elected through Postgres; it tracks which tables are due for compaction and retention and hands them to workers.',
      rows: [
        ['Runs on', runsOn('leader')],
        ['Scale it when', 'Never. Run it on one or two pods (<code>all</code> or <code>server</code>); extra copies wait as standbys.'],
        ['Needs', 'Postgres for the election. It does no rewriting itself on a <code>server</code> pod.'],
        ['Watch', '<code>bifrost_forge_leader_decision_seconds</code>']
      ]
    },
    {
      id: 'worker',
      label: 'Forge worker',
      caption:
        '<b>Forge worker.</b> Does the rewriting. Pulls due tables from the leader, compacts small files into larger ones, applies retention, and commits new Iceberg snapshots.',
      rows: [
        ['Runs on', runsOn('worker')],
        ['Scale it when', 'Compaction backlog: when tables stay due, add <code>forge-worker</code> pods.'],
        ['Needs', 'Object storage and Postgres. It opens no public API, so nothing routes client traffic to it.'],
        ['Watch', 'Iceberg snapshot count per table']
      ]
    }
  ]
};

/** The gap between a row being stored and a row being queryable. */
export const bifrostWriteRead: TimelineSpec = {
  type: 'timeline',
  answers: 'I flushed a row. Why does my query not return it yet?',
  lives: 'understand/bifrost#written-versus-queryable',
  lanes: ['Your process', 'Scribe · stored', 'Iceberg · published'],
  stages: [
    { at: 0, name: 'insert()', sub: 'returns at once' },
    { at: 30, name: 'flush() returns', sub: 'Scribe acknowledged' },
    { at: 58, name: 'published', sub: 'shortly after' },
    { at: 86, name: 'compacted', sub: 'by Forge' }
  ],
  readouts: ['Where the row is', 'A query returns', 'Survives a restart'],
  move: 6,
  start: 12,
  stops: [12, 40, 70, 92],
  phases: [
    {
      from: 0,
      lane: 0,
      readouts: [
        { v: 'Queued in your process', s: 'Not sent yet' },
        { v: '<span class="wd-badge danger">✕ no rows</span>', s: 'Nothing reached the server' },
        { v: '<span class="wd-badge danger">✕ No</span>', s: 'Lost if the process dies before flush' }
      ],
      caption:
        'The row is queued in the SDK. <code>insert()</code> and <code>observe.*</code> return immediately so your request is not slowed down. Nothing is stored yet.'
    },
    {
      from: 30,
      lane: 1,
      ghostLane: 2,
      readouts: [
        { v: 'Held by Scribe', s: 'In its write-ahead log, not yet published' },
        { v: '<span class="wd-badge warn">! usually 1 row</span>', s: 'Oracle reads the live tail best effort' },
        { v: '<span class="wd-badge ok">✓ Yes</span>', s: 'Durable once acknowledged' }
      ],
      caption:
        '<code>flush()</code> has returned, so the row is durable. Oracle reads it from the holding Scribe as live tail, on a best-effort basis. An empty result now is not proof the row is missing. The dashed box marks where it will appear in the table.'
    },
    {
      from: 58,
      lane: 2,
      readouts: [
        { v: 'Published to Iceberg', s: 'Parquet in object storage' },
        { v: '<span class="wd-badge ok">✓ 1 row</span>', s: 'Every query sees it' },
        { v: '<span class="wd-badge ok">✓ Yes</span>', s: '' }
      ],
      caption: 'The row is part of the published table. Every query returns it from here on.'
    },
    {
      from: 86,
      lane: 2,
      readouts: [
        { v: 'Compacted', s: 'Merged into larger files' },
        { v: '<span class="wd-badge ok">✓ 1 row</span>', s: 'Same answer, faster reads' },
        { v: '<span class="wd-badge ok">✓ Yes</span>', s: '' }
      ],
      caption: 'Forge has merged small files into larger ones. Nothing changes for your query except speed.'
    }
  ]
};
