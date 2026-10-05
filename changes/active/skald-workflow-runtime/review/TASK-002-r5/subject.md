# TASK-002 R5 immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2d669917c03699876b3c8926f0de5ac88c578c01`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Remediation tasks: `review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md`, `review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md`, and `review/TASK-002-r4/TASK-002-R4-close-workflow-selector-validation.md`
- Prior verdicts and finding ledgers: `review/TASK-002-r1` through `review/TASK-002-r4`
- Cumulative review command: `git diff 0569b79702218600c4f9790f45cc03100d5c6f1c..2d669917c03699876b3c8926f0de5ac88c578c01`
- CodeGraph: unavailable because the repository has no `.codegraph/` directory.

The worktree was clean and `HEAD` equaled the requested candidate when this
review began. Only this R5 review directory may be written during the review.

## Human standing direction

Wyrd does what everyone else does. Flag as **DRIFT** any mechanism, check,
file, setting, or option that neither the established standard nor comparable
widely used projects have, and never require one in a remediation.

## Compact navigation map

| Surface | Changed owners and symbols | Likely callers and consumers | Relevant proof |
|---|---|---|---|
| Authored loading | `wyrd-loader::{load,validate}`; `wyrd-client::Workflow::from_path`; `CardGraphHydrator` and authored `WorkflowBodies` | Rust, Python, and TypeScript Workflow facades | loader/client tests and three SDK journeys |
| Runtime composition | `skald-workflow::{WorkflowBodies,Workflow::from_card_with_agent_resolver}` | shared Workflow facade and server `EffectiveSpecs` validation | Skald/shared tests and native journey outputs |
| Registry planning and durable writes | `EffectiveSpecs`, `RegistrationWriter`, relationship expected-UID recheck | cards service/routes, SQL registration and lifecycle operations | workflow registration, card lifecycle, transaction-coupling proof |
| Contract validation | `WyrdError::WorkflowInvalidCardRef`, `CardRef`, `VersionBlock::deserialize` | all SDK selectors and server parsing | semver Serde test and malformed-selector journeys |
| Python | `PyCards` typed Workflow view and selector parsing; `PyWorkflow::from_path` | public `wyrd.cards` and `wyrd.agent` | Python unit and real-server journey |
| TypeScript / Node | native Workflow selector parser and public `Workflow` / `Cards.workflow` | `@wyrd/sdk` callers | Vitest unit/integration, N-API and type checks |
| Rust SDK | thin shared-client re-exports and `workflow_loading` journey | Rust SDK users | ignored real-server journey selected explicitly |
| Build and generated state | workspace-hack union, SDK declarations, crate manifests | workspace and packaging gates | Hakari, codegen, lint/type/boundary checks |
| Review and authority artifacts | task/spec/skill/reference revisions and prior review records | implementation/review agents | authority and cumulative-scope audit |

Sensitive domains selected for independent review are registry durability and
concurrency, tenancy/security and authorization, and async Workflow runtime and
lifecycle behavior. Reviewers must expand this map against source; it is not a
conclusion or allowlist.

## Available evidence

The original task and R2-R4 remediation records claim passing focused loader,
client, server, SQL, Rust/Python/TypeScript journey, selector, codegen, type,
boundary, formatting, lint, and patch-hygiene checks. R5 independently reruns a
candidate-bound focused subset and records it in `verification.md`. All prior
results remain claims to inspect rather than substitutes for source review.
