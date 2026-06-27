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
const nodes = await parallel(args.nodes.map((n) => () =>
  agent(
    `You are generating ONE thin task contract for commit ${n.id} (${n.title}) of ` +
    `the Wyrd feature at ${args.featureDir}.\n` +
    `1. Read ${args.featureDir}/spec.md and ${args.featureDir}/plan.md for the ` +
    `   decisions and this commit's row in the DAG. Do NOT reopen decisions.\n` +
    `2. Use codegraph_explore to identify the seams this commit reuses or must ` +
    `   avoid. Record them as SYMBOLS, never file:line.\n` +
    `3. Write ${args.featureDir}/tasks/${n.id}-${n.title}.md using the thin-task ` +
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

The main agent then merges the returned node fields into `tasks.yaml` (preserving
`depends_on` from the skeleton) and validates per the schema.

## Quality gate on each contract

Before accepting a generated contract, the main agent checks it:

- thin (40–70 lines), no rendered bodies, no `file.rs:line`;
- every load-bearing seam named with an invariant;
- decisions referenced from spec/plan, not re-derived.

Reject and regenerate any contract that under-names seams — that is the one
failure mode that degrades the downstream arch gate.
