# TASK-001 review verdict

**FIX_REQUIRED — FIND-001-1 through FIND-001-13.**

One decision-complete remediation task is ready for `$wyrd-implement`:
[TASK-001-R1-deployment-addressing-remediation.md](TASK-001-R1-deployment-addressing-remediation.md).
No implementation correction or runtime verification was performed by this review.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/deployment-addressing`.
- Cumulative base: `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`.
- Candidate: `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183` (46 commits beyond captured base).
- Frozen source: `/tmp/wyrd-task-review-ef2fe8bd`; [complete diff](cumulative.diff), [paths](changed-paths.txt), [history](commits.txt), [subject](subject.md).
- Reviewed approved spec: revision 5 at that commit; original task `changes/active/deployment-addressing/tasks/TASK-001-deployment-addressing.md`.
- Remediation authority: current `changes/active/deployment-addressing/spec.md` revision 6, reflecting the owner's subsequent explicit delegation decision.

The owner authorized ongoing test changes aligning with main. The review uses
the fixed archive; later source/tests/commits are outside its verdict, not a
reason to block. The interrupted 0aae38f9 attempt is preserved separately and
is not acceptance evidence. Approved adjacent work is reviewed for correctness
and is not classified DRIFT merely because it is adjacent. Current stronger
inline-Prompt materialization and async-connect authority were honored.

## Decision resolved after independent validation

The original independent validation recommended SPEC_REVISION_REQUIRED because
existing scopes cannot always express both parties' exact shared permissions.
The owner explicitly chose **“Reject unrepresentable exchanges (recommended)”**
and confirmed both actor and subject must permit an operation. That settles the
previous stop: successful exchange keeps exact shared authority; otherwise
return a clear refusal before signing and issue no token. Do not add a new
scope or silently remove shared permissions. Mixed inputs are not themselves a
refusal if the complete result is representable; no-overlap is an exactly
representable empty result and gains no new rejection policy from this decision.

This approved amendment is recorded as revision 6 and reconciled in
`architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`, and
`architecture/v1/00-foundations/permission-model.md`. The original reports remain
historical evidence of the r5 audit. [Approval resolution](approval-resolution.md)
records the validator's bounded follow-up on the amendment and remediation
packet. No human-directed addition is presented as a new independently found
FIND. Approval settles the correction policy; it does not fix FIND-001-1.

## Independent review coverage and reconciliation

| Role | Report | Result |
|---|---|---|
| Behavior implementation | [task-review-behavior.md](task-review-behavior.md) | FAIL; full obligations/non-goals |
| Invariant implementation | [task-review-invariants.md](task-review-invariants.md) | FAIL; full obligations/non-goals |
| Repository standards | [standards-review.md](standards-review.md) | FAIL; authority/rule coverage and rustdoc inventory |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL; changed owners/consumers/reuse |
| System resilience | [system-review.md](system-review.md) | FAIL; deployed and interruption/recovery paths |
| Security/RBAC | [domain-review-security.md](domain-review-security.md) | FAIL; grants, issuance, delegation, credential/egress boundaries |
| Tenancy/persistent data | [domain-review-data.md](domain-review-data.md) | FAIL; SQL/RLS, persistence, identity/capture |
| Concurrency/durability | [domain-review-concurrency.md](domain-review-concurrency.md) | FAIL; snapshots, claims, lifecycle/outbox |
| Prompt/provider/SDK | [domain-review-prompt-sdk.md](domain-review-prompt-sdk.md) | FAIL; compilation/media/identity/public projections |
| Focused follow-up | [followup-review.md](followup-review.md) | RESOLVED; permission representability and AC-004 proof conflict |
| Independent Ponytail validation | [findings-validation.md](findings-validation.md) | All proposed IDs disposed; 13 consolidated retained causes |

The [claim comparison](claim-comparison.md) grouped causes rather than counting
votes. Follow-up was required because correction representability was uncertain
and the implementation reviewers disagreed on AC-004 proof. The validator
independently traced all proposals, including unique ones, against source and
approved authority. AC-004 is a missing proof finding, not demonstrated hash or
storage corruption. Original source IDs and CONFIRMED/REVISED dispositions are
preserved in the validation crosswalk; no rejected proposal is softened into
optional advice. The orchestrator adds no independently unvalidated defect.

## Validated findings

Exact source locations, reachability, obligations, correction boundaries and
focused proof are in [findings-validation.md](findings-validation.md) and copied
into the self-contained remediation task. FIND-001-1's correction is completed
by the subsequent approved r6 decision.

| ID | Severity | Diagnosis | Selected existing-owner correction |
|---|---|---|---|
| FIND-001-1 | Critical | Permission algebra loses explicit grants and can grant delegated access one party lacks | PermissionSet and existing tenant issuer; refuse an unrepresentable exact intersection under approved r6 |
| FIND-001-2 | Important | Loaded runtime Agent drops its declared gateway route | Retain declared route at Agent owner and bind effective route per invocation |
| FIND-001-3 | Important | Public Anthropic/Google/Jev Agent paths lose or reject named targets | Repair current native caller projection and supported Jev Agent path |
| FIND-001-4 | Important | Portable image/document binding loses declared kind | Shared compiler/Prompt binder preserves and validates derived expected kind |
| FIND-001-5 | Important | Invalid output schemas register and dispatch | Shared pure acceptance validates schemas for native/portable/builders |
| FIND-001-6 | Important | Agent initial target ignores approved provider/adapter matrix | Ordinary initial admission enforces Agent-origin compatibility; preserve direct translation/fallback |
| FIND-001-7 | Important | Restricted batches can hide visible batches behind an unusable page | Existing snapshot/SQL eligibility filters before bounded LIMIT |
| FIND-001-8 | Important | Rendering can change the declared model | Shared literal model validation before derivation/rendering, reused by builders |
| FIND-001-9 | Important | Retained spans label arbitrary deployment names as providers | Carry actual admitted attempt provider to current capture projection |
| FIND-001-10 | Important | Batch dispatch reuses authorization from a different snapshot | Authorize requested target at actual snapshot admission; preserve claim fences |
| FIND-001-11 | Important | Portable media accepts invalid payload/MIME/source combinations | Validate at existing shared pure media/binder boundary with existing budget |
| FIND-001-12 | Important | Changed Rust items fail required rustdoc gate | Document bounded changed item inventory in place alongside substantive corrections |
| FIND-001-13 | Important | Three-SDK authored JSON/hash/introspection preservation proof is incomplete | Extend existing register/hydrate owners; no new production accessor/harness |

## Reconciled acceptance matrix

PASS means inspected source and recorded focused assertions support that
bounded obligation without a retained counterexample. No PASS claims fresh
execution on this candidate. FAIL means a validated source or required-proof
gap remains. T01–T17 identify original task commands, not newly run tests.

| Obligation | Implementation evidence / diagnosed gap | Verification evidence / limit | Result |
|---|---|---|---|
| REQ-001 | Named deployment/ModelRef/planner implemented; public Agent target and retained provider defects, FIND-001-3/9 | T03/T04/T17 cover core addressing; T10 bypasses defective Agent producer | FAIL |
| REQ-002 | PermissionSet/issuer and GatewayBatches defects, FIND-001-1/7/10; approved r6 resolves correction policy | T04 isolated grants omit mixed issuance/delegation and restricted-page/race cases | FAIL |
| REQ-003 | CLI complete-file validation, ordered per-resource PUT; current credential lifecycle retained | T06/T13 source assertions and recorded passes; not independently rerun | PASS |
| REQ-004 | External adapter/credential identity separation, unchanged model suffix and screened transport retained | T05/T13 source assertions and recorded evidence | PASS |
| REQ-005 | Shared authored model remains templatable, FIND-001-8 | T01/T02 lack model identity refusal | FAIL |
| REQ-006 | Shared compiler lacks typed binding/schema/media validity, FIND-001-4/5/11 | T02 mapping assertions omit identified negative cases | FAIL |
| REQ-007 | Native pair/extension preservation present, schema validation missing FIND-001-5 | T01/T02 valid roundtrip does not exercise invalid schema | FAIL |
| REQ-008 | Authored codec/hash/materialization retained; early validation/proof gaps FIND-001-4/5/8/11/13 | T07 and T14/T15 do not close invalid inputs or all preservation combinations | FAIL |
| REQ-009 | Runtime Agent drops declared route, FIND-001-2 | T09 manually attaches registries and does not load declared gateway route | FAIL |
| REQ-010 | Agent route/projection/compatibility/batch snapshot defects FIND-001-2/3/6/10 | T08 OpenAI and raw T10 cannot prove unsupported Agent paths | FAIL |
| REQ-011 | Direct runtime/public validation projections incomplete, FIND-001-2/3/5 | Codegen/T12/T14-T16 do not prove defective producer paths | FAIL |
| REQ-012 | Workflow Card/runtime/jobs/surfaces removed; independent Eval/Agent/judge owners retained | T12 served-contract assertions plus removal/consumer inspection | PASS |
| REQ-013 | Existing installed CLI/bootstrap/saved-auth/four-provider/restart owners reused | T13 source and recorded focused pass; not independently executed | PASS |
| INV-001 | TenantConn/RLS/tenant snapshots/registry/sealing owners retained; no validated cross-tenant defect | T04/T05/T13 recorded claims plus data/security source review | PASS |
| INV-002 | Secrets stay off Cards, bound/redacted; Wyrd identity not forwarded upstream | T05/T13 source/header assertions and existing credential owners | PASS |
| INV-003 | Retired shapes/weights/Workflow capability removed without compatibility aliases | T01/T03/T06/T12 source assertions and static inventory | PASS |
| INV-004 | Agent roundtrip changes route and rendering can change model, FIND-001-2/8 | Observed override tests omit direct runtime construction/render path | FAIL |
| INV-005 | Rust durable decisions, reusable Skald and shared client owners retained | Source/dependency inspection and recorded check:deps | PASS |
| INV-006 | Existing nonblocking process ScribeOutbox remains single audit/capture owner | Source inspection; no new queue or synchronous acceptance dependency | PASS |
| INV-007 | Authority/route/target/media defects FIND-001-1/2/4/6/8/10 | Existing isolated/happy tests omit counterexamples | FAIL |
| AC-001 | Core named onboarding, defaults/restrictions, same-adapter addressing owners present | Existing three-runtime gateway sources and recorded T03/T04; not all Agent protocols | PASS |
| AC-002 | Grant algebra, batch pagination and snapshot failures FIND-001-1/7/10 | Missing real mixed/delegated and restricted batch edge scenarios | FAIL |
| AC-003 | External target/security/revocation/screening owners retained | T05/T13 recorded forwarding/header/evidence checks; provider metadata defect tracked separately | PASS |
| AC-004 | Authored preservation implementation supported; required proof incomplete FIND-001-13 | Follow-up verifies missing native-inline/hash/introspection and Python/TS preservation matrix | FAIL |
| AC-005 | Direct runtime route, native-family producer and compatibility failures FIND-001-2/3/6 | T08/T14/T15 observed OpenAI and T09 manual binding do not close full route contract | FAIL |
| AC-006 | Media/schema/literal identity/source validity failures FIND-001-4/5/8/11 | T01/T02/T07 lack counterexample negatives/public boundary coverage | FAIL |
| AC-007 | Stated constrained judges retain both forms, structured verdict, subject/SYSTEM and native refusal | T11 and existing judge-route refusal source/recorded proof; generic incompatibility remains FIND-001-6 | PASS |
| AC-008 | Public Agent native addressing and initial compatibility failures FIND-001-3/6 | Raw HTTP T10 is not the public Agent producer | FAIL |
| AC-009 | Runtime declarations/parity, documentation and preservation proof gaps FIND-001-2/3/12/13 | Static/type/codegen claims cannot close actual source defects | FAIL |
| AC-010 | Workflow removal and independent Eval fields/DAG retained | T12 served-contract source and retirement inventory | PASS |
| AC-011 | Real installed CLI/fresh saved-auth TS four-adapter/evidence path exists | T13 inspected test plus recorded evidence, not independent execution | PASS |
| AC-012 | Single/list validation, convergence/progress and restart owners retained | T06/T13 inspected assertions and recorded results | PASS |

## Non-goals and regression boundaries

The cumulative review retains the excluded scope: no model mapping/alias or
model catalog; no portable audio/video, new local Vertex gateway support,
replacement application/Workflow runtime or hosted Agent job API; no new
credential/media upload system, compatibility routes, synchronous audit
acceptance or deployed-data deletion. Independent Eval DAGs and observed-data
fields survive. External deployments, existing credential/header/SSRF owners,
tenant RLS, nonblocking ScribeOutbox and already-consistent admitted-call
lifecycles must remain intact during correction. The bounded task explicitly
preserves direct-client protocol translations and authorized explicit fallback.
It does not treat unrelated ordinary “workflow” vocabulary as a removal target.

## Verification, prior closure and handoff

This is the initial completed task review; there are no prior FIND IDs to close.
All 13 findings remain open. The approved spec amendment resolves a decision,
not the implementation defect. The subsequent review must preserve these IDs
and reassess the complete original-base-to-new-candidate range, together with
this verdict, ledger, original task and remediation evidence.

No tests, builds, lint/codegen, package installation, served OpenAPI/MCP runtime,
live provider, crash/restart or multi-pod qualification were executed by review.
Recorded T01–T17/owner-check successes were read as claims and checked against
the actual assertions; the counterexamples are paths those tests omit. The
implementation's concurrent reruns are not silently attributed to the fixed
snapshot. The full `mise run gate` remains required once at integrated change
review; its absence during this task review is not a separate finding.
Remediation requires only its focused behavior and applicable owner checks,
not full journey suites or aggregate repetition.

Only review artifacts and the explicitly approved specification/architecture
amendment were written. No implementation/test source was changed, staged,
committed, merged, pushed or deployed. The task is ready for a fresh
`$wyrd-implement` agent; this verdict is not implementation authorization to
merge or deployment approval. Material residual risks include delegated access
one party lacks, gateway-declared Agents executing natively, broken provider
paths and stale batch authorization until corrected.

## Subsequent owner-directed docsite addition

After the validated review and delegation decision, the owner added initial
gateway documentation to the same remediation task: gateway overview,
provider onboarding and real first requests with stock clients and common
frameworks. The task records complete scope and R1-DOC-AC-01–03 under approved
`SPEC-developer-docsite-rebuild` r2 and its TASK-006 Connect structure, including
OpenAI Agents SDK/Pydantic AI, actual example proof, navigation/search/index
integration and `mise run docs:check`. This is explicit human-directed work,
not a newly discovered FIND or a claim that the guides are implemented. The
validated finding count stays 13. The earlier approval-resolution report
assessed the packet before this addition; it is preserved unchanged.
