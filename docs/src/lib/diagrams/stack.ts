import type { DiagramSpec } from './types';

/** Where the Wyrd server sits beside an application, CI, and a coding agent. */
export const wyrdInYourStack: DiagramSpec = {
  type: 'diagram',
  answers: 'Where does Wyrd sit next to my app, CI, and coding agent?',
  lives: 'get-started/introduction',
  views: [
    {
      size: [860, 430],
      rest: 'Your app, CI, and your coding agent all call one Wyrd server. Wyrd never calls into your app. Choose a step, or press Play.',
      groups: [{ x: 252, y: 14, w: 360, h: 402, label: 'wyrd-server' }],
      nodes: [
        { id: 'ci', kind: 'client', x: 16, y: 30, w: 188, h: 58, title: 'CI', sub: 'wyrd CLI' },
        { id: 'app', kind: 'client', x: 16, y: 176, w: 188, h: 70, title: 'Your app', sub: 'Python · Rust · TypeScript SDK' },
        { id: 'agent', kind: 'client', x: 16, y: 343, w: 188, h: 58, title: 'Coding agent', sub: 'MCP' },
        { id: 'decl', kind: 'declare', x: 284, y: 40, w: 296, h: 70, title: 'Declarations', sub: 'Services · Verifiers · versions' },
        { id: 'engine', kind: 'component', x: 284, y: 176, w: 296, h: 70, title: 'Verification engine', sub: 'checks · verdicts' },
        { id: 'bif', kind: 'evidence', x: 284, y: 312, w: 296, h: 70, title: 'Bifrost', sub: 'runs · observations · evidence' },
        { id: 'pg', kind: 'store', x: 660, y: 50, w: 184, h: 130, title: 'Postgres', sub: 'declarations · coordination' },
        { id: 'obj', kind: 'store', x: 660, y: 250, w: 184, h: 130, title: 'Object storage', sub: 'evidence · artifacts' }
      ],
      edges: [
        { id: 'cd', kind: 'control', from: 'ci', to: 'decl', pts: [[204, 59], [284, 59]], label: 'wyrd declare', lx: 244, ly: 51 },
        { id: 'ad', kind: 'control', from: 'app', to: 'decl', pts: [[204, 190], [240, 190], [240, 90], [284, 90]], label: 'declare', lx: 246, ly: 140, anchor: 'start' },
        { id: 'av', kind: 'control', from: 'app', to: 'engine', pts: [[204, 211], [284, 211]], label: 'verify', lx: 262, ly: 203 },
        { id: 'ao', kind: 'data', from: 'app', to: 'bif', pts: [[204, 232], [240, 232], [240, 340], [284, 340]], label: 'observe', lx: 246, ly: 290, anchor: 'start' },
        { id: 'ag', kind: 'control', from: 'agent', to: 'bif', pts: [[204, 372], [284, 372]], label: 'MCP', lx: 244, ly: 364 },
        { id: 'de', kind: 'ref', from: 'decl', to: 'engine', pts: [[432, 110], [432, 176]], label: 'bindings', lx: 440, ly: 147, anchor: 'start' },
        { id: 'eb', kind: 'data', from: 'engine', to: 'bif', pts: [[432, 246], [432, 312]], label: 'verdicts', lx: 440, ly: 283, anchor: 'start' },
        { id: 'dp', kind: 'ref', from: 'decl', to: 'pg', pts: [[580, 75], [660, 75]] },
        { id: 'eq', kind: 'ref', from: 'engine', to: 'pg', pts: [[580, 211], [630, 211], [630, 140], [660, 140]] },
        { id: 'bc', kind: 'ref', from: 'bif', to: 'pg', pts: [[580, 330], [640, 330], [640, 160], [660, 160]] },
        { id: 'bo', kind: 'data', from: 'bif', to: 'obj', pts: [[580, 360], [660, 360]], label: 'Parquet', lx: 620, ly: 352 }
      ]
    }
  ],
  steps: [
    {
      label: 'Declare',
      nodes: ['ci', 'app', 'decl', 'pg'],
      edges: ['cd', 'ad', 'dp'],
      caption:
        'CI runs <code>wyrd declare</code>, or your app calls <code>WyrdState.declare</code>. The server stores each Service and Verifier at its exact version in Postgres.'
    },
    {
      label: 'Observe',
      nodes: ['app', 'bif', 'obj', 'pg'],
      edges: ['ao', 'bo', 'bc'],
      caption:
        'Your app records what it did inside a Run. Bifrost writes those observations to object storage as Parquet and keeps its table catalog in Postgres.'
    },
    {
      label: 'Verify',
      nodes: ['app', 'decl', 'engine', 'bif', 'pg'],
      edges: ['av', 'de', 'eb', 'eq'],
      caption:
        'Your app sends evidence to a bound Verifier. The engine checks each requirement and returns a verdict, which Bifrost keeps with the exact versions it judged.'
    },
    {
      label: 'Coding agents',
      nodes: ['agent', 'bif', 'decl', 'engine'],
      edges: ['ag'],
      caption:
        'A coding agent connects through <code>wyrd mcp proxy</code> with your Wyrd credential. It reads declarations, verdicts, and evidence; write tools need explicit scopes.'
    }
  ]
};
