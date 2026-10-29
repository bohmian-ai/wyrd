# R1 bounded source review

**Formal verdict: BLOCKED — no new immutable candidate commit.** The inspected
working-tree source supports closure of the bounded R1 corrections. No new
material source finding is retained. The whole change remains **FIX_REQUIRED**
under the original integrated review because R2 and R3 remain unresolved.
This report grants neither final change PASS nor completion.

## Subject and authority

Original integrated base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`.
Current HEAD / prior reviewed target:
`462ebee97c81772103054f7d090053ac7054bafa`.
No replacement target commit exists. Independently inspected `git diff` against
HEAD contains exactly two source files, +27/-9, with these current Git blobs:

| File | Inspected blob |
|---|---|
| `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` | `5d87248722ecaac30fe4a0fa2bd07df5ed306176` |
| `crates/wyrd/wyrd-server/src/components/storage/routes.rs` | `0ecab2483e0dcdf9cab16ed0a317938e0950af79` |

This reviewer is independent of implementation and integration. Scope is the
revised [R1 packet](../change-462ebee97/TASK-CHANGE-R1-review-gates.md):
FIND-CHANGE-001, storage/MCP subset of 003, and MCP subset of 004. The prior
[full review](../change-462ebee97/change-review.md), its verification record,
approved storage specification revision 2, original TASK-001/002/003, and
delivery context remain the unchanged whole-change record.

Read applicable root AGENTS, change-review skill, spec-driven workflow, Rust
and agent-surface/maintainer guides, TESTING fixture/lane guidance, Wyrd File
upload/authorization authority, Bifrost inventory/queued-input authority, and
security principles. No CodeGraph index or nested AGENTS governs these changed
paths. Actual changed handler bodies, argument types, MCP test, verification
admission, fixture settlement, server config composition and reservation owner
were inspected directly. The original base-to-target audit is carried forward;
this bounded pass does not claim to repeat it.

## Closure matrix

| Obligation | Independent source evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-CHANGE-001: instrument all three affected handlers with scrubbed arguments | `routes.rs:161` part_url skips state/caller and records operation; remaining values are String upload ID and PartUrlQuery containing only u32 part_number. `:339` abort also skips HeaderMap. `:408` local_blob skips Body and records only its path extraction result in addition to operation. | Direct full-body/type/diff inspection; recorded implementation lints PASS. | Source PASS |
| Preserve storage validation, authorization, tenant/audit and response contracts | The routes diff adds only three attributes. All persisted-owner authorization, idempotency extraction, body bounds, local byte validation, service calls and responses are unchanged. Adjacent complete/download_init establish the reused pattern. | Exact source diff and owning File/security authority. | Source PASS |
| FIND-CHANGE-003 storage/MCP subset: document first DOCUMENT fixture without changing it | `verification.rs:1224` adds one purpose rustdoc to the same byte-slice constant. Literal `br#"{"answer":1}"#`, declaration and all upload/digest/assertion code remain unchanged. | Direct diff inspection; no runtime proof needed for the comment. | Source PASS |
| FIND-CHANGE-004 MCP subset: ordinary setup succeeds before refusal injection | `staging_pressure_is_retryable` now starts the ordinary server; Card registration, service credential, MCP connection and discovery precede flush and reservation. `flush_bifrost` begins with settle_bifrost_outbox. No one-item startup override remains. | Fresh exact journey PASS, one selected test, 2.667 seconds; both Postgres migration checks PASS. | PASS on inspected blobs |
| Use actual runtime-owned capacity/share rather than guessed occupancy | Test reads WyrdServerConfig::default().bifrost.scribe.outbox_capacity and RESERVED_SHARE. `server.rs:5016–5023` builds this fixture from the same default and overrides capacity only for an explicitly supplied builder option, absent here. `boot/mod.rs:889–902` passes that configured capacity to the process outbox; `outbox.rs:437–446` uses the identical share formula. | Independent config/boot/reservation source trace. | Source PASS |
| Hold refusal through both calls and release before readback | One unspent Reservation covers both EXECUTE and ENQUEUE. Direct verification reserves two Scribe slots; enqueue reserves one. Whole-share reservation therefore refuses both at the intended owner. Explicit drop precedes final flush, SQL query and client cancellation; unwinding/early return also releases by Reservation::drop. | Direct test/admission/Drop inspection; exact journey evidence as above. | Source PASS |
| Preserve stable public error, retry hint and no-input proof | Both tool names/discovery assertions, stable `WYRD_VERIFICATION_429_STAGING_FULL` check, `retry_after_seconds` check and exact SQL result `[[0]]` are unchanged. Existing staging_full maps failed reservation to the catalog refusal with a one-second retry hint. | Direct before/after assertion comparison and mapper inspection. | Source PASS |
| Keep correction local and within R1 | Two source paths only; no production queue/config/harness behavior, new dependency, worker, retry, wait or recovery machinery. | Exact diff and owner trace. | PASS |
| Immutable subject required by change-review skill | Working-tree blobs identify inspected bytes but HEAD still identifies the prior failing candidate. | git rev-parse and git hash-object. | BLOCKED for formal closure |

The initial flush is stronger than an outbox settle but does not invalidate this
small refusal-contract proof: no WAL, freeze, publication-age or recovery state
is under test. It occurs before the reservation and no verification input is
accepted. Existing lower-level/HTTP recovery proof remains with its owner.

The reservation `expect` asserts the settled fixture admits its whole share.
Outbox settlement observes pending writes, not a global fence against future
reservations. That distinction is real, but no concrete competing reserved
producer in this fixture or failing execution is established. Ordinary audit
and telemetry stage through the independent unreserved share. Holding an
unspent Reservation allocates no corresponding population of queued records.
The possibility of future ambient reserved work is an evidence limit, not a
retained finding or justification for new waiting/control machinery.

## Stable finding dispositions and whole-change acceptance

| Stable finding | Disposition in this pass |
|---|---|
| FIND-CHANGE-001 | Bounded source correction accepted provisionally; formal binding awaits a new immutable target. |
| FIND-CHANGE-002 | Open, unchanged; R2 exact restart accounting. |
| FIND-CHANGE-003 | Storage/MCP fixture subset accepted provisionally; delivered Scribe panic-contract subset remains open in R2. |
| FIND-CHANGE-004 | MCP refusal-contract subset accepted provisionally; inherited Rust/Python/TypeScript fixture subset remains open in R3. |
| FIND-CHANGE-005 | Open, unchanged; R2 configured publication-budget proof. |
| FIND-CHANGE-006 | Open, unchanged; R3 producer settlement and retained observers. |
| FIND-CHANGE-007 | Open, unchanged; R2 failed-replay snapshot/shutdown boundary. |
| FIND-CHANGE-008 | Open, unchanged; R3 canonical custom-verdict payload reader. |
| FIND-CHANGE-009 | Open, unchanged; R3 exact group-inclusive input population. |
| FIND-CHANGE-010 | Open, unchanged; R3 source/read-shape observation boundary. |
| FIND-CHANGE-011 | Open, unchanged; R3 post-exit PTY observer. |

The prior acceptance matrix remains the whole-change matrix. The following
rows explicitly carry its source-backed conclusions forward without claiming
fresh full-suite execution:

| Obligations | Bounded effect / carried evidence | Result |
|---|---|---|
| REQ-001–014, REQ-017–018; INV-001, INV-003–005; AC-001–006, AC-008 | Prior implementation and I1/I2/I3 evidence unchanged. Only handler attributes, one fixture comment and a local MCP refusal fixture changed. | Prior source acceptance retained, with prior proof limits |
| REQ-015–016; INV-002; AC-007 | Handler permission/tenant/response bodies and MCP error assertions unchanged; current inspection above confirms scrubbing and preserved public contract. | Source acceptance retained |
| Constraints: Artifact authoring and canonical benchmark plumbing; bounded requests/retries and nonblocking audit | No runtime lifecycle or producer behavior changed. | Prior source acceptance retained |
| Constraints/non-goals: fresh install; no migration/backfill, checksum certification/promotion, second inventory/worker/outbox, repair scan, retention/GC/dedup, general cache, visibility checkpoint or queue redesign | No such additions in the inspected diff. | Retained |
| Maintainer exclusion of benchmark measurement/remediation | No timing work or performance claim in this review. R2 remains correctness/docs work only. | Retained |
| Repository gates outside R1 | Prior red aggregate, inherited findings and unavailable/unexecuted proof remain as recorded. | FAIL / unresolved |

Original task-review correspondence stays as recorded: TASK-001
`5e7dec69732ba6d4d05399aeffd6f4d9c683745d`, TASK-002
`793912475795951d440b8d072c99a9957ad18464`, TASK-003
`57219dee3713e817ace6deac5e91ffc673a5a811`. This pass evaluates the bounded
TASK-002 seam correction; it does not manufacture replacement task verdicts.

## Verification and next handoff

Fresh root-owned sequential verification is recorded in
[verification.md](verification.md): exact MCP journey PASS, one selected test
in 2.667 seconds, 33 excluded, exit 0; both repository Postgres migration
checks PASS; fmt:check PASS; git diff --check PASS. HEAD and both inspected
source blobs remained unchanged before/after verification. These results
exercise both tool refusals and the exact zero-input readback on the inspected
source. The implementation packet's lints PASS is carried forward explicitly
as implementation evidence and was not rerun. This reviewer ran no Cargo
command or test and did not edit source, stage, commit, update a PR or alter
prior reports.

No full aggregate or language suites were repeated. Prior real-GCS multipart,
provider-emulator File-sweeper abort and combined queued delayed-ACK proof
limits remain explicit; prior unexecuted installed-CLI commands and aggregate
failure are not erased. Timing remains unmeasured under maintainer direction.

Recommendation: accept the inspected R1 source corrections with the fresh
focused verification, retain R2/R3 unchanged, and bind eventual formal closure to a
new immutable candidate containing the reviewed bytes and actual evidence.
No new remediation packet is warranted from this bounded source inspection.
Do not run wyrd-complete or delete the active packet.
