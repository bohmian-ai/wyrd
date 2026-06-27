// wyrd-implement — one dependency WAVE per invocation.
//
// The main agent computes waves from tasks.yaml and calls this once per wave,
// passing the wave's nodes as args. Within a wave, commits are independent, so
// every node runs in parallel in its own worktree. Between waves, the main agent
// integrates onto the feature branch (this script has no git/filesystem access).
//
// args = {
//   featureDir,            // ".dev/plan/<feature>"
//   base,                  // branch/sha the worktrees should branch from (feature tip)
//   maxIters,              // sonnet iterate-to-green budget before escalation (default 3)
//   tasks: [ { id, title, file, model, seams: [..], verify: [..] } ]
// }

export const meta = {
  name: 'wyrd-implement-wave',
  description: 'Implement one dependency wave of a Wyrd commit DAG in parallel, CodeGraph-hydrated',
  phases: [{ title: 'Implement' }],
}

const RESULT_SCHEMA = {
  type: 'object',
  required: ['id', 'status'],
  properties: {
    id: { type: 'string' },
    status: { enum: ['green', 'needs_escalation', 'blocked'] },
    branch: { type: 'string' },        // worktree branch holding the commit
    filesChanged: { type: 'array', items: { type: 'string' } },
    verifyRun: { type: 'array', items: { type: 'string' } },
    failure: { type: 'string' },       // compiler/test output when not green
  },
}

const maxIters = args.maxIters || 3

function executorPrompt(t) {
  return [
    `Implement ONE commit of the Wyrd feature at ${args.featureDir}.`,
    `Contract: ${args.featureDir}/${t.file}. Read it fully; it is authoritative.`,
    ``,
    `Procedure:`,
    `1. HYDRATE, do not re-read by hand. Use codegraph_explore to load the verbatim`,
    `   source + call paths for these seams: ${t.seams.join(', ') || '(none listed; read the contract)'}.`,
    `2. Honor the contract's decisions, seams, and INVARIANTS exactly. Do NOT reopen`,
    `   architectural decisions. If the contract is wrong or under-specified, STOP and`,
    `   return status "blocked" with the reason — do not improvise architecture.`,
    `3. Write code + tests following wyrd-rust-python doctrine (owning crate, WyrdError`,
    `   catalog, PyO3 boundary rules, tenant isolation + audit rows where the contract says).`,
    `4. Iterate to green against the contract's verify gates (targeted first):`,
    `   ${t.verify.map((v) => '`' + v + '`').join(', ')}.`,
    `   Budget: ${maxIters} attempts. If still red after ${maxIters}, return`,
    `   "needs_escalation" with the full failure output and what you tried.`,
    `5. Commit on this worktree's branch with a conventional message and return:`,
    `   id, status (green|needs_escalation|blocked), branch, filesChanged, verifyRun, failure.`,
  ].join('\n')
}

const results = await parallel(
  args.tasks.map((t) => () =>
    agent(executorPrompt(t), {
      label: `impl:${t.id}`,
      phase: 'Implement',
      model: t.model || 'sonnet',
      isolation: 'worktree',
      schema: RESULT_SCHEMA,
    })
  )
)

return { results: results.filter(Boolean) }
