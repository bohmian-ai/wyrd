# TASK-003 R1 — FIX_REQUIRED

FIND-TASK-003-1 through -5 are closed. FIND-TASK-003-6 remains only for one materially changed nested test helper missing its own required rustdoc/Panics documentation. No new behavior finding is retained.

## Immutable subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-3`.
- Approved spec: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, plus its three normative supplements.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md`.
- Original base: `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b`.
- Prior reviewed candidate: `d1d02cced07582a8f817d3f5651e7895b96d046d`.
- Record-only commit / corrective implementation base: `89449aa39be209b0fe34e2cfbdef87110d6f5ce8`.
- Candidate: `ee970fbbef1511607a71c89ac16802b34724d422`.
- Remediation under review: sibling `TASK-003-d1d02cced/TASK-003-R1-preserve-accepted-origin.md`, including committed evidence.
- Actual review delta: `git diff d1d02cced ee970fbbe`, distinguishing the prior review records from six corrective commits `89449aa39..ee970fbbe`. Cumulative context remains `ca2ac4a37...ee970fbbe`. Ancestry is unchanged.

This is the first remediation review, scoped to six finding closures, the corrective delta and affected callers/consumers/invariants/tests. The prior verdict, final ledger and selected reports remain acceptance context. Root/applicable nested SQL rules, architecture owner sections, spec-driven workflow, maintainer/Rust guides and TESTING govern the review. No CodeGraph directory exists. HEAD matched candidate and source was clean at review start; review added only this report directory.

## Reviewer selection and independent results

| Selected reviewer | Reason / scoped coverage | Report | Result |
|---|---|---|---|
| Fresh remediation ponytail closure reviewer | All six closures, changed code and callers, shared ownership/reuse, mandatory original/new-item docs and proof diagnoses | `remediation-review.md` | FAIL only residual FIND-6 |
| Domain security specialist | Material public Frame/private Rows validation, accepted delegated owner and live/retained lookup boundary changes | `domain-review-security.md` | PASS; FIND-1/2/3/5 closed |
| Domain persistent-data specialist | Material logical digest/reuse and warm/durable retry identity changes, affected recovery consumers and fixture diagnosis | `domain-review-persistent-data.md` | PASS; FIND-4 closed |
| Separate fresh ponytail validator | Independently inspect closure, source, materiality/correction burden and reconciled coverage | `findings-validation.md` | FIX_REQUIRED only FIND-6 |

Each reviewer was fresh relative to implementation and received the same starting map without other current discovery conclusions. The original full team was not recreated. A separate repo/maintainer/system reviewer was unnecessary: bounded docs/reuse/recovery coverage is in the closure review, with unchanged original specialist evidence carried forward. All selected reports are present.

`claim-comparison.md` groups closure claims. Agreement did not substitute for proof: the validator independently inspected source and test bodies for all five functional closures and the unique documentation proposal. No focused follow-up was needed because no material conflict/untraced common cause remained. The validator discovered no new independent blocker requiring separate validation. The residual uses the original finding ID rather than creating a new finding.

## Reconciled acceptance and carry-forward

| Original/remediation obligation | Source and focused proof assessment | Result |
|---|---|---|
| FIND-1: provider attempts retain each accepted exact origin through trusted delivery | Gateway spans repeat retained per-capture workload/subject/Run/environment/request; outbox retains account pair; Rows admission preserves accepted fields while still invoking canonical value validation. Configured journey inspects five attempts with mixed origins/failover/streaming; exact Rows unit independently rerun PASS. | CLOSED |
| FIND-2: public native drift cannot supply derived shared dimensions | Frame-only noncanonical builtin rule rejects six non-null dimensions before stamping; null placeholders receive accepted context. Native journey proves six mutations under rootless/rooted contexts and reads only valid owned/rootless/delegated rows. Trusted Rows, canonical signals and dynamic fields preserve their paths. | CLOSED |
| FIND-3: one shared exact execution-owner rule and accepted handoff | AuthContext uses existing owning_root; hash/descriptors/canonical/native frames share that immutable selection and Scribe consumes frame.workload. Delegated OTLP and native proof holds actor account constant across exact roots and distinguishes a rooted subject. Gateway/verification keep their accepted context/binding owner. | CLOSED |
| FIND-4: native same-ID replay binds stable managed origin, warm and durable | Shared digest/reuse exclude only volatile receipt fields. Warm cache, SQL fence and WAL/recovery consume the same identity; redundant OTLP correlation digest removed without removing its inputs. Dynamic-table journey covers changed workload/pair/subject/Run through restart; two exact identity units independently rerun PASS. Fixture change repairs account continuity without weakening production comparisons. | CLOSED |
| FIND-5: delayed accepted delivery can describe a deleted subject without authorizing new work | Trusted row_dimensions uses retained tenant-bound lookup; public observation_targets still performs a live read outside cache membership. Gateway cold/warm deletion readback and fresh public refusal inspect the changed gap; existing local/peer frame owner is shared. | CLOSED |
| FIND-6: original inventory and new materially changed items meet mandatory docs | Original inventory corrections are present; new changed nested rows fixture lacks item-local purpose/Panics docs. Outer test docs do not attach to it. | OPEN |
| Original catalog/grants, schemas/defaults, typed projection/raw values, SDK/public declarations, stored-row lifecycle and evidence-reader behavior | Carry forward prior source-backed acceptance. Changes to trusted validation, owner/digest inputs and descriptor restoration justified focused reinspection of their affected callers, not comprehensive rediscovery. No changed dependency or new evidence invalidates other original acceptance. | PASS carried forward |
| Non-goals and stronger boundaries | No new writer, graph owner, public API/header/JWT profile, alias, migration, persistence policy, schema, evidence narrowing or performance claim. Tenant/graph authorization, public forgery refusal, accepted binding ownership and nonblocking audit remain. | PASS |

## Validated remaining ledger

**FIND-TASK-003-6 — REVISED / VIOLATION / OPEN (incomplete closure).** Prior source `REPO-003-001`; current discovery `remediation-review.md`; independently confirmed and narrowed in `findings-validation.md`.

- Exact location: `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs:1333`, nested `fn rows(value: i64, workload: &str, request: &str, receipt: i64) -> RecordBatch`.
- Obligation: root AGENTS requires rustdoc on every changed Rust item and blocks missing docs; Rust guide explicitly includes private/test helpers and requires Panics for possible panics. R1 extends that same obligation to newly changed remediation items.
- Source proof: R1 changes this helper's signature, schema and array to include workload. The distinct nested item has no doc comment and ends with `RecordBatch::try_new(...).expect("identity batch")`. It directly supplies the changed identity assertions. It is neither unchanged debt nor a blank-line preference.
- Consequence/materiality: explicit mandatory gate remains unmet; no runtime defect or likely fixture panic is alleged.
- Smallest correction: add brief item-local fixture intent/invariants and accurate Panics docs. Preserve its body/callers/assertions. No Errors section, new helper, runtime behavior or test suite is needed. Deleting/inlining would rewrite accepted proof and duplicate work without a smaller correction.
- Closure proof: inspect the helper's own docs and a documentation-only diff; run narrow formatting checks and applicable lint checks only. Preserve all five closed behavior findings and existing proof.

No optional finding is retained or invented. Misleading existing refusal code, existing overbroad test names and mixed Panics heading spacing do not establish an additional material regression or mandatory violation for this remediation; no corrective work is assigned for them.

## Verification and limits

The orchestrator independently ran three exact Redux library tests for logical identity volatility, known-identity reuse and trusted Rows environment/request: **3/3 PASS**, 862 filtered, nextest ID `89832355-57b0-4b12-8b09-1d4d81dfc89d`. `git diff --check d1d02cced ee970fbbe` passed. See `verification.md` for the exact command and provenance. Cargo was serialized; no source edits or policy suppressions were made.

Gateway 5/5, server 4/4, OTLP 2/2, replay/retry integration 20/20, libraries 1431/1431 and fmt/lints remain recorded implementation evidence. Reviewers inspected the relevant assertions and diagnosis source; they did not rerun those broad results or claim access to an unsupplied diagnostician transcript. No missing focused behavior proof was established that justified another build or a broad suite. No generated contract changed, so codegen was not required for this delta.

This is one unsuccessful remediation review, so the two-round convergence threshold is not reached. The functional corrections converged; only one newly modified proof helper missed the same existing documentation gate. The next scope is that item-local correction and actual delta, carrying forward all five behavior closures and unchanged task acceptance. This is an explicit mandatory-doc closure, not an optional style review or another behavioral discovery round.

A fresh bounded comprehension reader inspected the approved specification, original task and R2 task without implementation or review conclusions. It found no material gap or contradiction in the correction, invariant preservation, static proof or absence of new product/architecture decisions. This adds no approval gate or source-verification claim.

**Verdict: FIX_REQUIRED.**

**Remediation:** `TASK-003-R2-document-identity-fixture.md`, directly to `$wyrd-implement`.
