# TASK-003-carryover mid-task verdict: FIX_REQUIRED

The current implementation has eight validated behavioral defects and one mandatory verification evidence gap. Correct them through [TASK-003-carryover-R1-current-work-corrections.md](TASK-003-carryover-R1-current-work-corrections.md). This is the requested mid-task review, not completion acceptance. Cycle 11 remains intentionally RED and cycles 12–20 remain the original task's unfinished work. No blocked ruling or specification revision is required by the retained findings.

## Immutable subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd`.
- Base: `4355587f128a0ac1d05071be0a8a84ba53a52290`.
- Candidate: `b7206eafefbf7e981898eca3fc22a5d5ae76b815`.
- Source comparison: `git diff 4355587f12...b7206eafef`; 12 commits after base, including the two cycle-6 commits and cycle-11 test commit. The supplied count of 11 does not change the identified range.
- Contract: [SPEC-authorization-model revision 7](../../spec.md); [original task](../../tasks/TASK-003-carryover.md), especially assigned REQ-002/011–018 and AC-004–013.
- Governing authority: root and applicable SQL/docs/TypeScript `AGENTS.md`; `changes/spec-driven-development.md`; `TESTING.md`; `agent_docs/{rust,errors,maintainer-style,agent-surfaces}.md`; security posture social admission/application mail, browser sessions and tenant binding; design human sign-in; deployment native UI/BFF routing. Applicable UI verification skill supplies the minimum UI lane.

HEAD and production source remained unchanged. This review writes only new reports/remediation under this directory. There is no prior carryover review closure to carry forward and no remediation-round convergence assessment yet.

## Independent review coverage

| Reviewer | Concrete assigned boundary | Evidence |
|---|---|---|
| behavior-rev | Primary requirement/scenario mapping, methods/password/source admission, observable assertions, recovery consumers and TDD; rules not separately assigned | [behavior report](task-review-behavior.md) |
| domain-rev | Browser/purpose/provider/revision binding, email one-use lifetime, verified provider/contact proofs, challenge/secret boundary | [security report](task-review-domain.md) |
| invariant-rev | Shared durable admission, cap concurrency/key lifecycle, claim ownership and sibling writers, promotion, contacts/RLS/migration purposes | [lifecycle report](task-review-invariants.md) |
| repo-rev | Contracts/projections/unchanged consumers, BFF seam, dependency/owner reuse/rustdoc and affected gates | [repository report](task-review-repo.md) |
| ponytail-rev | Fresh independent source validation of every proposal and complete coverage/proof reconciliation | [validated findings](findings-validation.md) |

SQL/lifecycle and security reviewers cover material failure/recovery paths; repository and primary reviewers cover owner reuse and maintainability. Separate system/maintainer reviewers would duplicate those assignments. All selected reports are present. The orchestrator did not discover or validate findings. No new finding originated in validation, so no additional self-validation exception is used.

## Reconciled requirement and scenario matrix

`PASS` here means the bounded current-work claim has credible source/assertion and recorded execution support; it is not whole-task acceptance. `UNRESOLVED — planned` identifies unfinished assigned behavior, not unknown authority or a blocking ruling. Findings are defined once in [findings-validation.md](findings-validation.md).

| Existing requirement / task scenario | Implementation and actual proof | Status |
|---|---|---|
| REQ-014 trusted visitor source; cycle 1 / AC-005 | `TrustedIngress`/`RequestSource` and shared `source_key`; direct Rust journeys assert trusted/untrusted peers, IPv4 mapping, IPv6 /64, distinct windows and signup missing-source refusal. Production BFF drops the source. | FAIL: FIND-003-carryover-7 |
| REQ-014 every signup throttle; cycle 2 | Shared HTTP error renderer and `SocialSignIn::admit`; journey asserts 429, Retry-After equals whole-second problem field, source/identity/start exhaustion and unavailable admission. | PASS for delivered response semantics |
| REQ-014/017 HMACs, bounded cleanup, spent retention; cycle 3 / AC-005/007 | Keyring HMAC and auth-maintenance batches have actual storage/batch assertions; email spent digest is deleted too early and rotation loses cap identity. | FAIL: FIND-003-carryover-4, -5 |
| REQ-014/015/017 local password policy; cycle 4 / AC-012 | Shared `new_password`; real password-path journey asserts 14/15/128/129 and Unicode normalization/input bounds. New future password consumers remain assigned later. | PASS for delivered validator consumers |
| REQ-011/012/014/016/017 usable methods and enforcement; cycle 5 / AC-011 | `available_methods`, tenant policy and delivered handler checks; methods journey asserts exact advertisement and no-session/no-User negative outcomes. Wrong disabled-method status and SMTP redemption predicate remain. | FAIL: FIND-003-carryover-2, -8 |
| REQ-012/014/017 sealed pre-proof provider/email state; cycle 6 / AC-005/007 | Sealed purpose variants, fixed redirect/provider revision, PKCE/nonce, same-origin/cookie checks; journeys inspect no pending row, delivered fragment proof and wrong-browser refusal. Durable spent retention and SMTP removal fail. | FAIL: FIND-003-carryover-2, -4 |
| REQ-012/014/017 one completion path and own interrupted claim; cycle 7 / AC-005/007 | Local/social credential union → claim/tenant initialization → promotion → ordinary session; journey asserts usable admin, expired/foreign attempts and two-minute own stale recovery. Local hashing order, successful replay and generic operator sibling adoption fail. | FAIL: FIND-003-carryover-1, -3, -6 |
| REQ-014 persistent atomic cap; cycle 8 | SQL per-creator transaction lock/count/claim; two-replica assertions cover same-key race, attempt purge, failed claims, own resume and fresh operator creation. Supported rotation changes count/lock identity. | FAIL: FIND-003-carryover-5 |
| REQ-014 optional browser challenge; cycle 9 | `require_challenge` + screened Siteverify, after admission/before claim; journey checks absent/invalid/reused/mismatched/outage cases, no claim and challenge-call ordering. | PASS for current HTTP browser completion boundary |
| REQ-011/012/013/014/017 verified local/social creator; cycle 10 | Tenant transaction inserts User/contact/admin; journey checks local usable admin, Google validated email_verified, GitHub acquisition scope/verified emails and bound fallback. Returning/invited login keeps its distinct proof semantics. | PASS for contact provenance/transaction; completion defects above still apply |
| Affected UI verification; current UI edits | BFF method projection and signup-page password wording changed; supplied checks omit mandatory UI lane evidence. | FAIL: FIND-003-carryover-9 |
| REQ-002/014 named-human setup; cycle 11 / AC-008 | Candidate adds only the journey/handoff; observed RED is missing required human input before claim. No production completion claimed. | UNRESOLVED — planned |
| REQ-011/017 verified contact replacement/discovery/reset; cycles 12–13 / AC-009/010 | Contacts foundation does not implement directory reconciliation, disclosure revalidation or reset journeys. | UNRESOLVED — planned |
| REQ-018 isolated platform console; cycle 14 / AC-013 | Existing machine platform API sessions are not the approved human console outcome. | UNRESOLVED — planned |
| REQ-002/011–014/017/018 mock and production pages/docs; cycles 15–16 / AC-004/005/007–013 | Mock gate remains cycle 15; production consumers follow it. No mock approval or visual match is claimed. | UNRESOLVED — planned |
| REQ-012/015/016 explicit linking/removal/current-anchor invalidation/SSO; cycles 17–19 / AC-006 | Existing returning social behavior is dependency evidence, not completion of these remaining connected workflows. | UNRESOLVED — planned |
| REQ-002/011–018 delivered client/MCP/CLI/UI and provider proof; cycle 20 | Signup Rust-client/MCP support is absent; HTTP tests do not substitute. Live-provider smoke and all delivered-surface journeys remain required by original task. | UNRESOLVED — planned |
| REQ-014 retirement releases cap | Counting excludes deleted tenants; current tests retire through SQL. No public retirement surface/journey is established. | UNRESOLVED — known original-task delivery/evidence limit |
| Tenant isolation, persistent boundaries, no hot-path expansion | Contacts lead with tenant ID, force RLS and have tenant-qualified references; genuine platform records stay operator-owned; actual capability signatures inspected. No unsupported persistence purpose or defined hot-path wait was established. | PASS structurally, subject to correcting retained lifecycle findings |

## Behavioral evidence and verification limits

Latest recorded targeted execution is 1138/1138 at `4dd3b6b24a`, plus clean Clippy with `-D warnings`, `codegen:check`, `docs:check` and `check:tenant-isolation`. Source comparison from that commit to candidate contains only setup-test registration, the intentional RED journey and task evidence. The prior production proof therefore remains useful where source/dependencies are unchanged; it does not exercise the missing cases or make candidate entirely green. Review independently inspected assertions rather than accepting test names. `git diff --check` passed during orchestration and repository review. No fresh runtime, UI lane, live-provider, benchmark or broad gate execution occurred in this review.

The changed waits are in authentication/onboarding/provisioning and background maintenance, outside the explicitly enumerated verification, gateway, Oracle and Scribe caller-wait hot paths. No wait on PostgreSQL, Scribe, object storage or Vault is added/moved on those hot paths in this slice; the applicable inventory is empty. L1 engine benchmarking is therefore not an acceptance requirement for this bounded current-work review. This does not waive it if remediation later touches such a path.

## TDD compliance: violated, separately recorded

Preserve three acknowledged violations: cycle 1 production preceded the first journey execution with no observed RED; cycle 7 step 4b production preceded its assertion; cycle 9 Turnstile unit/client tests did not follow the required journey-RED order. Later mutation assertions strengthen behavior proof but do not rewrite history. No historical RED reconstruction or production churn is required as remediation. Cycle 11's intentional missing-behavior RED is expected unfinished work, not a diagnostic failure.

The [remediation task](TASK-003-carryover-R1-current-work-corrections.md) closes FIND-003-carryover-1 through -9. Completing it alone does not finish TASK-003-carryover; the original remaining cycles and delivery proof still stand.
