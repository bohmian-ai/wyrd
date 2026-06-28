# Fan-Out: Parallel Task-Contract Generation

The task stage generates one thin contract per DAG node. Nodes are independent at
this stage (they all read the same approved `spec.md` + `plan.md`), so generate
them in parallel.

## When to fan out vs. inline

- **≤ 3 commits:** generate inline, sequentially. The overhead of orchestration
  is not worth it.
- **> 3 commits:** fan out with a Workflow. One agent per node, in parallel.

## Workflow shape

The main agent reads `plan.md` + the `tasks.yaml` skeleton, then passes the node
list as `args`. Each agent writes its own `tasks/NN-*.md` and returns the filled
`tasks.yaml` node as structured output.

```js
export const meta = {
  name: 'wyrd-tasks-fanout',
  description: 'Generate one thin task contract per commit DAG node, in parallel',
  phases: [{ title: 'Generate' }],
}

const NODE_SCHEMA = {
  type: 'object',
  required: ['id', 'crates', 'seams', 'model', 'verify'],
  properties: {
    id: { type: 'string' },
    crates: { type: 'array', items: { type: 'string' } },
    seams: { type: 'array', items: { type: 'string' } },
    model: { enum: ['sonnet', 'opus'] },
    verify: { type: 'array', items: { type: 'string' } },
  },
}

// args = { featureDir, nodes: [{id, title, depends_on, cratesHint}] }
//
// The Workflow runtime delivers `args` VERBATIM, and the orchestrator routinely
// serializes it to a JSON STRING in the tool call. On a string, `args.nodes` is
// undefined and `.map(...)` throws "undefined is not an object" at 0s — before any
// agent spawns. Normalize so both an object and a stringified payload work.
const A = typeof args === 'string' ? JSON.parse(args) : (args || {})

// MIND THE PARENS: `await parallel(...).filter(...)` parses as
// `await (parallel(...).filter(...))` — `.filter` runs on the *Promise*, not the
// resolved array, and throws. Wrap the await first: `(await parallel(...)).filter`.
const nodes = (await parallel(A.nodes.map((n) => () =>
  agent(
    `You are generating ONE thin task contract for commit ${n.id} (${n.title}) of ` +
    `the Wyrd feature at ${A.featureDir}.\n` +
    `1. Read ${A.featureDir}/spec.md and ${A.featureDir}/plan.md for the ` +
    `   decisions and this commit's row in the DAG. Do NOT reopen decisions.\n` +
    `2. Use codegraph_explore to identify the seams this commit reuses or must ` +
    `   avoid. Record them as SYMBOLS, never file:line.\n` +
    `3. Write ${A.featureDir}/tasks/${n.id}-${n.title}.md using the thin-task ` +
    `   format: goal, binding decisions, seams & invariants, approach, ` +
    `   acceptance, verify, done-when. 40-70 lines. No rendered code.\n` +
    `4. Return the tasks.yaml node fields (id, crates, seams, model, verify). ` +
    `   model=opus only for concurrency / trait-object-safety / PyO3 boundary / ` +
    `   migrations / novel algorithms; else sonnet.`,
    { label: `task:${n.id}`, phase: 'Generate', schema: NODE_SCHEMA }
  )
)).filter(Boolean)

return { nodes }
```

**Invoking this Workflow:** pass `args` as an actual JSON **object** (`{ featureDir,
nodes: [...] }`), never a JSON-encoded string. The runtime hands `args` to the
script verbatim; a stringified payload arrives as a `string`, `args.nodes` is
`undefined`, and the fan-out crashes at 0s. The `const A = …JSON.parse…` line above
tolerates a string defensively, but the call site should still pass an object.

The main agent then merges the returned node fields into `tasks.yaml` (preserving
`depends_on` from the skeleton) and validates per the schema.

## Quality gate on each contract

Before accepting a generated contract, the main agent checks it:

- thin (40–70 lines), no rendered bodies, no `file.rs:line`;
- every load-bearing seam named with an invariant;
- decisions referenced from spec/plan, not re-derived.

Reject and regenerate any contract that under-names seams — that is the one
failure mode that degrades the downstream arch gate.
