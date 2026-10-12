# Task decomposition and lossless traceability

Organization amendment authorized by the user's explicit request on 2026-10-11: decompose original TASK-001 into the six agreed tasks without losing any information, agreed decision, implementation detail or requirement. Spec revision 4 is the behavioral authority (S is the sole Bifrost implementation; main survives only as X1–X9); M/S/B pins are unchanged. This is proposed planning, not implementation approval or a readiness verdict.

## Planning amendment 2026-10-12 — four checkpoints merged into one

At the user's request, former TASK-002, TASK-003, TASK-004 and TASK-005 are merged into one task, [TASK-002 Close the integration](tasks/TASK-002-close-the-integration.md). After spec revisions 3 and 4 the remaining work is one diff against S plus one list of failing lanes, and those failures cross the former boundaries. The former tasks are preserved verbatim as Parts A–D of the merged file, so no scenario, criterion, recipe, stop condition or recorded result is lost; former criterion IDs remain the acceptance units. In the tables below, `T002`, `T003`, `T004` and `T005` (and `TASK-003`/`TASK-004`/`TASK-005`) mean TASK-002 Parts A, B, C and D. The AC-028 ledger (P-028) moved from TASK-006 to TASK-002. Four sequential task reviews become one. The approved spec is unchanged.

## Active task sequence

| Task | Outcome | Direct dependency | Primary REQ/AC closure |
| --- | --- | --- | --- |
| [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | Source engine integration | None | REQ-001 / AC-001, REQ-002 / AC-002, REQ-003 / AC-003, REQ-004 / AC-004, REQ-025 / AC-025 (REQ-024 struck) |
| [TASK-002](tasks/TASK-002-close-the-integration.md) | Close the integration: S lifecycle owners with X3/X4/X8 (Part A), audit and ingress (Part B), verification results (Part C), public consumers (Part D), work queue, and the AC-028 ledger | TASK-001 | REQ-007 / AC-007 (X8 remainder), REQ-008 / AC-008, REQ-009 / AC-009, REQ-010–REQ-023 / AC-010–AC-023, AC-028 (REQ-005/006 struck) |
| [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) | Benchmark qualification and review handoff | TASK-002 | REQ-026 / AC-026, REQ-027 / AC-027 |

These are sequential internal checkpoints of one integrated delivery (three since the 2026-10-12 amendment). There is no parallel implementation/controller requirement. Each checkpoint closes its affected consumers/tests/artifacts/docs and receives independent task review; no partial candidate is released. Cross-cutting ACs assemble multiple recorded task contributions and receive integrated acceptance only after all contributions are complete.

## Required shared inputs

- [Approved specification](spec.md): all 27 original decisions, ten-table matrix, 12 invariants, non-goals, immutable inputs and final qualification.
- [Shared implementation contract](implementation-contract.md): complete execution/provenance/consumer/static-proof/stop requirements applicable to all tasks.
- [Complete exact proof catalog](proof-recipes.md): P01–P31, original recipe provenance, feature corrections and all 133 inspected literal Rust selectors.
- [Original TASK-001, verbatim](source/TASK-001-integrate-bifrost-variant-with-main.md): preserved full historical record, SHA-256 `20d65e5a4ac4acd6abe8a87ed593aa20b891ccee40f4bc99ac0125d2cbbbfe5d`. Its former one-task scope/order is superseded by this decomposition; no substantive approved requirement is retired.
- [Canonical evidence](evidence.md) and [reconciliation history](reconciliation.md): unchanged provenance/decision corpus. Current spec resolutions prevail over older contradictory research proposals.
- Independent plan-readiness review requested by the user: run after this decomposition is written and resolve material planning findings before handoff. It neither implements nor waives scope.

## Every requirement and acceptance obligation

The primary task owns closure coordination for the requirement's scoped implementation evidence, not an authority to mark the global AC passed before its supporting work exists. TASK-006 assembles the final all-27 matrix; final change review judges it. No requirement is dropped, weakened or satisfied merely by naming its ID.

| REQ / AC | Original approved decision | Primary task | Required implementation/proof contributions |
| --- | --- | --- | --- |
| REQ-001 / AC-001 | One source-authoritative analytical engine | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | T001-C1/S01-foundation, S03/S04/S20; T002 approved engine safeguards; T005 complete source/public journeys |
| REQ-002 / AC-002 | Source snapshot retention and deadlines | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | T001-C2/S02 retention/clamp; S `forge::reader_expiry_ordering::*`; T005 entrypoint/deadline regressions |
| REQ-003 / AC-003 | Source read-cut cache | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | T001-C2/S03 listener/cache lifecycle and current-DB fallback |
| REQ-004 / AC-004 | Source Scribe durability and recovery | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | T001-C2/S04/S20 group/WAL/fence/recovery; T002-C1 preserves flush recovery; T003 retries |
| REQ-005 / AC-005 | Struck in revision 3 | — | S staging/persistence unchanged; T002 P-S proves S restored |
| REQ-006 / AC-006 | Struck in revision 3 | — | S Forge and SQL unchanged; T002 P-S proves S restored |
| REQ-007 / AC-007 | Settlement model struck; X8 no-wait graph release remains | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | T002-C01/S07 inline release, late child via shared root, view-drop poison, one cancellation token, pre-first-batch `cancelled` |
| REQ-008 / AC-008 | Role heartbeat and one membership poller | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | T002-C4/S08 heartbeat/poller teardown and delayed Scribe discovery; T003 drain regression |
| REQ-009 / AC-009 | Scribe capability and routing independent of Oracle | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | T002-C5/S09 configured large private frame, frozen peer fences and bounded routing/retry |
| REQ-010 / AC-010 | One shared non-blocking Scribe outbox | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | T003-C1/S10/S11 one outbox/non-blocking permission/identical retries/loss; T004 result settlement; T005 drain |
| REQ-011 / AC-011 | Combined audit content and credential attribution | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | T003-C2/S11 real Gate/Oracle credential builders, created_at/Variant and retained fields |
| REQ-012 / AC-012 | Six-destination trusted internal writes | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | T003-C3/S10/S11 six destination ACK/readback/private attribution/public refusal |
| REQ-013 / AC-013 | UID-only observation correlation | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | T003-C4/S12 signed/unbound whole-frame UID authorization; T005 client encodings/journeys |
| REQ-014 / AC-014 | Acknowledged observations activate the correct writer's bindings | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | T004-C1/S13 original-created_at ACK activation/outage/writer/replay |
| REQ-015 / AC-015 | Main verification authority and runtime lifecycle | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | T004-C2/C3/S14 main hydration/cache/leases/schedule/gateway authority |
| REQ-016 / AC-016 | Typed Drift/Eval summaries and complete Task evidence | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | T004-C4/C5/S15 typed summaries, Task AssertionResult, app-Run identity and result_id joins |
| REQ-017 / AC-017 | Declared time axes for all retained tables | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 source foundation + T003 audit + T004 result axes; T005-C1/C2 S01/S16 closes all ten-table producers/readers/runtime projections |
| REQ-018 / AC-018 | No development agent trace table | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 removes dev/CodeAxis foundation/I18; T005-C2/S16 all-ten eager reads and transferred reachable generic assertions |
| REQ-019 / AC-019 | Preserve main typed query parameters | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 retains atomic request foundation; T005-C3/S17 all first-class ordered params/argument positions/source plans |
| REQ-020 / AC-020 | Atomic terminal problem protocol | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 atomic wire/type conversion foundation; T005-C4/S18 full problem/partial failure across server/forwarder/SDK/CLI/MCP |
| REQ-021 / AC-021 | Shared types and narrow first-class client projections | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 shared mapper foundation; T005-C1/C6/S01/S16/S19 narrow facade/runtime projections/Service preflight/lock outcome |
| REQ-022 / AC-022 | Main identity, credentials, SQL trust, and bootstrap | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T001 preserves SQL trust; T003-C5/S12 verified API-key entrances; T005-C5/C6/S19/S21 credential/platform/bootstrap/config preservation |
| REQ-023 / AC-023 | Retained product owners and ordered shutdown | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | T002 membership lifetime; T003/T004 shared writers; T005-C5/S19/S21 connected main owners/tracked tools/ordered drain |
| REQ-024 / AC-024 | Struck in revision 3 | — | Bifrost migrations are S files edited in place (INV-009); no forward/drop migration or migration-history test; T006 P-028 diff |
| REQ-025 / AC-025 | Fresh-only Bifrost, no implicit destructive transition | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | T001-C2/S20 v3 format/schema refusal without metadata/object/control-plane mutation |
| REQ-026 / AC-026 | Coherent native graph and retained main simplifications | [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) | T001-C4/I26 native graph/immutable compaction pin/no Ring/signing/tool preservation; T006-C1/C2/S23 retained workloads/consolidated target |
| REQ-027 / AC-027 | Consumer-complete tests, artifacts, and documentation | [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) | Every task-EVIDENCE consumer/assertion/generator/docs closure; T005 public surfaces; T006-C3/C4/I27 complete dispositions/registration/CI/global AC evidence |
| AC-028 | Bifrost scope has no trace of main except X1–X9 | [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) | T002-C04, T003-C06, T004-C06, T005-C08 per-write-set hunk maps; T006-C02 integrated ledger, struck-symbol absence, migration/table-schema diffs |

## Every invariant

| Invariant | Required semantics (verbatim) | Concrete owning contributions |
| --- | --- | --- |
| INV-001 | Source analytical architecture remains one engine; no alternate reader/storage/executor owner. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part A](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-002 | Tenant, Card, credential, peer fence/body, and object authority never widen on retries or adaptation. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-003 | Audit is one non-blocking authorization path; engine lineage never becomes permission audit. | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-004 | ACK/replay/publication retain exact durable identities and uncertainty evidence. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-005 | Role discovery remains live through necessary drain; all owners settle or report counted/visible loss. | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-006 | Observation subject/writer/application Run and Verifier/result identity remain distinct and exact. | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-007 | Declared types/time axes and logical Variant values agree across writers/readers/schema/layout. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-008 | Partial/failed/cancelled query streams are not successful complete results. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md); all tasks must preserve it |
| INV-009 | Main migration history and control-plane state survive; old Bifrost format is not silently admitted. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md); all tasks must preserve it |
| INV-010 | No old dev table/CodeAxis, publisher, audit chain, deleted scripts, descriptor, or duplicate harness is restored. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md), [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md); all tasks must preserve it |
| INV-011 | Main public integrations and all first-class clients consume the same source-authoritative contract. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md), [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md); all tasks must preserve it |
| INV-012 | Checks are not bypassed, generated output is not hand-edited, and private mechanics remain implementer-owned. | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md), [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md); all tasks must preserve it |

## Every original behavioral scenario

Every complete original block is retained verbatim in the indicated active task, including success/failure/edge behavior, expected RED, required existing-test extensions, GREEN, REFACTOR and mapped REQ/ACs. S01-foundation in TASK-001 is an additional prerequisite contribution; it does not replace or reduce original S01 in TASK-005.

| Original scenario | Original behavior/title | Owning task | Cross-task regression rule |
| --- | --- | --- | --- |
| S01 | Fresh registration and lossless typed rows | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S02 | Retained snapshots and source deadlines | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S03 | Invalidated or stopped caches cannot return frozen cuts | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S04 | ACK and recovery retain one exact logical batch | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S05 | Struck (REQ-005); S tests prove S staging | — | Replaced by T002 P-S |
| S06 | Struck (REQ-006); S tests prove S Forge | — | Replaced by T002 P-S |
| S07 | No-wait graph release and one cancellation signal (X8) | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | Rerun when Oracle admission/analytical/resources change. |
| S08 | Withdrawn roles keep discovery alive until teardown | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S09 | Independent fenced Scribe peers and configured transport | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S10 | One outbox delivers bounded identical private writes | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S11 | Audit records real credentials and never changes authorization | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S12 | UID and API-key scope are preserved at native and OTLP entrances | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S13 | ACK-derived Eval activation survives outage and next-day replay | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S14 | Verification keeps leases, schedule clock and gateway authority | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S15 | Every implementation retains its result and application lineage | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S16 | Ten built-ins and protected payloads use declared schemas | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S17 | Bound SQL stays data across first-class clients | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S18 | Failure after rows remains a typed failed terminal | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S19 | Main app owners and Service registration stay connected | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S20 | Fresh v3 hot/recovery/rewrite reads stay equivalent | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S21 | Shutdown and installed local bootstrap preserve real callers | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |
| S22 | Struck (REQ-024) | — | Replaced by T006 P-028 migration diff |
| S23 | Same benchmark workloads, consolidated compiled targets | [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) | Run affected accepted proof now and rerun this scenario when a later mapped consumer changes; full original assertions remain required. |

Original scenario order was monolithic. The new task order follows real ownership/dependencies. Cross-task 'rerun' references are regression obligations, not hidden forward dependencies. TASK-001's mechanical consumer foundations retain M outbox/auth/product/wire compatibility so TASK-002's lifecycle/routing proof can execute before TASK-003's full attribution composition. No temporary compatibility or broken schema/terminal contract is permitted.

## Every original proof group and static proof

| Proof group | Original purpose | Task recipe ownership |
| --- | --- | --- |
| P01 | Declared schema/time conformance (source lib owners) | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P02 | Source cache reset/invalidation | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) |
| P03 | Source deadline projection and retention configuration | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P04 | Source group/fence/crash recovery | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) |
| P05 | Deleted (REQ-005 struck) | — |
| P06 | Main Forge recipes deleted (REQ-006 struck); S retention/rewrite selectors survive in S lanes | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) P-S |
| P07 | Deleted (REQ-007 settlement struck); X8 proof is T002 S07 | — |
| P08 | Role task stop/abort/drop | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md) |
| P09 | Real membership lifetime and audit drain | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P10 | SQL role fences and private peer trust | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) |
| P11 | Shared outbox no-Oracle routing, bounded retry and refusal | [TASK-002 Part A](tasks/TASK-002-close-the-integration.md), [TASK-002 Part B](tasks/TASK-002-close-the-integration.md) |
| P12 | Native UID/ACK/audit projection and actual Gate adapter | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P13 | Real retained audit and six-destination private validator | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) |
| P14 | OTLP API-key and UID runtime entrances | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P15 | Evaluation/Task/trace/result layout real journeys | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P16 | Verification queue, schedule, lease and judging dependency proof | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md) |
| P17 | Result authoring and schema-by-name | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P18 | SQL bind forwarding and protected gateway payloads | [TASK-002 Part C](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P19 | Focused first-class Rust query/observation/verification stories | [TASK-002 Part B](tasks/TASK-002-close-the-integration.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P20 | Accepted Workflow owner accounting | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P21 | Real Workflow shutdown and forwarded query settlement | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P22 | Source logical Variant reads and recovered publication | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md), [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P23 | Focused Python/TypeScript public stories and imported source capability files | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P24 | Changed boot, credentials, signing boundary and platform initialization | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P25 | Installed CLI bootstrap (unmigrated fixture) | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P26 | Installed local callers in owning language runtimes | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P27 | Deleted (REQ-024 struck; no migration-history test) | — |
| P28 | SQL replay/readiness/RLS owners (no main upgrade path) | [TASK-001](tasks/TASK-001-integrate-bifrost-variant-with-main.md) |
| P29 | Mounted MCP, compiled CLI and late HTTP terminal | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P30 | Service before-activation preflight and served OpenAPI | [TASK-002 Part D](tasks/TASK-002-close-the-integration.md) |
| P31 | Consolidated benchmark target and new focused mode proof discovery | [TASK-006](tasks/TASK-006-qualify-workloads-and-integrated-evidence.md) |

| Static proof | Owner and unchanged proof obligation |
| --- | --- |
| I18 / AC-018 | TASK-001 removes sole-owner dev/CodeAxis; TASK-005 proves all-ten runtime schemas and transferred reachable assertions. |
| I24 / AC-024 | Struck (REQ-024); T006 P-028 proves migrations are S files with only X2/X5 in-place edits. |
| I26 / AC-026 | TASK-001 graph/pin/feature/profile/storage audit; TASK-006 packaging/tooling/workload parity handoff. |
| I27 / AC-027 | Every task closes source tests/registration/generation/docs for its write set; TASK-006 checks the complete canonical disposition and integrated evidence. |

Test recipe corrections remain bounded and non-weaker. Source S/M selectors are facts at pins, not promises that old standalone targets survive. Rehome into actual consolidated targets, discover actual names/features and record exact selectors before GREEN. Runtime prerequisites and test discovery never count as successful behavioral execution.

## Complete original-section disposition

| Original section | Frozen source line | Active disposition |
| --- | --- | --- |
| Outcome and Value | 15 | Scheduling decomposed into six scoped outcomes; all original behavior remains in spec/shared contract/tasks |
| Owners, Scope, Consumers, and Prohibited Changes | 30 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Source-backed reuse and all-27 obligation map | 58 | All 27 rows retained across task-local inspected reuse maps |
| Approach | 95 | Scheduling decomposed into six scoped outcomes; all original behavior remains in spec/shared contract/tasks |
| Ordered Implementation Scenarios | 115 | All 23 complete blocks retained; dependency-aware execution explained here |
| S01 — Fresh registration and lossless typed rows | 133 | Verbatim scenario retained in its owning task |
| S02 — Retained snapshots and source deadlines | 143 | Verbatim scenario retained in its owning task |
| S03 — Invalidated or stopped caches cannot return frozen cuts | 153 | Verbatim scenario retained in its owning task |
| S04 — ACK and recovery retain one exact logical batch | 163 | Verbatim scenario retained in its owning task |
| S05 — Flush waits for its snapshot, not unrelated future work | 173 | Struck in revision 3 |
| S06 — Forge safety within source retention and native rewrites | 183 | Main parts struck in revision 3; S parts proved by S lanes |
| S07 — Oracle drain accounts for late children and excludes Forge | 193 | Rewritten as X8 scenario in TASK-002 |
| S08 — Withdrawn roles keep discovery alive until teardown | 203 | Verbatim scenario retained in its owning task |
| S09 — Independent fenced Scribe peers and configured transport | 213 | Verbatim scenario retained in its owning task |
| S10 — One outbox delivers bounded identical private writes | 223 | Verbatim scenario retained in its owning task |
| S11 — Audit records real credentials and never changes authorization | 233 | Verbatim scenario retained in its owning task |
| S12 — UID and API-key scope are preserved at native and OTLP entrances | 243 | Verbatim scenario retained in its owning task |
| S13 — ACK-derived Eval activation survives outage and next-day replay | 253 | Verbatim scenario retained in its owning task |
| S14 — Verification keeps leases, schedule clock and gateway authority | 263 | Verbatim scenario retained in its owning task |
| S15 — Every implementation retains its result and application lineage | 273 | Verbatim scenario retained in its owning task |
| S16 — Ten built-ins and protected payloads use declared schemas | 283 | Verbatim scenario retained in its owning task |
| S17 — Bound SQL stays data across first-class clients | 293 | Verbatim scenario retained in its owning task |
| S18 — Failure after rows remains a typed failed terminal | 303 | Verbatim scenario retained in its owning task |
| S19 — Main app owners and Service registration stay connected | 313 | Verbatim scenario retained in its owning task |
| S20 — Fresh v3 hot/recovery/rewrite reads stay equivalent | 323 | Verbatim scenario retained in its owning task |
| S21 — Shutdown and installed local bootstrap preserve real callers | 333 | Verbatim scenario retained in its owning task |
| S22 — Main SQL history advances without control-plane loss | 343 | Struck in revision 3 |
| S23 — Same benchmark workloads, consolidated compiled targets | 353 | Verbatim scenario retained in its owning task |
| Acceptance Criteria | 383 | Every original REQ/AC/INV and inventory obligation mapped above, independently evidenced |
| Expected Write Set and Consumer Closure | 397 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Verification and Evidence | 414 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Exact proof recipe contract | 416 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Non-TDD and regression-only obligations | 442 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Required verification execution | 473 | Original section retained in shared contract and/or task-local scope/proof/consumer closure; frozen record preserves exact wording |
| Focused command catalog | 492 | All 31 groups retained with static source-selector evidence |
| P01 — Declared schema/time conformance (source lib owners) | 498 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P02 — Source cache reset/invalidation | 512 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P03 — Source deadline projection and retention configuration | 518 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P04 — Source group/fence/crash recovery | 525 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P05 — Main flush snapshot and concurrent ownership | 541 | Deleted (struck) |
| P06 — Forge term/quiescence, uncertainty and source non-blocking retention | 551 | Main parts deleted (struck) |
| P07 — Main owner-specific Oracle memory settlement | 577 | Deleted (struck) |
| P08 — Role task stop/abort/drop | 585 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P09 — Real membership lifetime and audit drain | 591 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P10 — SQL role fences and private peer trust | 597 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P11 — Shared outbox no-Oracle routing, bounded retry and refusal | 605 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P12 — Native UID/ACK/audit projection and actual Gate adapter | 611 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P13 — Real retained audit and six-destination private validator | 619 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P14 — OTLP API-key and UID runtime entrances | 626 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P15 — Evaluation/Task/trace/result layout real journeys | 634 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P16 — Verification queue, schedule, lease and judging dependency proof | 640 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P17 — Result authoring and schema-by-name | 648 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P18 — SQL bind forwarding and protected gateway payloads | 654 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P19 — Focused first-class Rust query/observation/verification stories | 670 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P20 — Accepted Workflow owner accounting | 682 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P21 — Real Workflow shutdown and forwarded query settlement | 688 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P22 — Source logical Variant reads and recovered publication | 694 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P23 — Focused Python/TypeScript public stories and imported source capability files | 708 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P24 — Changed boot, credentials, signing boundary and platform initialization | 728 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P25 — Installed CLI bootstrap (unmigrated fixture) | 738 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P26 — Installed local callers in owning language runtimes | 745 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P27 — Historical migration immutability | 754 | Deleted (struck) |
| P28 — Actual SQL replay/readiness/RLS/main upgrade owners | 762 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P29 — Mounted MCP, compiled CLI and late HTTP terminal | 773 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P30 — Service before-activation preflight and served OpenAPI | 783 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| P31 — Consolidated benchmark target and new focused mode proof discovery | 791 | Complete catalog retained in proof-recipes.md and selected active task recipes |
| Material Stop Conditions | 803 | Material stop contract retained in every task and shared contract |
| Authority Links | 825 | Authority links retained in every active task and shared contract |

## Global inventory and historical decisions

All 527 B→S changed records, 148 conflict paths and 76 automatic overlaps require reviewed canonical dispositions, with new discoveries appended. TASK-001 establishes the inventory, every task closes its touched paths and main-only retained seams, TASK-006 verifies total coverage. Overlapping classes do not require duplicated decisions, but grouped paths must be explicit. All L01–L64 and S1–S4 historical obligations remain in unchanged evidence/reconciliation; TASK-006 must tie their approved disposition to current implementation/proof and refuse blind replay of retired architecture.

## Final qualification and scope

The broad gate, both production geometry lanes and six benchmark parity commands/settings/order/exact-S baselines remain in spec Acceptance and evidence closure, executed once during immutable change review with full logging/telemetry/non-fail-fast semantics. TASK-006 delivers correct workload dispatch and complete baseline/review preparation; it cannot substitute static checks or preparation for those final results. No deployment, push, main merge, old-state deletion/migration or external fork publication is newly authorized.

Planning preserves all original scope and detail. No production code, tests, generated contracts, dependency pins or verification policy are changed by this decomposition.
