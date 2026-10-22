import type { DiagramSpec } from './types';

/** What a team runs: one server binary in role-targeted pods behind one endpoint. */
export const wyrdDeployment: DiagramSpec = {
  type: 'diagram',
  answers: 'What do I run for a team, and what does it connect to?',
  lives: 'understand/architecture',
  views: [
    {
      size: [860, 410],
      rest: 'Every pod is the same <code>wyrd-server</code> binary; <code>WYRD_TARGET</code> picks its role. One pod running <code>all</code> is a complete deployment. Choose a step, or press Play.',
      groups: [{ x: 346, y: 14, w: 250, h: 360, label: 'wyrd-server pods' }],
      nodes: [
        { id: 'apps', kind: 'client', x: 16, y: 40, w: 150, h: 52, title: 'Your apps', sub: 'SDK' },
        { id: 'ci', kind: 'client', x: 16, y: 112, w: 150, h: 52, title: 'CI', sub: 'CLI' },
        { id: 'agents', kind: 'client', x: 16, y: 184, w: 150, h: 52, title: 'Coding agents', sub: 'MCP' },
        { id: 'people', kind: 'client', x: 16, y: 256, w: 150, h: 52, title: 'People', sub: 'browser' },
        { id: 'lb', kind: 'component', x: 196, y: 130, w: 120, h: 80, title: 'Load balancer', sub: 'yours · TLS' },
        { id: 'api', kind: 'pod', x: 360, y: 40, w: 222, h: 58, title: 'server', sub: 'API · verification · LLM gateway' },
        { id: 'scribe', kind: 'pod', x: 360, y: 120, w: 222, h: 58, title: 'scribe', sub: 'writes · persistent disk' },
        { id: 'oracle', kind: 'pod', x: 360, y: 200, w: 222, h: 58, title: 'oracle', sub: 'SQL queries' },
        { id: 'worker', kind: 'pod', x: 360, y: 280, w: 222, h: 58, title: 'forge-worker', sub: 'compaction · no API' },
        { id: 'idp', kind: 'client', x: 640, y: 24, w: 204, h: 56, title: 'Identity provider', sub: 'OIDC sign-in' },
        { id: 'models', kind: 'client', x: 640, y: 100, w: 204, h: 56, title: 'Model providers', sub: 'your keys' },
        { id: 'pg', kind: 'store', x: 640, y: 196, w: 204, h: 72, title: 'Postgres', sub: 'declarations · coordination' },
        { id: 'obj', kind: 'store', x: 640, y: 290, w: 204, h: 72, title: 'Object storage', sub: 'evidence · artifacts' }
      ],
      edges: [
        { id: 'a', kind: 'data', from: 'apps', to: 'lb', pts: [[166, 66], [180, 66], [180, 150], [196, 150]] },
        { id: 'c', kind: 'control', from: 'ci', to: 'lb', pts: [[166, 138], [182, 138], [182, 160], [196, 160]] },
        { id: 'g', kind: 'control', from: 'agents', to: 'lb', pts: [[166, 210], [182, 210], [182, 180], [196, 180]] },
        { id: 'p', kind: 'control', from: 'people', to: 'lb', pts: [[166, 282], [180, 282], [180, 190], [196, 190]] },
        { id: 'la', kind: 'control', from: 'lb', to: 'api', pts: [[316, 150], [328, 150], [328, 69], [360, 69]], label: 'HTTP', lx: 334, ly: 110, anchor: 'start' },
        { id: 'ls', kind: 'data', from: 'lb', to: 'scribe', pts: [[316, 165], [338, 165], [338, 149], [360, 149]], label: 'gRPC', lx: 322, ly: 140, anchor: 'start' },
        { id: 'lo', kind: 'control', from: 'lb', to: 'oracle', pts: [[316, 185], [338, 185], [338, 229], [360, 229]] },
        { id: 'ai', kind: 'ref', from: 'api', to: 'idp', pts: [[582, 52], [640, 52]], label: 'JWKS', lx: 611, ly: 44 },
        { id: 'am', kind: 'data', from: 'api', to: 'models', pts: [[582, 84], [612, 84], [612, 128], [640, 128]], label: 'model calls', lx: 618, ly: 120, anchor: 'start' },
        { id: 'sp', kind: 'ref', pts: [[596, 232], [640, 232]] },
        { id: 'so', kind: 'data', pts: [[596, 326], [640, 326]] }
      ],
      texts: [{ x: 360, y: 362, text: 'every pod shares Postgres and object storage', anchor: 'start', cls: 'group' }]
    }
  ],
  steps: [
    {
      label: 'One endpoint',
      nodes: ['apps', 'ci', 'agents', 'people', 'lb', 'api', 'scribe', 'oracle'],
      edges: ['a', 'c', 'g', 'p', 'la', 'ls', 'lo'],
      caption:
        'Every client reaches one address. Your load balancer terminates TLS and routes HTTP to API pods, Bifrost writes over gRPC to <code>scribe</code> pods, and queries to <code>oracle</code> pods. It never decides who the caller is; the Wyrd token does.'
    },
    {
      label: 'Identity',
      nodes: ['people', 'idp', 'api'],
      edges: ['p', 'ai'],
      caption:
        'People sign in through your OIDC provider. Apps, CI, and agents use API keys or workload identity. Every request is checked against the caller’s roles and tenant.'
    },
    {
      label: 'Model calls',
      nodes: ['api', 'models'],
      edges: ['am'],
      caption:
        'A model-judged check, or your app calling through the LLM gateway, reaches the provider from the server with your provider key. Each call is authorized and recorded.'
    },
    {
      label: 'Storage',
      nodes: ['api', 'scribe', 'oracle', 'worker', 'pg', 'obj'],
      edges: ['sp', 'so'],
      caption:
        'Postgres holds declarations, permissions, and coordination. Object storage holds evidence and artifacts. <code>forge-worker</code> pods compact Bifrost tables in the background and take no client traffic.'
    }
  ]
};
