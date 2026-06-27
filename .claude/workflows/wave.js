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
    `You run in a git worktree that has NO codegraph index of its own — that is`,
    `expected. Hydrate from the main repo's index via the CLI (below); do not try to`,
    `build a picture of the codebase by reading files.`,
    ``,
    `Procedure:`,
    `1. HYDRATE — do not spelunk. Run ONE Bash call:`,
    `     codegraph explore "${(t.seams.length ? t.seams : ['(read the contract for seam symbols)']).join(' ')}"`,
    `   It auto-resolves the primary working tree's index (the original clone dir,`,
    `   not this worktree) and returns the verbatim source +`,
    `   call paths + blast radius for these seams. It will print a "results come from`,
    `   a different git worktree" warning — IGNORE IT: seams are pre-existing symbols,`,
    `   and their committed source on the base branch is exactly what you want. Re-run`,
    `   codegraph explore with more symbol names if the contract references a seam you`,
    `   still can't see. Add more codegraph calls as needed — but do NOT use cat/ls/`,
    `   grep/head/find or "cargo metadata" to READ or DISCOVER source. (You MAY read`,
    `   the contract file, edit/write files, and run build/test/lint gates.)`,
    `2. Follow the contract's ## Approach steps LITERALLY and in order. They are`,
    `   prescriptive (e.g. exact git mv / Cargo edits / modules to delete). Do not`,
    `   re-derive the layout, and do not read crates outside this commit's scope`,
    `   (${(t.crates && t.crates.length) ? t.crates.join(', ') : 'see the contract'}).`,
    `3. Honor the contract's decisions, seams, and INVARIANTS exactly. Do NOT reopen`,
    `   architectural decisions. If the contract is wrong, under-specified, or names a`,
    `   seam codegraph cannot find, STOP and return status "blocked" with the reason —`,
    `   do not improvise architecture and do not go spelunking to fill the gap.`,
    `4. Write code + tests following wyrd-rust-python doctrine (owning crate, WyrdError`,
    `   catalog, PyO3 boundary rules, tenant isolation + audit rows where the contract says).`,
    `5. Iterate to green against the contract's verify gates (targeted first):`,
    `   ${t.verify.map((v) => '`' + v + '`').join(', ')}.`,
    `   Budget: ${maxIters} attempts. If still red after ${maxIters}, return`,
    `   "needs_escalation" with the full failure output and what you tried.`,
    `6. Commit on this worktree's branch with a conventional message and return:`,
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
