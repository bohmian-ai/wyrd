# TASK-003-carryover cycles 12–16: FIX_REQUIRED

Six validated findings remain: three implementation defects, two behavioral evidence gaps, and one bounded documentation/evidence gap. This accepts neither cycles 12–16 nor the whole carryover. No specification revision is required. Implement only the cohesive [R3 remediation task](TASK-003-carryover-R3-close-c12-16-gaps.md) when authorized by the caller; review launches no implementation.

## Immutable subject and authority

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `63ee31a0205c9b59a11877d5cf571d7bd0632cd6`; candidate `dfb3599d5fbfb3d863ecb68382257cb30abfd79b`. Actual review diff is `git diff 63ee31a02...dfb3599d5`. Approved authorization-model spec revision 11; original `changes/active/authorization-model/tasks/TASK-003-carryover.md`, cycles 12–16 only. Exclude unrelated commit `05fc05138` and verification-workflow-ui planning changes. Cycles 17–18 are deferred; cycles 19–20 belong to TASK-004. Earlier cycles and previous bounded acceptance remain dependencies, not silently reopened work.

Authority: approved REQ-011–014/017/018 and mapped acceptance scenarios; revision-6/7 persistence purposes; root/SQL/docs/TypeScript AGENTS; narrow social-admission/mail, browser/platform-session and tenant-connection sections of `architecture/wyrd-security-posture.md`; browser-session deployment in operations; applicable Wyrd UI instructions; spec-driven development, implementation execution, maintainer style and TESTING. Scope and navigation are recorded in [subject.md](subject.md).

## Independent assignments and validation

- Fresh `behavior_rev`: approved requirement/scenario coverage, production UI/contracts/mock, TDD, maintainability and unassigned repository rules. [Report](task-review-behavior.md).
- Fresh `domain_rev`: contacts/discovery/reset security, actual SQL capability types/RLS, concurrency, persistence and failure paths. [Report](task-review-domain.md).
- Fresh `invariant_rev`: platform operator session lifecycle, trust-anchor invalidation, plane separation and destructive reauthentication. [Report](task-review-invariants.md).
- Fresh `system_rev`: official-host routing, Node/Rust cookie/origin handoff, deployment failure and evidence. [Report](task-review-system.md).
- Fresh `ponytail_rev`: independent source validation of every proposal, coverage, proof strength and specialist applicability. [Validation and retained findings](findings-validation.md). No validator-origin blocker required another source check.

Root orchestrated and ran checks; it did not discover or validate source findings. Additional repo/maintainer specialists would duplicate primary and boundary coverage. The selected security/lifecycle/system expertise is present. Shared state, sibling callers, unchanged dependencies and downstream consumers were included. A further first-party SDK convenience facade was not invented for neighboring HTTP-only browser operations.

## Reconciled acceptance matrix

Detailed assertion locations and proposal dispositions are in the validation report. This matrix references existing promises rather than creating another contract.

| Existing requirement/scenario | Implementation/assertion/execution assessment | Status |
|---|---|---|
| REQ-017; C12 S1/S4 fresh bound contact/replacement/preserved authority | Rust contact owner, tenant SQL and mail-derived HTTP assertions; fresh platform97/97 | PASS for exercised behavior |
| REQ-017; C12 S2/S5/S7 live bounded discovery/revalidation/RLS/reconciliation | HMAC projection, authorized tenant/User/contact checks, pagination/reconnect/unavailable-read assertions; actual capabilities and forced RLS inspected | PASS |
| REQ-017; C12 S3 private address suppression | Conditional SMTP wait/error distinguishes suppressed recipients | FAIL; -13 |
| REQ-017; C12 S6 purpose/binding/one-use/deadlines | Spent-read expiry proved; first-use envelope expiry unproved | UNRESOLVED; -16 |
| REQ-002/013/017; C13 S1–S3 private reset/stale and offline codes/no lockout | Real-mail/reset/session/CLI-renewal/API-key and admission assertions; fresh platform97/97 | PASS |
| REQ-018/AC-013; C14 password/OIDC/plane separation/replica/TTL/logout/suspension/fresh proof | Existing-owner source and real HTTP assertions; fresh platform97/97 | PASS for exercised paths |
| REQ-018/AC-013; original OIDC connection invalidation | Removal/reinstall reuses revision and revives authority | FAIL; -12 |
| REQ-011/014; C15 mock approval/reserved slugs | Recorded approval; exact18 names refused before claim, operator path preserved; HTTP/browser assertions | PASS |
| REQ-011; C16 quota presentation | Cap refusal leaves creation active | FAIL; -14 |
| REQ-011/014/017; C16 onboarding and connected provider/challenge pages | Email/discovery/reset/contact/invitation/device assertions exist; fresh onboarding run fails6/7 after SMTP refusal; provider/contact/challenge page seams lack connected proof | UNRESOLVED; -15 |
| REQ-018; C16 console password/SSO/provider/relay/signup controls | Six recorded real BFF/Rust console journeys and inspected assertions; safe forwarding/same-operator behavior | PASS for recorded flows; -12 remains |
| REQ-018/AC-013; C16 official-host split | Source/config plus recorded real nginx-template/stub probe and separate real BFF/Rust journeys; full startup/image execution unrun | PASS for bounded routing evidence; image qualification excluded |
| C16 docs handoff and changed relay contract | Required explanation omitted; docs gate lacks successful evidence | FAIL/UNRESOLVED; -17 |
| C16 approved appearance/follow-up rulings | Recorded four-size/mode sweep and justified deviations, Show/device follow-up | PASS for available evidence except -14 |

## Validated findings and correction boundary

Each finding is retained once with full authority, conditions, source evidence and smallest correction in [findings-validation.md](findings-validation.md):

| Stable ID | Classification | Required closure |
|---|---|---|
| FIND-003-carryover-12 | INCORRECT | OIDC removal permanently invalidates original browser sessions and pending bound login/reauth state; reinstall cannot revive them. |
| FIND-003-carryover-13 | INCORRECT | Discovery's public answer/timing conceals recipient suppression and relay outcome. |
| FIND-003-carryover-14 | INCORRECT | Cap refusal disables/replaces creation with existing recovery guidance. |
| FIND-003-carryover-15 | EVIDENCE_GAP | Connected provider/contact/challenge page proof and clean existing onboarding execution. |
| FIND-003-carryover-16 | EVIDENCE_GAP | Contact/discovery first-use email-envelope expiry and prohibited side effects proved. |
| FIND-003-carryover-17 | MISSING / EVIDENCE_GAP | Bounded relay update explanation, generated corpus and passing required docs check. |

No speculative membership leak, broken current docs example, mandatory hosted-provider credentials or new SDK/persistence mechanism is alleged. Matching the mock does not waive explicit cap behavior. The unrun full startup is disclosed separately from recorded focused nginx routing proof and is not a new standalone blocker in this scope.

## Verification and process

[Fresh execution receipts](verification.md): `ui:check`105/105 and zero Svelte errors/warnings; `test:platform:journey`97/97 plus both SQL migration/idempotence tests1/1; tenant-isolation and codegen checks pass. Fresh `test:identity:ui` fails: production BFF entry point passes with Node5/5, onboarding fails with Node6/7 failures after SMTP connection refusal/503. Prior green entry-point results cannot override this failure. Narrow diagnosis traces the shared fixture path; an environment forwarding cause is plausible but not established conclusively. Treat this as inadequate current proof, not an independently established new product defect. Clippy and six console journeys are recorded implementation evidence, not fresh root reruns. The whitespace check reports a new EOF blank line; no material behavioral finding is invented for it.

No defined verification/gateway/Oracle/Scribe hot-path wait is added or moved by the reviewed auth/control-plane changes. The selected reviewers trace those boundaries; no benchmark gate applies. Full official-image startup, live external-provider smoke and final cross-surface launch proof remain unclaimed and with their approved later owner.

**TDD compliance: VIOLATED, separately from behavioral verdict.** Preserve C12 first-contact race/S5–S7; C13 supersession/contact-revision/admission/bounded-send/SQL one-use late tests; C14 cleanup compilation failure masquerading as RED; C16 PartB recovery S5 and PartC S3 late tests; and previous recorded task violations. Mutation checks do not establish historical RED. No retroactive RED or production churn is required to repair history.

Earlier findings1–11 stay closed under prior bounded verdicts; this review covers new cycles and does not constitute another failed remediation of them. R3 names the next packet in the task's existing sequence, not a third attempt to fix earlier closed findings. No expanding expectations or contract ambiguity requires escalation. Final HEAD remains the candidate; only new review reports are written, with unrelated entry edits preserved. No commit, push, merge, deploy or implementation occurred.
