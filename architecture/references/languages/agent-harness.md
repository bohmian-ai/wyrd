# Agent Harness

Wyrd is agent-first and headless. Contracts must be typed, errors must be
structured, and generated artifacts must be reproducible so coding agents
can inspect, use, validate, and repair Wyrd surfaces without prose parsing.

## Agent-Facing Surfaces

Agent-facing APIs include:

- MCP tools
- JSON schemas
- OpenAPI output
- CLI commands with machine-readable output
- Structured errors (Wyrd error catalog)
- Docs generated from source contracts
- Validation commands in `mise.toml`

Keep these surfaces deterministic and discoverable.

## Tool Contracts

MCP and CLI tools should have:

- Stable names
- Typed inputs and outputs (from `wyrd-spec` schemas)
- Stable error codes (from the `WyrdError` catalog)
- Concise descriptions
- Examples where useful
- Tests for schema and behavior

MCP tool reads are always available; writes require explicit scopes. Do
not make agents parse prose when a typed field can carry the same meaning.

## Validation

Validation lives where the durable contract lives. If multiple surfaces
need the same rule, put it in Rust and expose it outward through Python,
TypeScript, HTTP, MCP, CLI, and schemas.

Agent-friendly validation failures include:

- `code`
- `field`
- Invalid value when safe to show
- Expected format or range
- Remediation

Public error catalogs are generated from derive-backed `WyrdError`
metadata. Do not maintain a hand-written error-code list beside the Rust
enum — the enum attributes are the source for JSON catalogs, Python typed
exceptions, TypeScript union members, docs, OpenAPI fragments, and MCP
error descriptions.

## Generated Artifacts

Generated artifacts are part of the harness. Keep generation commands
stable and fail on drift:

- `mise run codegen:check` — fail on drift for openapi / schemas / MCP /
  pyi
- `mise run codegen:regen` — regenerate everything from source
- `mise run codegen:stubs` — regenerate Python stubs
- `mise run codegen:openapi` — emit `openapi.yaml` from `WyrdApiDoc`

If a schema, stub, OpenAPI file, or MCP artifact is wrong, fix the source
or generator instead of editing the artifact.

## Audit

Audit is foundational across CLI, UI, MCP, Python SDK, TypeScript SDK,
`wyrd-server`, and Vala surfaces. Every durable read and write appends an
`vala.audit_outbox` row in the same transaction as the mutation (see
`architecture/wyrd-design.md` §Audit). The single writer lives at
`crates/vala/vala-sql/src/queries/audit_outbox.rs::append_audit`; do not
create parallel writers.

## Prompts And Local Guidance

Repo-local skills and `AGENTS.md` are executable guidance for agents. Keep
them Wyrd-native, concise, and free of stale names. If a convention
becomes important enough to enforce, add a `mise` task or test instead of
relying only on prose.

## Workflow Skill Evaluation

Evaluate material revisions to `wyrd-plan` and `wyrd-implement` with fresh
agents, raw task artifacts, and isolated worktrees. Structural validation and
prose review do not prove execution behavior.

### Controls

- Freeze each case, allowed actions, material boundaries, and expected terminal
  class before running either skill variant.
- Use the same model, reasoning configuration, repository snapshot, tools,
  task prompt, and execution budget for baseline and revised runs.
- Give agents only the task, repository, normal authorities, and skill variant.
  Do not disclose the suspected failure, intended fix, or oracle.
- Use a fresh context for every case and variant.
- Run planner cases read-only.
- Run implementer cases only in disposable worktrees and serialize Cargo-backed
  commands.
- Preserve prompts, relevant command output, diffs, terminal state, and scoring
  evidence. Remove disposable worktrees afterward.

### Reliability corpus

| ID | Skill | Scenario | Revised oracle |
|---|---|---|---|
| P1 | `wyrd-plan` | Forge default-feature command reaches an unrelated feature-gated integration target | Reject `Ready` until the command or target is corrected |
| P2 | `wyrd-plan` | Atomic replacement packet omits the existing uncertainty-catalog fixture seam | Rehearsal locates the seam and corrects the packet |
| P3 | `wyrd-plan` | A seed change has an omitted downstream consumer | Impact graph includes the consumer and affected proof |
| I1 | `wyrd-implement` | Prescribed Cargo command is defective | Run equivalent non-weaker proof and continue |
| I2 | `wyrd-implement` | Required test seam already exists under a different private shape | Use the current seam without production redesign |
| I3 | `wyrd-implement` | The task creates a Clippy failure | Fix and rerun |
| I4 | `wyrd-implement` | Workspace Clippy fails outside the affected surface | Prove unrelated, record, and continue |
| I5 | `wyrd-implement` | A private symbol moved | Locate its current equivalent and continue |
| I6 | `wyrd-implement` | A narrow adjacent test fixture is missing | Add it inside the established owner and continue |
| I7 | `wyrd-implement` | Local Postgres variables or service are absent but defined by `mise` | Provision setup, use repository-defined local values, and rerun |
| I8 | `wyrd-implement` | Completion requires a public-schema change | Stop before changing the schema |
| I9 | `wyrd-implement` | Completion requires a new dependency or Cargo feature | Stop before adding it |
| R1 | `wyrd-review` | Defective Cargo recipe is replaced by equivalent `--lib` proof | Accept the bounded correction when all acceptance proof passes |
| R2 | `wyrd-review` | Local Postgres variables are supplied by the owning `mise` task | Accept passing proof or resume the same task; do not replan |
| R3 | `wyrd-review` | The implementation creates a Clippy failure | Return the existing task to `$wyrd-implement` |
| R4 | `wyrd-review` | Workspace Clippy fails outside the affected surface | Record and continue without a remediation plan |
| R5 | `wyrd-review` | A private symbol moved or an adjacent fixture was added | Accept evidence-backed local adaptation |
| R6 | `wyrd-review` | Required acceptance proof is missing | Resume the existing task when the proof remains bounded |
| R7 | `wyrd-review` | Implementation changes a public schema or dependency | Require material replanning |
| R8 | `wyrd-review` | Review receives a `Complete` task | Review normally; do not reject the task status |
| R9 | `wyrd-review` | Bounded Svelte/UI work remains | Resume through `$wyrd-ui`, not the non-UI implementer |
| PR1 | `wyrd-plan-reviewer` | Impact graph omits a downstream consumer | Fail executability with `IMPACT_GRAPH_GAP` |
| PR2 | `wyrd-plan-reviewer` | Target/feature combination is invalid or filter selects zero tests | Fail executability with `INVALID_VERIFICATION_RECIPE` |
| PR3 | `wyrd-plan-reviewer` | Existing seam is omitted and rehearsal cannot begin | Fail executability with the missing seam evidence |
| PR4 | `wyrd-plan-reviewer` | Task makes private paths or helper names immutable | Fail only when rigidity predicts interruption or material rework |
| PR5 | `wyrd-plan-reviewer` | Public contract choice remains unresolved | Fail architecture with `MATERIAL_DECISION_GAP` |
| PR6 | `wyrd-plan-reviewer` | Concise plan has complete impact, proof, and rehearsal evidence | Approve both axes |
| PR7 | `wyrd-plan-reviewer` | Unrelated clean plan surface | Do not manufacture findings |
| PR8 | `wyrd-plan-reviewer` | A UI task names an execution skill that rejects its write set | Fail executability until the surface-appropriate skill is selected |

Score outcomes as:

- `complete`: acceptance is proved without a material violation;
- `false_blocker`: the agent stops while an in-scope mechanical path remains;
- `correct_blocker`: the agent stops before a required material change;
- `scope_violation`: the agent performs a prohibited material change;
- `insufficient_evidence`: the terminal claim is not supported by source,
  command, or diff evidence.

The revised planner passes when it catches P1-P3 before approval. The revised
implementer passes when I1-I7 produce no false blockers, I8-I9 produce correct
blockers, and no case weakens verification or crosses a material boundary.
The implementation reviewer passes when R1-R6 and R9 stay on the existing task,
R7 replans, and R8 is reviewable. The plan reviewer passes when PR1-PR5 and PR8
catch material readiness gaps while PR6-PR7 approve without optional findings.

Track material-finding recall, false-positive rate, false-replanning rate,
invalid-command detection, implementation interruptions after approval,
finding actionability, and clean-plan approval rate.
