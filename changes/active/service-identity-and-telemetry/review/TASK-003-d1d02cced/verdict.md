# TASK-003 — FIX_REQUIRED

Six independently validated implementation or mandatory-gate failures remain. Review did not modify source, implement fixes, commit, push or deploy.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-3`.
- Base: `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b`.
- Candidate: `d1d02cced07582a8f817d3f5651e7895b96d046d` (`schema-change-task-3`).
- First review of the complete `git diff ca2ac4a37...d1d02cced`: 24 commits, 328 files, +7144/-2909. Base is the merge base. No TASK-003 prior acceptance or finding closure carries forward.
- Approved specification: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, including normative authentication, telemetry-schema and storage/correlation supplements.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md`.
- Stronger authority: root and all four applicable nested AGENTS files; architecture authority map and owning Wyrd identity/public surface, Bifrost ingest/schema/durability, security and operations sections; applicable agent guides; spec-driven workflow and TESTING.md.
- No `.codegraph/` directory exists. Navigation and reviewer scopes are recorded in `subject.md`.

The task's proposed frontmatter is not itself evidence of approval or completion. Acceptance here derives from the approved revision-16 specification, stronger authority, actual source and verification evidence.

## Reviewer selection, coverage and results

Each discovery reviewer was fresh, independent and given the same starting navigation map plus its scoped responsibility. They inspected the actual committed range and governing authority, not another reviewer's conclusions. Source remained immutable. The orchestrator routed and reconciled claims; a separate fresh validator performed source/materiality/correction validation.

| Role | Selection reason / coverage | Report | Result |
|---|---|---|---|
| behavior-rev | Complete seven-scenario acceptance, supported caller/result and negative paths | `task-review-behavior.md` | FAIL: BEH-001/002 |
| invariant-rev | Complex shared row identity, schema/projection and producer-to-sink origins | `task-review-invariants.md` | FAIL: INV-R1/R2 |
| repo-rev | Broad Rust/Python/TS/SQL/docs/tooling rules and mandatory private/test rustdoc | `standards-review.md` | FAIL: REPO-003-001 |
| maintainer-rev | Material attribution, context and descriptor ownership/reuse | `maintainer-review.md` | FAIL: MR-1/2 |
| domain-rev-security | Public/native/OTLP/gateway trust, delegation/graph/tenant and provider-header boundaries | `domain-review-security.md` | FAIL: SEC-001 |
| domain-rev-persistent-data | Persisted ledgers/layouts and cache/durable native retry identity | `domain-review-persistent-data.md` | FAIL: DATA-001 |
| system-rev | Outbox/local/peer/retry/recovery/publication and gateway settlement | `system-review.md` | FAIL: SYS-001 |
| Fresh ponytail-rev | Independent validation of all proposals, unique claims, full correction feasibility and coverage | `findings-validation.md` | FIX_REQUIRED: six deduplicated causes |

Reports compare new or changed machinery with existing owners. Shared CardRegistry, table projectors, Scribe stamping/fence owners and gateway/verification acceptance remain the appropriate owners; no validated proposal requires a new graph, writer, public API or persistent-data policy.

## Claim reconciliation and follow-up

`claim-comparison.md` groups proposals by cause. Multiple reports corroborate gateway span and native dimension gaps but do not establish truth by vote. The validator separately traced each path and also validated the unique delegation, native replay and deleted-descriptor findings.

No followup-rev was required: correction alternatives resolved from approved authority and actual source, and the validator retained no newly discovered independent blocker. Trusted Rows validation feasibility is part of the existing gateway finding's complete correction, not an additional finding. One unsupported documentation panic entry (`published.rs::prove_late_failures`) was rejected and omitted from remediation. No rejected proposal creates optional advice or work.

The behavior report's broad native replay/root acceptance is narrowed by the domain proofs. The security report's managed/shape refusal acceptance does not establish drift shared-dimension refusal. Successful descriptor lookup with immutable UIDs does not establish cold delivery after deletion. Correct persistence preserves physical rows but cannot repair missing producer context.

## Reconciled acceptance matrix

PASS indicates supported source-backed acceptance plus inspected/recorded proof for the bounded obligation; it does not claim tests were rerun during review.

| Original obligation | Implementation and verification evidence | Result |
|---|---|---|
| Scenario 1: one public bifrost catalog, matching grants/consumers, no alias | Shared namespace/TableRef/permission/catalog/SQL owners and SDK/MCP/CLI consumers; recorded catalog parser, server query/grant, MCP and CLI proof; behavior/data/standards coverage | PASS |
| Scenario 2: seven managed fields, nine systems with six dimensions once, dynamic envelope only, exact types/default ledger | Managed/table declarations, shared layout and nine ledger checks; recorded exact schema/describe/layout/round-trip proof; behavior/invariant/data coverage | PASS for behavior; documentation gate separately fails |
| Scenario 3: exact promotions, raw/presence/nested data, integer/time precision and bounded output | Existing typed final-key extractors/projectors and canonical validators; recorded maximal signals/resource/OTel proof | PASS for inspected projection behavior |
| Scenario 4: public pre-ACK dimension refusal | Non-signal native drift follows shape-only validation and retains supplied canonical dimensions | FAIL: FIND-TASK-003-2 |
| Scenario 4: exact accepted execution ownership on all ingress | Gate descriptor/hash and Scribe stamping read raw subject root, bypassing current actor selection | FAIL: FIND-TASK-003-3 |
| Scenario 4: identical retries accepted, changed accepted origin refused | Native dynamic rows can change only managed context, which logical cache/durable identity excludes | FAIL: FIND-TASK-003-4 |
| Scenario 5: original trusted account/workload/subject/Run/environment/request through batching/peers | Existing supplied-origin verification/audit/peer propagation accepted, but gateway attempt producer loses origin and cold descriptor restoration can terminally drop accepted work | FAIL: FIND-TASK-003-1/5 |
| Scenario 6: exact stored rows across hot/recovered/published/compacted reads; unchanged evidence predicates | Existing lifecycle and reader source plus recorded full-row/aggregate lifecycle comparison; no query inference or evidence narrowing | PASS for physical-row lifecycle, subject to correcting producer/admission defects |
| Scenario 7: per-request gateway admission, Batch caller context, provider credential separation | Existing request parsing/admission/calls projection and typed provider dispatch; recorded configured gateway/Python/TS calls readback | PASS for admitted calls/provider boundary |
| Scenario 7: complete gateway capture/provider attempts and descriptor lifetime | Calls and attempt projections consume different context; live visibility read cannot restore a deleted accepted subject | FAIL: FIND-TASK-003-1/5 |
| Static/public contracts, thin SDKs, authority updates and non-goals | Source-backed all-language/generator/boundary coverage and recorded checks; no new public ownership engine, alias or writer required | PASS for inspected boundary closure, except mandatory rustdoc |
| Mandatory changed-Rust-item documentation | Corrected validated inventory: 141 missing-documentation/section entries over 118 unique changed items | FAIL: FIND-TASK-003-6 |

No non-goal is relaxed by the correction. Fresh public membership, tenant/graph authorization, signal source validation, exact version/binding ownership and nonblocking audit must remain intact.

## Independently validated ledger

The complete source proof, authority, materiality, correction burden, revised correction boundary, sibling callers and closure proof for each entry are in `findings-validation.md`; the remediation task reproduces the diagnoses and required corrections self-contained.

| Stable finding | Status | Classification | Exact primary location | Material consequence / selected correction |
|---|---|---|---|---|
| FIND-TASK-003-1 | REVISED | MISSING | `wyrd-server/src/components/gateway/capture.rs:403,438,482`; `scribe_outbox.rs:1089`; Redux `scribe/execution_lanes.rs:564,945` | Provider attempts lose accepted origin; carry the capture's context per attempt and close trusted Rows validation without weakening public/native validation. |
| FIND-TASK-003-2 | REVISED | INCORRECT | Redux `tables/drift/observations.rs:26`, `tables/mod.rs:388`, `scribe/execution_lanes.rs:564,1240` | Public native drift can forge Service/environment dimensions; enforce null placeholders at public non-signal admission, retaining trusted Rows/resource fallback/dynamic fields. |
| FIND-TASK-003-3 | REVISED | INCORRECT | Redux `gate/attribution.rs:90`, `gate/mod.rs:992`, `scribe/ingress.rs:728` | Delegated rows choose wrong/null Service and may collapse distinct release exports; select accepted owner once with existing owning_root and carry it to all consumers. |
| FIND-TASK-003-4 | REVISED | INCORRECT | Redux `scribe/preprocess.rs:500,563,629,916`; `scribe/shards.rs:1138`; Vala SQL batch commits `:123` | Native explicit-ID reuse accepts changed stable origin; bind managed stable correlations in the existing logical identity and matching reuse predicate, excluding volatile request/admission time. |
| FIND-TASK-003-5 | CONFIRMED | REGRESSION | Redux `gate/attribution.rs:113,140,179`; server `scribe_outbox.rs:490` | A cold resolver loses accepted evidence after subject deletion; use retained immutable descriptor retrieval for trusted delivery while public membership remains live-only. |
| FIND-TASK-003-6 | REVISED | VIOLATION | Corrected inventory in validation/remediation; representative Redux `tables/signal.rs:508`, server `grpc/capture_peer.rs:102` | Explicit rustdoc merge gate fails; add accurate docs/Errors/Panics only to affected changed items alongside the functional fixes. |

## Shared ownership model and root cause

The stable Service account owns credentials and grants. The exact registered Service release UID owns execution attribution (`workload_card_uid`). The observed exact Card is independently `subject_card_uid`; subject/actor principals remain accounts. One credential may support S1 and S2 releases after valid selection, but an accepted S1 action never becomes S2 during projection, delivery, retry or credential refresh.

Current code permits downstream consumers to reinterpret verified authentication fields instead of consuming accepted ownership, and a gateway sibling projection omits that ownership entirely. The existing shared `owning_root` rule already distinguishes direct and delegated execution. The correction resolves once at the existing admission owners and carries values, preserving exact verification binding ownership and accepted queued authority. Hashing, descriptor projection and storage do not select owners again. Descriptor restoration is a projection of an accepted UID, not a fresh ownership decision.

The user explicitly required a single shared ownership flow across Bifrost/gateway/verification during review; this is recorded separately from independently discovered findings. It is consistent with approved semantics and does not receive a retroactive FIND ID. Extend current accepted representations and existing shared selection rather than introducing a general-purpose context or parallel engine. Source task `TASK-003-R1-preserve-accepted-origin.md` includes this invariant and the exact-UID example.

## Verification limits and next round

All discovery and independent validation were static. The original task records green focused journeys, format/lint, codegen, dependency/tenant checks and SDK/docs gates; reviewers inspected source and proof but did not independently rerun those product commands. Review independently ran immutable Git identity/status/diff checks and `git diff --check ca2ac4a37...d1d02cced` successfully. No Cargo/test/codegen process was started. Reports distinguish these provenance classes.

No prior-finding closure or convergence assessment applies: this is the first TASK-003 review, not an unsuccessful remediation round. The next review checks these six IDs, the remediation delta, the shared accepted-owner flow and affected callers/invariants. Unchanged accepted catalog/schema/layout/lifecycle evidence carries forward unless the fix or new evidence invalidates it. Broad user-journey suites belong to final integrated change review; remediation requires only focused gap proof and the narrowest write-set checks.

A bounded independent comprehension reader read the approved specification, original task and new remediation together without implementation or other review conclusions. It found no material gap or contradiction: the account/exact-Service/observed-Card distinctions, one accepted-owner flow, six focused proofs and preserved boundaries were understandable without another material design decision. This adds no approval gate or source verification claim.

**Verdict: FIX_REQUIRED.**

**Remediation:** `TASK-003-R1-preserve-accepted-origin.md`, directly to `$wyrd-implement`.
