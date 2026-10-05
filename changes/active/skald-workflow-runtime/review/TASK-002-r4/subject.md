# TASK-002 R4 immutable review subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `0569b79702218600c4f9790f45cc03100d5c6f1c`
- Candidate: `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, Revision 12
- Original active task: `changes/active/skald-workflow-runtime/tasks/TASK-002-cleanup.md`
- Prior remediation: `changes/active/skald-workflow-runtime/review/TASK-002-r2/TASK-002-R2-close-cleanup-review-gaps.md`
- Current remediation: `changes/active/skald-workflow-runtime/review/TASK-002-r3/TASK-002-R3-close-remaining-review-gaps.md`
- Prior verdicts and ledgers: `review/TASK-002-r1`, `review/TASK-002-r2`, and `review/TASK-002-r3`
- Review command: `git diff 0569b79702218600c4f9790f45cc03100d5c6f1c..2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
- CodeGraph: unavailable; the repository has no `.codegraph/` directory.

The candidate includes `375d97e67f3affe0d5c59727ef3135b22a459140`, the
implementation formerly associated with FIND-TASK-002-11. The human removed
that finding from the R3 remediation task; this review nevertheless audits the
commit as part of the complete cumulative diff.

## Human standing direction

Wyrd does what everyone else does. Flag as **DRIFT** any mechanism, check,
file, setting, or option that neither the established standard nor comparable
widely used projects have, and never require one in a remediation.

## Compact navigation map

| Surface | Changed owners and symbols | Likely callers / consumers | Main proof |
|---|---|---|---|
| Authored loading | `wyrd-loader::{load, validate}`; `wyrd-client::Workflow::from_path`; `CardGraphHydrator` runtime graph scope | Rust/Python/TypeScript Workflow facades | shared loader/client tests and three SDK journeys |
| Runtime composition | `skald-workflow::{WorkflowBodies, Workflow::from_card_with_agent_resolver}` | shared Workflow facade and server `EffectiveSpecs` validation | Skald/shared tests and native journey outputs |
| Registry planning and writes | `EffectiveSpecs`, registration planning, `RegistrationWriter`, relationship UID recheck | cards routes/service, SQL registration and lifecycle operations | `pg_workflow_registration`, `pg_cards_register`, tx-coupling check |
| Public contracts | `WyrdError::WorkflowInvalidCardRef`, `CardRef`; shared Cards/Workflow facade | all SDK wrappers and server error projection | error assertions, codegen and type checks |
| Python | `PyCards` typed workflow view and selector parsers; `PyWorkflow::from_path` | public `wyrd.cards` and `wyrd.agent` | Python unit plus real-server journey |
| TypeScript / Node | native Cards/Workflow wrappers and public `Workflow`, `Cards.workflow` | `@wyrd/sdk` callers | Vitest unit/integration, napi/type checks |
| Rust SDK | thin re-exports and `workflow_loading` journey | Rust SDK users | ignored real-server journey selected explicitly |
| Build/generated state | workspace-hack feature union, generated Python/TS declarations | workspace and packaging gates | hakari, codegen, lint/type checks |
| Review/authority artifacts | task/spec/skill/reference revisions and prior review records | implementation and review agents | scope/authority audit and cumulative diff hygiene |

Sensitive domains selected for independent review: (1) registry durability and
concurrency, (2) tenancy/security and authorization, and (3) async workflow
runtime/lifecycle behavior. Reviewers must expand this map against current
source and the complete cumulative diff; it is not a conclusion or file
allowlist.

## Available verification evidence

The R3 remediation records passing focused shared loader/client tests, server
registration and UID-race tests, the Rust/Python/TypeScript Workflow journeys,
Python selector tests, workspace-hack, cumulative diff hygiene, relevant
format/lint/type checks, and selected broader lanes. Those are implementation
claims to inspect, not substitutes for source or independent acceptance review.
The R3 evidence also records diagnosed fixes for the TypeScript journey timeout
and the server wrong-kind expectation. Reviewers must distinguish credible
existing proof from missing or weakened assertions.
