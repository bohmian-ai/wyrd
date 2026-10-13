# Shared implementation contract for the six-task decomposition

This document is required input for every task. The user explicitly requested six tasks, with no loss of information, decisions or implementation details. It replaces only the original task's scheduling and ownership; approved specification revision 2 behavior, pins, non-goals and proof strength remain unchanged.

## Immutable inputs and restart state

M: `8a8813a7458b7c98707c7b4f9b9d403f604ee0e6`; S: `cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`; B: `5ab92b0039404565e51f4a246863ce90cbd8d11f`. Fresh integration starts at `d48eb3ca2b942dfd243cdcb7cd123623d1adddcf`, the approved plan commit before the rejected attempt. Do not use the older variant TASK-001 worktree as S. The rejected candidate and dirty edits are archived separately; they are diagnostic evidence, not source authority or approved implementation.

Use the existing integration branch. The user's explicit archive/reset instruction superseded the original instruction never to reset that worktree; it does not authorize further reset, deployment, push or main merge. Preserve original research/benchmark packet commits and the combined compaction pin `22cd178d14009468bc951a14d7acf63237af808a`.

## Source fidelity and implementation authority

S owns Bifrost except the exact approved M overlaps enumerated in the spec. For each material changed production hunk, record its S/M/base origin and the requirement authorizing the composition. Necessary mechanical adaptations must preserve behavior in the existing owner. Passing tests do not justify unauthorized divergence. Confirmed drift is rejected; unexplained source divergence is not accepted as a harmless refactor.

Inspect existing S and M owners, complete callers and tests before drafting new code. Reuse their logic/assertions and current main fixture infrastructure. Do not invent replacement machinery. Required new cross-boundary assertions and minimal private fixture glue are permitted by the original task; retain stronger existing assertions, error handling and validation. An unapproved source-only semantic fix requires authority rather than a convenient test-driven redesign.

Execute one implementation task at a time with its direct predecessor accepted through independent task review. Scheduling is caller-owned; this packet introduces no coordinator/controller. Checkpoints are internal integration outcomes, never partial product release. Each task closes its necessary callers/tests/exports/fixtures/generated artifacts/docs immediately. A later task is no excuse for an incompatible wire/schema consumer, stubbed runtime owner, dropped main capability, broken earlier proof or weakened security boundary.

## Cross-task scenario and acceptance semantics

Original S01–S23 IDs remain stable and every complete Behavior/RED/GREEN/REFACTOR block is retained in its owning task. Local execution follows the six-task dependencies rather than blindly preserving the old monolithic numeric order. References such as 'rerun S17' mean rerun already implemented affected proof now, and rerun the earlier scenario when S17 is subsequently integrated; they do not create a forward dependency or waive regression closure. TASK-005 explicitly reruns affected engine/lifecycle/ingress/result proof after its consumers change.

TASK-001 must carry the mechanical public/producer/boot adapters needed for a cohesive executable source import, preserving already-approved main parameters, error identities, narrow projections, trusted ingress and product owners. Later tasks implement/prove their specified composed behaviors, not restore temporarily deleted main capabilities. Keep M's existing shared outbox connected through the foundation so TASK-002 can prove delayed-discovery drain; TASK-003 repeats that proof after changing its projection.

Every task records criterion-specific contributions to the global ACs. A task review may accept a completed scoped outcome; it cannot mark an integrated AC PASS while another explicitly mapped contribution is outstanding. TASK-006 reconciles complete integrated AC-001–AC-027 evidence; final immutable change review owns change acceptance. Source-preserved behavior can use verification-only/no-op proof, but missing required cross-boundary assertions cannot.

## Owners, Scope, Consumers, and Prohibited Changes

M means pinned main; S means pinned variant. Start from M
`8a8813a7458b7c98707c7b4f9b9d403f604ee0e6` and integrate S
`cfe21dc9007d5d22d9ae4411be07fb5504b0cc05`. Preserve branch provenance; inspect
three-way differences rather than treating endpoint differences as deletions.
Do not advance the input pins silently.

Vala owns the analytical engine and its SQL; Wyrd owns durable app/auth/security
and boot/lifecycle; wyrd-spec owns wire shapes; wyrd-client is the sole shared
SDK facade; language SDKs own only their runtime boundary. Existing concrete
owners remain discoverable and cohesive under AGENTS.md. Paths/symbols below
are inspected guidance, not a private implementation or file allowlist.

No deployment, push, main merge, destructive reset, old Bifrost data migration,
compatibility shim, new peer services, public broad internal client exports,
separate audit/capture/result publisher, test-only dev table, raw SQL connection
API, unrelated storage/signing/review-skill churn, or gate weakening is allowed.
Retain main's removed scripts/descriptor and all unrelated main-only behavior.

The existing remote branch and its research/benchmark handoff are preserved;
do not recreate/reset it. The approved revision supersedes conflicting research
options. The compaction fork prerequisite is supplied: use published commit
`22cd178d14009468bc951a14d7acf63237af808a` on the fork’s `wyrd/main`,
already pinned by remote integration commit `914495674`. Preserve this pin when
importing source manifests; resolve the combined lock/Hakari graph and verify
compatibility and the no-REST/Ring boundary without a waiver.

## Invariants and excluded outcomes

| ID | Required invariant |
|---|---|
| INV-001 | Source analytical architecture remains one engine; no alternate reader/storage/executor owner. |
| INV-002 | Tenant, Card, credential, peer fence/body, and object authority never widen on retries or adaptation. |
| INV-003 | Audit is one non-blocking authorization path; engine lineage never becomes permission audit. |
| INV-004 | ACK/replay/publication retain exact durable identities and uncertainty evidence. |
| INV-005 | Role discovery remains live through necessary drain; all owners settle or report counted/visible loss. |
| INV-006 | Observation subject/writer/application Run and Verifier/result identity remain distinct and exact. |
| INV-007 | Declared types/time axes and logical Variant values agree across writers/readers/schema/layout. |
| INV-008 | Partial/failed/cancelled query streams are not successful complete results. |
| INV-009 | Main migration history and control-plane state survive; old Bifrost format is not silently admitted. |
| INV-010 | No old dev table/CodeAxis, publisher, audit chain, deleted scripts, descriptor, or duplicate harness is restored. |
| INV-011 | Main public integrations and all first-class clients consume the same source-authoritative contract. |
| INV-012 | Checks are not bypassed, generated output is not hand-edited, and private mechanics remain implementer-owned. |

Excluded: deployment, main merge/push, destructive reset, retained-data migration,
mixed-version compatibility, new public peer services, new distributed scheduler,
new runtime/graph/transport layers, unrelated auth/storage/UI rewrites, blanket
lint suppression, synthetic host load, or changes to the inferred source-only
cache-timestamp/fleet-deadline concerns. These observations are evidence, not
new merge architecture instructions. A reproduced failure that prevents an
approved acceptance outcome still requires diagnosis and cannot be ignored.

## Ten-table time/type matrix

### REQ-017 — Declared time axes for all retained tables

Remove managed wyrd_event_time and use source types, precision, bounds, and
layout semantics consistently across ingestion, WAL, hot/sealed reads, Forge,
and consumers. card_uid/run_id are correlation-only; server-owned columns stay
unwritable. Dynamic tables default to wyrd_ingested_at or use a valid declared
non-null source-supported timestamp axis. Parent/detail joins use result_id
and cannot assume both rows share a day partition.

| Retained built-in | Declared time axis | Required adaptation |
|---|---|---|
| vala.traces.spans | start_time_unix_nano | Timestamp(ns,+00:00) OTLP instants; preserve exact ordering/bounds. |
| vala.metrics.points | time_unix_nano | Typed point/start/exemplar instants. |
| vala.logs.records | time_unix_nano | Typed event/observed instants. |
| vala.drift.observations | created_at | Timestamp(us,+00:00); retain main window/aggregation semantics. |
| vala.eval.observations | created_at | Variant context/media; preserve writer/Run inputs. |
| vala.verification.results | ended_at | Source typed summaries plus Task assertion_result Variant. |
| vala.drift.result_features | wyrd_ingested_at | result_id join; measured window stays domain time. |
| vala.eval.result_items | wyrd_ingested_at | Variant actual/expected; result_id join. |
| vala.gateway.calls | started_at | request_payload/response_payload Variant, preserving payload-read authorization. |
| vala.system.audit_log | created_at | Actual decision timestamp and Variant detail, no chain. |

## Evidence retained and incrementally closed

Keep `evidence.md` and `reconciliation.md` unchanged as provenance/decision history, subordinate to approved spec resolutions. The verbatim original task is `source/TASK-001-integrate-bifrost-variant-with-main.md`; its one-task and scheduling statements are historical, not active instructions. Its substantive requirements, scenarios, command recipes, implementation details and prohibitions remain preserved through this document, the new tasks and traceability map.

Start a canonical implementation disposition ledger in TASK-001 and update it in every task, covering all 527 changed source records, 148 conflicted paths and 76 automatically merged overlaps. These categories overlap; a canonical entry may carry several classifications but must enumerate its paths unambiguously. Reconcile L01–L64 and S1–S4 against actual integrated source/evidence, not merely the historical retention summary. Record preserved, approved-adapted, deliberately retired, generated, or unresolved disposition and the owning task. Do not discard source test bodies because names moved.

Keep an incremental source-test assertion map covering source main entrypoints and included modules, runtime boundaries, replacement selectors and any approved obsolete behavior. Preserve engine late-failure, real multi-batch IPC, ACK/readback/retry/backpressure, schema/Variant and tenant lifecycle proofs; modern consolidated targets are required. Final closure belongs to TASK-006, but earlier owners must complete their actual implementation/test work.

### Exact proof recipe contract

P01-P30 are source-inspected package/target/selector recipes; P31 explicitly
records discovery for new benchmark mode assertions rather than inventing names.
These are
not claims of execution or permanently fixed private test structure. A recipe
with imported source tests becomes executable after integration. Before running
it, confirm every exact selected symbol exists on the combined target; after
relocation use its actual discovered name and record the equivalent correction.
Do not count zero tests as success. Tests whose names express superseded reader
barriers must be adapted to source semantics rather than preserve those barriers.

All commands run from the integration repository root through mise. Postgres
wrapper owns isolated local DB setup and supplies serving/admin DSNs; never
substitute production credentials. PgFixture-based SQL/engine tests create and
migrate their own databases. Real server/SDK recipes use their inspected setup
migration task. Installed-bootstrap recipes intentionally do not pre-migrate.
Use default/minimal declared features, not all-features. Cargo-backed work is
sequential. On failure rerun with WYRD_LOG=info,vala_bifrost_redux=debug, verify
the subscriber, and read traces before diagnosing.

New runtime tests use their owning Python/Node process, never a Rust-emulated
foreign runtime. Source Rust client tests rehomed into main integration retain
test-support and get their actual module selectors after discovery; no invented
future selector is supplied. P23 defines bounded existing language files.

### Non-TDD and regression-only obligations

- **I18 (AC-018):** inspect canonical inventory/types/docs and removed-table
  callsites; no dev/CodeAxis owner remains. Runtime S16 covers supported table
  behavior. No manufactured RED for deleting unreachable sole-owner machinery.
- **I24 (AC-024):** compare historical bytes and migration versions to pinned
  main; inspect only genuine source migration additions. P27/P28 cover executable
  SQL transitions. Historical checklist corrections are static evidence.
- **I26 (AC-026):** combined-fork availability and dependency/feature/profile graph, directed lock/Hakari regeneration,
  script/descriptor deletions, lint policy and shared-storage consumer audit.
  Do not manufacture RED for configuration regeneration or retained simplification.
- **I27 (AC-027):** test registration/CI selection/source-generation/doc and full
  changed-path disposition audit. Source-owned generated artifacts use their
  generator; no hand-edited stubs. Benchmark dispatch has TDD in S23/P31;
  target metadata and test registration are static proof, not substitutes. Runtime scenarios separately prove behavior.

For the affected write set, run applicable focused owner/static checks:
`mise run fmt:check`, `mise run lints`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run check:workspace-hack`,
`mise run codegen:check`; Python `py:format:check`, `py:lints`, `py:typecheck`;
TypeScript `ts:format:check`, `ts:lints`, `ts:typecheck`, `ts:napi:check`;
`mise run docs:check` for actual docs-site changes; `git diff --check`.
Use `mise run codegen:regen` and the current Hakari commands to regenerate before
checking. Add only an affected owner lane whose required property is not proved
by its exact selectors; record why. Do not add full language/user-journey suites
or broad repository aggregates to this task. Those run once under spec closeout.

Record expected RED, actual GREEN/no-op, scoped regressions, features/setup,
exact final selectors and source/diff evidence per AC. Lack of runtime proof in
research is not a passing implementation result.

### Required verification execution

Expect substantial pedantic Clippy fallout while importing the rewrite. Fix
underlying code rather than adding `#[allow]`, `#[expect]`, `cfg_attr` lint
suppressions, new workspace exceptions, or weakened checks. Run each lane to
completion with non-fail-fast behavior and collect all runnable failures before
fixing them. Bucket the results by shared root cause; record setup/compile
blockers separately from executed failures and rerun affected proof after fixes.
The same execution requirements apply to the spec-owned final aggregate.

Set `WYRD_LOG=info,vala_bifrost_redux=debug`, `RUST_LOG` to the same filter,
and `RUST_BACKTRACE=1` for lane/gate invocations. Preserve production test
telemetry subscribers, tracers and metric recorders; keep full logs, exit
statuses, failing span/trace evidence and metric diagnostics. A missing
subscriber/recorder is a harness gap to fix. Do not cancel a lane after its
first error; if an aggregate stops early, run its remaining independent checks
and identify dependent checks blocked by prerequisites. Never turn incomplete
execution into a passing report. See the spec for required final closeout.

## Required implementation/review records

Each task records every assigned criterion separately, source/hunk dispositions, expected RED and actual GREEN/no-op, actual discovered exact selectors, setup/features, affected regressions, exit statuses, full logs and telemetry. Setup/build/discovery is not behavioral proof; zero selections or early-return missing-DB tests are not success. A trailing shell echo never substitutes for the test's exit/result. Preserve original P01–P31 and all assertions even when correcting syntax, features, relocated names or environment setup. `proof-recipes.md` contains the complete catalog and inspected selector evidence.

Use `$wyrd-implement` for implementation and fresh independent `$wyrd-task-review` for each immutable cumulative task. No task is complete on self-declared IMPLEMENTED. TASK-006 prepares the integrated `$wyrd-change-review`; its PASS invokes `$wyrd-complete`. This planning edit neither implements nor approves a candidate.

## Final qualification remains specification-owned

Task iteration does not run the broad gate, full language/journey suites or production benchmark parity repeatedly. Preserve every command and evidence condition in spec's Acceptance and evidence closure: final logging/telemetry/non-fail-fast gate, both production geometry lanes, and all six exact-S matched benchmark parity commands. TASK-006 owns workload packaging and baseline/handoff preparation. Final review executes and retains the whole-candidate qualification, including any independent lanes skipped after aggregate abort; do not infer PASS from preparation. No required final proof is removed by this split.

## Material Stop Conditions

- A required product/wire/persisted/security/lifecycle outcome contradicts
  approved revision 2 or needs a different decision. Finish unaffected work,
  cite the exact conflict and request a semantic revision; do not guess.
- Supplied compaction commit `22cd178d14009468bc951a14d7acf63237af808a`
  becomes unreachable or fails source Arrow60 compatibility or main no-REST/Ring
  narrowing in the combined graph. Report the concrete failure; do not replace
  the pin silently, waive checks, or publish externally without authorization.
- Work would reset/migrate retained old Bifrost objects or damage main control
  plane. Fresh-only scope does not authorize data deletion.
- A proposed fix requires a new public client/service, dependency family,
  cross-tenant/raw SQL boundary, scope widening, or optional feature not earned
  by the approved analytical graph/foreign-runtime boundary.
- User/main/source input pins change materially; record the delta and resolve
  its authority before extending this packet's scope.

Compiler, lint, setup, current test-name, private plumbing or sanctioned fixture
failures are ordinary implementation diagnosis, not material stop conditions.
A required red check blocks completion; no authorship hunt, older-code baseline
checkout, disabling assertion, extra sleep, or unsupported no-op substitute.

## Authority

Read approved `spec.md`, the active task, `decomposition.md`, this contract and its task-selected exact recipes. Follow repository AGENTS/agent-rules/design/doctrine/Bifrost/security authorities, spec-driven development, implementation execution and testing workflows. Source precedence and explicit exceptions stay as approved.
