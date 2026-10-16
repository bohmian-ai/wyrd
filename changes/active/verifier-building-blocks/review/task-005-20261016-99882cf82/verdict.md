# TASK-005 review verdict

**FIX_REQUIRED**

TASK-005 does not pass its independent completion gate. Approved specification revision 9 resolves the local evidence route approval stop. Nine stable finding IDs remain as bounded corrections; FIND-TASK-005-4 and FIND-TASK-005-6 share the same evidence-admission correction. Review does not implement those corrections or mark the task complete.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-verifier-building-blocks`.
- Branch: `change/verifier-building-blocks-task-005`.
- Base, exclusive: `f2a726c90edbf8956c7b91c843379175ee20eac0`.
- Candidate: `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`.
- Approved spec: `changes/active/verifier-building-blocks/spec.md`, revision 9. The original r8 review authority remains frozen in [spec-snapshot.md](spec-snapshot.md); r9 is the current approved authority for this disposition.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-005-server-integration.md`; frozen in [task-snapshot.md](task-snapshot.md).
- Actual cumulative diff: [cumulative.diff](cumulative.diff); inventory: [changed-files.txt](changed-files.txt).
- Initial review: no prior TASK-005 verdict or finding IDs to close. Prior tasks' reviews are not rewritten.
- The initial review kept HEAD and tracked source unchanged. During this artifact completion, HEAD is `d88387d79fad063be856b14947b4385b78f422e7`; its sole post-candidate change is original-task external-example evidence wording. Production and test source are identical to the reviewed candidate. The owner supplied r9 as a working-tree spec change. This completion changes only this verdict and the new remediation task; no production code, tests, specification, original task/status, commits, merges, pushes or deployments were changed.

## Owner clarification

The owner stated during review: **“remember - nothing has shipped. no migrations needed”**. [user-clarification.md](user-clarification.md) records this controlling constraint. Existing-deployment upgrades, historical catalog evolution, and pending legacy dispatch migrations are excluded. DATA-001 is rejected. No migration machinery or migration proof is requested.

## Spec revision 9 disposition

The owner-approved r9 REQ-010, REQ-012 route table and revision history approve
`PUT /v1/verification/evidence/local/{sha256}`. The public-route decision is
closed and is not reopened. A fresh independent supplemental validator checked
the unchanged candidate owners against that approval; the original reports and
`findings-validation.md` remain preserved as the r8 review record.

FIND-TASK-005-6 is **REVISED: bounded authorization correction remains**.
`components/verification/service.rs:399-406::upload_local_evidence` still asks
for `Permission::verifier_run_any()` (scope All), as POST evidence does at
`:323-329`. This rejects an exact-Verifier run grant and conflicts with r9's
operation-level admission. Reuse `PermissionSet::covers_operation(Resource::Verifier,
Action::Run)` and existing nonblocking allow/deny audit in VerificationControl,
as already used for MCP coarse admission at `mcp/verification.rs:133-142`.
Consolidate this with FIND-TASK-005-4, preserving both IDs and exact selected-UID
checks on execute/enqueue.

No additional OpenAPI correction was found: `routes.rs:38-49` mounts and
publishes the route only for local storage through the same registration;
`:162-184` publishes the binary PUT, digest path, 204 and refusals, and
`http/openapi.rs:18-52` provides the global token scheme without OAuth scopes.
Caller tenant (`service.rs:415-420`), bounded body (`:399-414`), digest-before-write
and metadata-free tenant key (`wyrd-storage/src/service.rs:546-570`), and atomic
same-digest replacement (`wyrd-storage/src/local.rs:57-85`) match r9. Keep those
behaviors and remote single-PUT plans. Align the existing Wyrd design route
inventory/file-transfer prose to this approved local transfer in the same
remediation documentation batch.

The other eight validated findings retain their diagnoses and IDs. DATA-001
and BEH-04 stay rejected; nothing has shipped and no migrations are required.
One self-contained [TASK-005-R1](TASK-005-R1-verification-corrections.md) packages
all bounded corrections for `$wyrd-implement`. No runtime tests were run for
this artifact-only completion; its focused commands are required implementation
proof, not claimed results.

## Independent review record

All reviewers received the same immutable subject and starting navigation map, expanded it independently against source, and owned separate reports. Discovery reviewers received no peer conclusions or intended verdict. A fresh focused reviewer resolved one correction conflict, followed by a fresh independent validator.

| Role | Report | Discovery result |
|---|---|---|
| Caller-to-result implementation | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| Producer-to-sink implementation | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| Repository standards | [standards-review.md](standards-review.md) | FAIL |
| Maintainer specialist | [maintainer-review.md](maintainer-review.md) | FAIL |
| System resilience | [system-review.md](system-review.md) | FAIL |
| Security/RBAC/tenancy domain | [domain-review-security-tenancy.md](domain-review-security-tenancy.md) | FAIL |
| Persistent data/durability domain | [domain-review-data-durability.md](domain-review-data-durability.md) | FAIL; upgrade proposal rejected under owner clarification |
| Concurrency/lifecycle domain | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL |
| Evidence/evaluation domain | [domain-review-evidence.md](domain-review-evidence.md) | PASS |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED |
| Structured Ponytail validation | [findings-validation.md](findings-validation.md) | Original r8: SPEC_REVISION_REQUIRED; r9 supplemental disposition above revises FIND-6 to bounded authorization repair |

Discovery agreement was not treated as proof. The orchestrator groups and records independently validated results; source validation and correction selection belong to the fresh validator.

## Follow-up decision

[claims-comparison.md](claims-comparison.md) records all grouped claims. Reports disagreed on whether missing signed subject scope should be checked synchronously using signed UIDs or asynchronously after registry resolution. One fresh focused follow-up traced minting, signed serialization, delegation, accepted authority, siblings, and consumers. It resolved the boundary to existing in-memory signed UID admission in shared enqueue before staging; no registry read or new asynchronous authorization contract is necessary.

The historical pending-Operator uncertainty no longer required follow-up after the owner's unshipped clarification. Other unique findings and narrower happy-path PASS assessments went to mandatory independent validation rather than additional reviewer votes.

## Reconciled acceptance matrix

PASS below describes inspected source and supplied proof for the named obligation; it does not waive a related retained finding.

| Requirement, criterion, invariant or constraint | Implementation and verification evidence | Result |
|---|---|---|
| REQ-001–003 declarative contract, graph/reference/registration closure | Existing contract/validation and reference owners; registration/lineage and generated contract proof inspected in implementation/domain reports | PASS |
| REQ-004–006 preparation, checks, missing evidence and aggregation | Shared Vala block engine and real three-language action/check journeys; optional scheduled absence bypasses normal missing-slot behavior | FAIL FIND-3; remaining inspected behavior PASS |
| REQ-007–009 shared Agent/Prompt/tool ownership and original-source citations | HostedBlocks composes existing AgentExecutor, registry, evidence, gateway and Vala engine; domain evidence review validates pinned baseline and citation identity | PASS for inspected paths; resource pressure separately fails FIND-2 |
| REQ-010/011 equal File and JSON entry workflows | Shared RunEvidence pinning precedes invocation; evidence endpoints demand All despite exact invocation grants | FAIL FIND-4 |
| REQ-012 approved HTTP/MCP operations and exact public boundary | r9 approves local PUT; conditional publication matches; evidence admission incorrectly requires All | FAIL FIND-4/6; public-route approval closed |
| REQ-012/015 real result and separate execution/result identity | Normal result projection is complete; retry after staged result/failed settlement can return abandoned attempt outcome | FAIL FIND-5 |
| REQ-013 async staging, ACK-only queue creation, replay/conflict and no request PG writes | Existing Scribe outbox → submission outbox → transactional queue acceptance; source and supplied real SQL/route proof | PASS; signed subject admission separately fails FIND-1 |
| REQ-013 fenced recovery and lifecycle | Existing claim/renew/release/shutdown owners retained; per-attempt result selection loses fencing meaning on recovered result reads | FAIL FIND-5; other inspected lifecycle paths PASS |
| REQ-014 explicit scheduled slots, baselines and existing statistical algorithms | Exact requirement-specific fitted state and mapped scoring retained; optional omitted selectors ignored by report producer | FAIL FIND-3; mapped PSI/SPC/Custom paths PASS |
| REQ-014 failed-only reactions, frozen context and independent delivery | Existing queue/worker/renderer and execution dispatch view retained; supplied scheduled Operator journeys and delivery tests | PASS normal paths; recovered selected-result identity covered by FIND-5 |
| REQ-015 canonical protected analytical surfaces, timing/correlation and attribution | Named column mapping, reserved Gate destinations, application Run/execution/binding projection retained | PASS current-install shape; FIND-1/3/5 affect correctness of produced evidence |
| REQ-016 matched runnable examples and external repositories | Three-language deterministic real-server journeys supplied; clean external source heads/archive captured | PASS exercised deterministic paths, with build/live limits below |
| REQ-017 current architecture, docs and consumers | Live contract consumers migrated; normative schema/metric/activation descriptions remain inconsistent | FAIL FIND-8 |
| INV-001/002 server ownership and one shared client | Existing control, engine, queue, storage and SDK owners reused; no alternate graph executor or transport introduced | PASS; scoped File behavior FIND-4 |
| INV-003 signed scope, tenant/delegated authority and exact grants | RLS/effective caller retained; manual queue drops signed subject scope and generic file staging excludes exact grant | FAIL FIND-1/4 |
| INV-004 shared nonblocking audit and approved loss guarantees | Existing Scribe staging retained; no new durable result writer demanded | PASS inspected architecture; accurate scope denial required by FIND-1 |
| INV-005/007 bounded runtime and capacity | Startup parsing/shared pool/worker packaging present; real saturation is converted to terminal evidence and workspace waiters accumulate | FAIL FIND-2 |
| INV-006 honest missing/recovered evidence | Optional absent scheduled slots can be scored; abandoned result can satisfy a completed handle | FAIL FIND-3/5 |
| INV-008 final cutover without duplicate retired workflow | Old live contracts/judge removed as required; copied trace-wait API survives with only its own test | FAIL FIND-9; scenario preservation PASS |
| Mandatory changed-item Rust documentation | Concrete omitted item docs/panic contracts independently verified across Rust helpers/tests/associated items | FAIL FIND-7 |
| AC-001–004 contract/engine/hosted isolated and integrated proof | Supplied gate and source tests provide credible exercised-path evidence; do not prove real capacity deferral | FAIL overall due FIND-2/3; no demand for new isolation mechanism |
| AC-005–007 real SDK/status/negative/queued/drift paths | Supplied three-language gate journeys credible; scoped File, scope denial, saturation, omitted slot and recovered result switches remain gaps | FAIL FIND-1–5 |
| AC-008 deterministic examples | Supplied external tests and support-desk local fake-provider journeys; live mains explicitly not counted | PASS within recorded proof limits |
| AC-009 schemas, runtime MCP/OpenAPI, current authoritative documentation | Supplied generated/MCP/served OpenAPI proof; r9 accepts route inventory; evidence authorization and normative descriptions remain incorrect | FAIL FIND-4/6/8 |
| AC-010 canonical gates | Full gate recorded successful; cumulative diff whitespace check passed; tests cannot approve new scope or missing source behavior | Supplied verification PASS; acceptance remains blocked |
| No migration/deployed-history requirements | Owner's explicit unshipped constraint | Not required; DATA-001 rejected |
| No adjacent benchmark redesign | BEH-04 relies on superseded older direct-drift/judge qualification meaning; current task does not require replacement qualification design | Not required; BEH-04 rejected |

## Validated finding ledger

The full original source proof, discovery dispositions, correction boundaries and closure scenarios are in [findings-validation.md](findings-validation.md). The r9 disposition above supersedes only its r8 approval stop for FIND-6. This table contains only independently CONFIRMED or REVISED findings.

| Stable ID | Validation / classification | Finding and selected correction boundary |
|---|---|---|
| FIND-TASK-005-1 | REVISED; VIOLATION / REGRESSION | Enqueue loses signed subject scope. Restore shared in-memory signed UID admission and denied audit before staging; preserve direct/scheduled behavior and async acceptance decisions. |
| FIND-TASK-005-2 | REVISED; INCORRECT | Runtime pressure becomes terminal evidence; workspace waiters are unbounded. Preserve distinguishable admission refusal through existing hosted consumers and reuse fenced queue deferral/refunded attempts; bound existing workspace admission. |
| FIND-TASK-005-3 | CONFIRMED; INCORRECT | Unmapped optional scheduled samples are inferred from retained rows. Respect frozen mapped slots at DriftEngine report production and reuse missing-evidence inconclusive semantics. |
| FIND-TASK-005-4 | REVISED; INCORRECT | Exact-Verifier grant cannot prepare File evidence. Reuse existing operation-level run-grant admission for generic staging; keep exact invocation and tenant/digest/size checks. |
| FIND-TASK-005-5 | CONFIRMED; REGRESSION | Reclaimed attempts alias result identity. Mint one immutable result ID per produced payload and retain the queue-selected pointer; keep execution correlation, retries and approved precommit staging/loss behavior. |
| FIND-TASK-005-6 | REVISED under r9; INCORRECT | Route approval is closed; local PUT still demands All instead of operation-level verifier:run. Share FIND-4's VerificationControl admission correction; preserve approved route, publication and storage behavior. |
| FIND-TASK-005-7 | REVISED; VIOLATION | Mandatory rustdoc missing on concrete changed items. Batch substantive in-place docs/panic-contract corrections with bounded work; no implementation or unrelated helper rewrite. |
| FIND-TASK-005-8 | CONFIRMED; INCORRECT | Normative schema, input time/layout, metric labels and activation/read descriptions are stale. Align existing authority prose to approved current behavior. |
| FIND-TASK-005-9 | CONFIRMED; DRIFT, deletion-only | Retired trace-wait SQL API is used only by its own test. Delete the constant/type/method/dedicated test/import; preserve active retry/release/fencing and current trace semantics. |

Rejected proposals DATA-001 and BEH-04 are excluded from remediation. The equal-millisecond trace concern has no established parent-first contract and is not a finding. Live provider absence, unpushed source commits, and the unrelated reported Oracle flake are evidence limits, not additional findings. Human clarification is separate authority, not a retroactively assigned FIND ID.

## Verification and limits

See [verification-evidence.md](verification-evidence.md).

- Supplied `mise run gate` exit 0 at `03712af5e`, 2550 seconds, includes the recorded Rust/server/SDK journeys, Python 278 passed / 2 skipped, TS 31/31 integration files and generation/docs/boundary checks. Final candidate commit only records task evidence; no production/test changes followed that gate candidate.
- External source identities: TypeScript `0e45152b5ad0bf33f6d6518c305f5d8868eb7e71`; Python `a9a6327b9027223a128d53f64360d1d9bc7c0555`; Rust `f7647cf3fc6f63b102695a08d22ee73aef9c0b04`. All source trees clean at capture. Recorded tests: TS 5/5 plus typecheck, Python 5/5, Rust 1/1. [external-identities.json](external-identities.json) and [external-source.tar.gz](external-source.tar.gz) preserve source; ignored local dependency builds are not reproduced or hashed by this review.
- Review ran `git diff --check f2a726c90..99882cf82` successfully and repeatedly checked HEAD/tracked-source immutability. No broad gate, runtime reproduction, live provider, live cloud storage, benchmark, image build or external build rerun was performed. Named test references in discovery describe inspected source or supplied evidence, not new executions.
- Controlled focused closure scenarios in the validated ledger are recommendations, not claims that new tests already ran. Do not prescribe a full gate/full journey battery for a bounded repair; final integrated change review owns that proof.

## Disposition

**FIX_REQUIRED**. Revision 9 closes the public-API approval decision; the remaining
local upload authorization mismatch shares FIND-TASK-005-4's bounded correction.
All nine FIND IDs are retained in
[TASK-005-R1-verification-corrections.md](TASK-005-R1-verification-corrections.md),
which routes directly to `$wyrd-implement` with diagnoses, existing owners,
preserved behavior, acceptance criteria and focused proof. No production or
test changes or task status updates were made. A subsequent review must audit
the complete cumulative candidate against the original task and r9, preserving
these IDs and proving closure from source.

Skipped runtime/live-provider/build reruns. The retained bounded findings prevent
task completion; the API approval itself is no longer a blocker.
