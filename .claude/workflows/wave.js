// wyrd-implement — one dependency WAVE per invocation.
//
// The main agent computes waves from tasks.yaml and calls this once per wave,
// passing the wave's nodes as args. Within a wave, commits are independent, so
// every node runs in parallel — each in a worktree the MAIN AGENT pre-created at
// the correct base. Between waves, the main agent integrates onto the impl
// branch (this script has no git/filesystem access).
//
// IMPORTANT — worktree base ref: this script does NOT use Workflow's
// `isolation:'worktree'`. That option branches from the harness default (main),
// ignoring our base, which would hide prerequisites that live on the feature's
// base branch (e.g. wyrd-client-identity) and make executors rebuild the crate
// from scratch. Instead the orchestrator runs `git worktree add <path> <base>`
// itself — `base` = the impl-branch tip, which descends from the real base
// branch and includes every prior wave — and passes each task its `worktree`
// path. Executors operate INSIDE that path and never create or switch worktrees.
//
// args = {
//   featureDir,            // ".dev/plan/<feature>"
//   base,                  // impl-branch tip the orchestrator based the worktrees on (informational)
//   maxIters,              // sonnet iterate-to-green budget before escalation (default 3)
//   tasks: [ { id, title, file, model, worktree, crates: [..], seams: [..], verify: [..] } ]
//                          //   worktree = absolute path to the pre-created, correctly-based worktree
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

// The Workflow runtime delivers `args` VERBATIM. The orchestrator (an LLM following
// the wyrd-implement SKILL) routinely serializes it to a JSON STRING in the tool
// call. On a string, `args.tasks` is undefined and `args.tasks.map(...)` throws
// "undefined is not an object (evaluating 'args.tasks.map')" at 0s — before any
// agent spawns. Accept BOTH shapes so the wave can never fail on invocation form.
const A = typeof args === 'string' ? JSON.parse(args) : (args || {})
if (!Array.isArray(A.tasks) || A.tasks.length === 0) {
  throw new Error(
    'wave.js: args.tasks must be a non-empty array — got ' +
      (typeof args === 'string'
        ? 'a JSON string with no usable tasks (pass args as an OBJECT/array, not a stringified one)'
        : JSON.stringify(A.tasks)) +
      '. Invoke via the wyrd-implement orchestrator with args = { featureDir, base, tasks: [...] }.'
  )
}

const maxIters = A.maxIters || 3

function executorPrompt(t) {
  return [
    `Implement ONE commit of the Wyrd feature at ${A.featureDir}.`,
    `Contract: ${A.featureDir}/${t.file}. Read it fully; it is authoritative.`,
    ``,
    `Your worktree is ALREADY CREATED at: ${t.worktree}`,
    `It is correctly based on ${A.base} — all prerequisites (prior commits and`,
    `the feature's base-branch crates) are present. Prefix EVERY shell command with`,
    `\`cd ${t.worktree} && …\` so it runs inside the worktree. Do NOT create, switch,`,
    `or remove worktrees, do NOT git-checkout another branch, and do NOT touch the`,
    `primary working tree. The worktree has NO codegraph index of its own — that is`,
    `expected; hydrate from the primary tree's index via the CLI (below). Do not`,
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
  A.tasks.map((t) => () =>
    agent(executorPrompt(t), {
      label: `impl:${t.id}`,
      phase: 'Implement',
      model: t.model || 'sonnet',
      // NO isolation:'worktree' — see header. The orchestrator pre-creates each
      // worktree at the correct base; the executor cd's into t.worktree.
      schema: RESULT_SCHEMA,
    })
  )
)

return { results: results.filter(Boolean) }
