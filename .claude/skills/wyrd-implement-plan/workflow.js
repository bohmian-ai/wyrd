export const meta = {
  name: 'wyrd-implement-plan-preflight',
  description: 'Validate a Wyrd plan, resolve the baseline, and create the isolated execution worktree',
  whenToUse: 'Called by the wyrd-implement-plan skill before its serial controller loop. Not useful standalone.',
  phases: [
    { title: 'Validate', detail: 'run the plan validator and parse the task DAG' },
    { title: 'Isolate', detail: 'resolve HEAD and create the dedicated branch and worktree' },
  ],
}

// Preflight is deliberately the ONLY thing this workflow does. The controller
// loop stays in the skill's main context because it needs SendMessage to resume
// the same implementor across remediation, which workflow agent() cannot do.

const PLAN_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['ok', 'planPath', 'planStatus', 'slug', 'tasks', 'errors'],
  properties: {
    ok: { type: 'boolean', description: 'True only if the validator exited 0 and the plan is Approved.' },
    planPath: { type: 'string', description: 'Repo-relative path to implementation-plan.md.' },
    planStatus: { type: 'string', enum: ['Draft', 'Review Required', 'Approved'] },
    slug: { type: 'string', description: 'Plan directory slug under .dev/plan/.' },
    closeoutCommands: {
      type: 'array', items: { type: 'string' },
      description: 'Exact commands from the plan Closeout verification section.',
    },
    tasks: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['id', 'packet', 'status', 'riskTier', 'executionSkill', 'dependsOn'],
        properties: {
          id: { type: 'string', description: 'Task ID such as T1.' },
          packet: { type: 'string', description: 'Repo-relative path to the task packet.' },
          status: { type: 'string', enum: ['Planned', 'Ready', 'Complete', 'Blocked'] },
          riskTier: { type: 'string', enum: ['Luna', 'Terra', 'Sol'], description: 'Risk tier, NOT a model selection.' },
          executionSkill: { type: 'string', description: 'Verbatim Execution skill metadata value.' },
          dependsOn: { type: 'array', items: { type: 'string' } },
        },
      },
    },
    errors: { type: 'array', items: { type: 'string' }, description: 'Validator or coherence failures. Empty when ok.' },
  },
}

const ISOLATION_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['ok', 'branch', 'worktree', 'baselineSha', 'callerDirty', 'importedArtifacts', 'errors'],
  properties: {
    ok: { type: 'boolean' },
    branch: { type: 'string', description: 'Created execution branch name.' },
    worktree: { type: 'string', description: 'Absolute path to the dedicated worktree.' },
    baselineSha: { type: 'string', description: 'Full SHA of the caller HEAD the worktree was cut from.' },
    callerDirty: { type: 'boolean', description: 'Whether the caller worktree had uncommitted changes. Never modified either way.' },
    importedArtifacts: { type: 'array', items: { type: 'string' }, description: 'Plan artifacts imported, if any.' },
    errors: { type: 'array', items: { type: 'string' } },
  },
}

const planPath = args?.planPath
const repoPath = args?.repoPath ?? '.'
const stamp = args?.stamp // caller-supplied; Date.now() is unavailable in workflow scripts

if (!planPath) {
  return { ok: false, errors: ['workflow requires args.planPath'] }
}

phase('Validate')

const plan = await agent(
  `Validate a Wyrd implementation plan. Do NOT modify anything; this is read-only.

Plan directory or plan file: ${planPath}
Repository: ${repoPath}

Steps:
1. Resolve the plan directory (the parent of implementation-plan.md) and its slug.
2. Run: python3 .claude/skills/wyrd-plan/scripts/validate_plan_artifacts.py <plan-dir>
   Capture its exit code and full stderr.
3. Read implementation-plan.md. Extract the Status metadata value and the
   Closeout verification commands verbatim.
4. Parse the Task inventory table. For each row, open the referenced task packet
   and read its metadata: Status, Assigned model (this is a RISK TIER, not a
   model), Execution skill, Depends on.
5. Cross-check: every task in the inventory has a packet file that exists; every
   Depends-on ID resolves to a task in the inventory; no dependency cycle.

Set ok=true only when the validator exited 0, the plan Status is Approved, and
the cross-checks pass. Put every failure in errors[] as a specific, actionable
sentence. Do not attempt to repair the plan — the controller owns repair.`,
  { label: 'validate-plan', phase: 'Validate', schema: PLAN_SCHEMA },
)

if (!plan || !plan.ok) {
  log(`Preflight validation failed: ${(plan?.errors ?? ['agent returned nothing']).join('; ')}`)
  return { ok: false, stage: 'validate', plan: plan ?? null }
}

log(`Plan ${plan.slug} is Approved with ${plan.tasks.length} task(s).`)

phase('Isolate')

const isolation = await agent(
  `Create the isolated execution environment for Wyrd plan ${plan.slug}.

Repository: ${repoPath}
Plan directory: ${planPath}
Branch name suffix to use: ${stamp ?? plan.slug}

Rules — these are repository authority, not preferences:
1. Inspect git status WITHOUT changing it. Report whether the caller worktree is
   dirty. Never stash, reset, clean, checkout paths, or commit caller changes.
2. Verify the configured git identity is set. NEVER run git config to alter it.
   If identity is missing or wrong, fail with an error rather than setting it.
3. Resolve and record the caller HEAD full SHA. This is the baseline.
4. Create a uniquely named branch wyrd/implement-${plan.slug}-${stamp ?? 'run'}
   after verifying it does not already exist.
5. Create a clean worktree for that branch from the baseline SHA, under a
   task-specific temporary parent directory (NOT inside the repo, NOT a
   user-owned general directory). Report its absolute path.
6. If plan artifacts are absent from the baseline, import ONLY the plan
   directory into the worktree, preserving repo-relative paths. Before staging
   anything, check ignore status with: git check-ignore --no-index <path>
   An ignored artifact stays local and untracked. NEVER use git add -f or
   --force, and never override an ignore rule. Report imported paths.

Do not create any commit. Do not push. Set ok=false with specific errors[] if
any step cannot be completed cleanly.`,
  { label: 'create-worktree', phase: 'Isolate', schema: ISOLATION_SCHEMA },
)

if (!isolation || !isolation.ok) {
  log(`Isolation failed: ${(isolation?.errors ?? ['agent returned nothing']).join('; ')}`)
  return { ok: false, stage: 'isolate', plan, isolation: isolation ?? null }
}

log(`Execution branch ${isolation.branch} at ${isolation.worktree} (baseline ${isolation.baselineSha.slice(0, 8)}).`)

const ready = plan.tasks.filter((t) => t.status === 'Ready' && t.dependsOn.length === 0)
if (!ready.length) {
  log('WARNING: no root task is Ready. The controller must repair task status before dispatching.')
}

return {
  ok: true,
  slug: plan.slug,
  planPath: plan.planPath,
  closeoutCommands: plan.closeoutCommands ?? [],
  branch: isolation.branch,
  worktree: isolation.worktree,
  baselineSha: isolation.baselineSha,
  callerDirty: isolation.callerDirty,
  importedArtifacts: isolation.importedArtifacts,
  tasks: plan.tasks,
}
