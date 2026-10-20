# TASK-001-R1 cumulative task review

**FIX_REQUIRED**

Seven independently validated bounded source findings remain:
**FIND-TASK-001-3 through FIND-TASK-001-9**. Prior FIND-TASK-001-1's installer
implementation cause is closed. Prior FIND-TASK-001-2's executable nginx path
now exists, but successful Linux execution remains pending. Required native/
image/human evidence is not certified. No new architecture decision is needed.

Remediation assignment:
[TASK-001-R2-session-boundary-fixes.md](TASK-001-R2-session-boundary-fixes.md).
Route to `$wyrd-implement`; review does not execute that assignment.

## Immutable subject and authority

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
- Cumulative base: `8651b96fd0c049ab5e63b0606c98326af1fc8848`.
- Candidate: `2741395a33d5e1e5ee5d7022916627917254a663`.
- Previous reviewed candidate: `843484f6cceba1f48139349f66794966221c5d37`.
- Remediation navigation base: `deadf39af11ab4cb75c2e0fa0495fc83a2cf127e`.
- Approved [spec revision 10](../../spec.md), original
  [TASK-001](../../tasks/TASK-001-distribution-login.md), retained
  [TASK-002](../../tasks/TASK-002-remove-old-ui.md) behavior, and
  [TASK-001-R1](../tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md).
- Prior [verdict](../tasks-001-002-843484f6c/verdict.md) and
  [validated ledger](../tasks-001-002-843484f6c/findings-validation.md) remain
  immutable historical inputs. Their review subject and verdict were not changed.

Review covered `git diff 8651b96fd...2741395a3`, using
`deadf39af...2741395a3` only to locate changed remediation owners. Approved
planning/reference additions are context, not unrelated shipped behavior.
Revision 10 supersedes the confidential Node OAuth/encrypted credential-cookie
premise; that requirement is not revived. REQ-033 is a human-directed change,
not a retroactive reviewer finding. New retained finding IDs begin at 3.

Source/HEAD stayed immutable. Pre-existing trace modifications in
`crates/wyrd-spec/src/vala/trace/mod.rs` and deletion of `trace_summary.rs` were
excluded and preserved; the subject records their diff digest. Only this new
review directory's reports and remediation assignment were written. No source
implementation, merge, commit, push or deployment was performed.

## Independent reports and reconciliation

| Fresh independent role | Artifact | Discovery result |
|---|---|---|
| behavior-rev | [Behavior acceptance](task-review-behavior.md) | FAIL: native public auth forwarding and metadata response |
| invariant-rev | [Invariant acceptance](task-review-invariants.md) | FAIL: metadata response and prohibited test setting |
| repo-rev | [Standards](standards-review.md) | FAIL: finite mandatory rustdoc and metadata response |
| maintainer-rev | [Maintainer](maintainer-review.md) | PASS within owner/reuse/maintainer scope |
| system-rev | [System resilience](system-review.md) | FAIL: metadata response and failed fixture cleanup |
| domain-rev, security/auth/RBAC | [Security](domain-review-security.md) | FAIL: metadata and callback error classification |
| domain-rev, persistence/tenancy/concurrency | [Data](domain-review-data.md) | FAIL: selected logout tenant binding |
| followup-rev | [Focused documentation resolution](followup-review.md) | RESOLVED; finite rule gap, narrower correction |
| ponytail-rev | [Independent source validation](findings-validation.md) | FIX_REQUIRED; seven source findings retained |

All required reports are present. The additional data-domain audit was scheduled
when a discovery slot became available; it remained fresh and independent.
The [claim comparison](claim-comparison.md) groups shared causes. A focused
follow-up was required for the standards/maintainer rustdoc conflict; it credited
existing lifecycle and inherited formatter documentation while identifying the
finite new-item and delivery-account gap. Independent validation confirmed that
resolution. No additional discovery pass was needed.

The validator inspected every proposed finding, unique claims included, and
selected the existing owner/minimum safe correction from source rather than
reviewer agreement. Metadata reports consolidate to one cause. General
maintainer PASS does not erase those reachable behavioral paths. No proposed
optional style correction enters the retained ledger. No duplicate custom
Origin validator is required: pinned SvelteKit's production form-Origin checks
and form-only action dispatch already protect the inspected actions.

## Reconciled acceptance matrix

PASS below is bounded to the stated source/available evidence; it does not
certify an unexecuted deployment. FAIL (unverified) distinguishes absent
required proof from a demonstrated source defect.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 production package, system Node and three native targets | Existing bundle/build/release owners stage production UI and dependencies; no native Node bundle | Supplied Mac fixture/build evidence; real signed source-free extracted runtime/digests and Linux native runs absent | Source supported; FAIL (unverified runtime acceptance) |
| REQ-002 signed fresh/cache/current eligibility and preservation | Shared `check_payload` governs extraction, pre-activation cache reuse and installed lookup; truthful refusal before current changes | R1 adds realistic positive layout, malformed/cached/active and healthy-reuse exact tests, reported passing | PASS for prior implementation cause; FIND-1 closed in source |
| REQ-003 / AC-002 native startup/readiness/state/cleanup | Existing DevServer owns both children and preserves setup/config; two-child cleanup unchanged | Supplied CLI journey 45 passed/3 ignored and native process regressions; Linux native not exercised | PASS for bounded source/Mac proof; full native acceptance unverified |
| REQ-004 native public `/auth` forwarding | Correct configured target/origin/manual redirect owner, but session-cookie authority is stripped from public session/logout requests | Existing tests/happy path use internal Node calls and miss direct public case | FAIL FIND-3 |
| REQ-005 / AC-004 official topology and lifecycle | Debian/Rocky output matrix and Rust/Node/nginx routing remain; focused provider test exists | Actual image/provider/restart run absent; failure cleanup defect and extra test setting found | FAIL FIND-6/7; FIND-2 execution pending |
| REQ-006 / AC-003 retained login/recovery/chooser/settings/logout | Rust direct SSO completion and safe Node projection substantially present | Supplied native real-provider and identity regressions support happy path; direct/error/mismatch cases incomplete | FAIL FIND-3/4/5/8 |
| REQ-033 Rust durable ownership, digest-only persistence, DB time/RLS | Existing auth/SQL owners, additive session row, five-minute login and twelve-hour absolute expiry, no recoverable key/token | Supplied SQL/HTTP authority tests; separate data audit supports owner/RLS/lock source | PASS within inspected evidence |
| REQ-033 one-use provider correlation, current issuer/connection/credential/permission authority | Existing provider/callback/issuer/key owners reused; no alternate verifier or role authority | Supplied native real-provider, binding/replay/revision and key/grant regressions | PASS except error/selected-logout consumers below |
| REQ-033 current enumeration and unavailable behavior | Rust fails correctly; Node metadata seeds prior identity even after refusal or selected absence | No existing metadata 503/401/missing-current regression | FAIL FIND-4 |
| REQ-033 callback backend error semantics | Completion retains typed error; browser response adapter converts server failure into sign-in refusal | Existing positive/refused cases do not prove post-consume unavailable classification | FAIL FIND-5 |
| REQ-033 selected logout only | SQL digest/RLS correct, but lifecycle mutation does not bind selected route to value tenant | Existing mislabel tests cover enumeration rather than mutation | FAIL FIND-8 |
| REQ-033 tenant HTTP boundary, Origin and bearer precedence | Existing principal/permission/audit boundary; explicit bearer wins; other protocols excluded; framework origin enforcement retained | Supplied focused cookie/bearer/Origin and served OpenAPI tests | PASS in source/available bounded proof |
| REQ-033 no additional deploy secrets/env/service; replicas/restarts | Existing Postgres authority, request-local Node projection, retired OAuth dependencies/profile | Native second Rust replica/fresh Node evidence; same Rust/container restart only in unrun nginx test | Source supported; restart runtime acceptance unverified |
| REQ-033 safe cookies, old-cookie re-login, source-driven contracts | Host-only HttpOnly/Secure/Lax cookies, no token/key payload, typed Rust/served API | Supplied cookie/identity/codegen/OpenAPI checks | PASS within inspected scope, except forwarding/metadata consumers |
| REQ-032 deletion and retained plain pages | Prescribed obsolete graph removed; retained consumers remain plain branded markup | Supplied UI check/unit/build/identity results; no new removal defect validated | PASS implementation; human acceptance pending |
| INV-001/002 contracts, server authority and no failed-live fallback | Existing contract/tenant owners retained, but metadata fallback violates current-authority rule | Source trace of FIND-4 and callback classification | FAIL in retained consumer paths |
| INV-003 one build owner/supported outputs | Current build/release/container owners reused | Cumulative source/manifests | PASS |
| INV-004 / AC-005 small integration/no prohibited test setting | No deferred domain client/controller added; new nginx fixture env violates explicit prohibition | Unique producer/reader, existing transport available | FAIL FIND-7 |
| INV-005/006 no deferred domain/analytical/whole-branch integration | No Cards/Query/Bifrost client/addon/framework onboarding in remediation | Cumulative imports/manifests and owner audit | PASS |
| Mandatory substantive new-item/async rustdoc | Finite new formatters/test modules and delivery semantics lack required docs | Follow-up and independent validation agree on exact limited write set | FAIL FIND-9 |
| REQ-020, INV-007, AC-010/012 preservation and credible acceptance | Original references/reviews/task evidence preserved; real owner-based proof exists | Required platform/human proof and historical RED/diagnosis artifacts incomplete | Preservation supported; acceptance unverified |

Deferred domain/library tasks remain outside this task audit; their unimplemented
features are not new findings. No live credential, expiry, cookie or session
store redesign is demanded by the retained corrections.

## Validated ledger and selected correction boundaries

The [validation report](findings-validation.md) contains complete producer,
caller/sibling and source evidence, dispositions and focused proof. Only its
confirmed/revised findings are included here.

| Stable ID | Discovery sources | Status / classification | Source and observable consequence | Minimum selected correction / closure |
|---|---|---|---|---|
| FIND-TASK-001-3 | BEH-R1-001 | CONFIRMED / INCORRECT | UI `upstream.ts:69–75`: native public metadata/logout loses tenant cookies; logout may clear browser cookie without revoking durable row | Existing forwarder carries Wyrd session authority for browser read/logout operations, preserving unrelated-cookie exclusion and Rust validation. Unit forwarding plus direct native public enumeration/durable logout proof. |
| FIND-TASK-001-4 | BEH-R1-002, INV-R1-001, SYS-R1-001, SEC-1, REP-002 | CONFIRMED / INCORRECT | UI `browser-sessions.ts:262–269`: failed/selected-missing current enumeration produces stale signed-in metadata | Existing metadata consumer propagates non-success and derives selected identity/expiry/choices only from accepted live views. Test 503, 401 and 200 sibling-only plus positive/cookie behavior. |
| FIND-TASK-001-5 | SEC-2 | REVISED / INCORRECT | Server `auth/browser.rs:277–300`: callback unavailable/server error becomes ordinary sign-in refusal after healthy directory lookup | Existing browser response adapter preserves typed server errors/no-store/login-cookie clearing while genuine refusal redirects remain. Focused healthy-directory unavailable/refusal response proof; no lifecycle enum/retry change. |
| FIND-TASK-001-6 | SYS-R1-002 | CONFIRMED / INCORRECT | `test-startup-identity.sh:16`: nonzero status-restoration or failed forwarder deletion under errexit prevents common cleanup | Existing startup cleanup runs unconditionally with original status and existing callers preserved. Tiny stubbed success/failure/forwarder-removal proof plus syntax check; no new controller. |
| FIND-TASK-001-7 | INV-R1-002 | CONFIRMED / DRIFT / VIOLATION | Identity startup/test producer-reader introduces prohibited test-only `WYRD_UI_NGINX_JOURNEY` | Reuse established `WYRD_UI_JOURNEY`, preserving fixture content and exact scenario. Remove extra setting without alias/name-ban framework; focused Linux container run remains required. |
| FIND-TASK-001-8 | DATA-001 | REVISED / INCORRECT | HTTP `browser.rs:498–500`, auth `browser_sessions.rs:288–301`: Acme-selected logout can revoke possessed Beta value relabeled Acme | Existing lifecycle mutation binds selected TenantSlug to authoritative value tenant through existing route resolver before revocation. Preserve idempotent clearing/unavailable/key/sibling behavior; no full live permission check merely to logout. Extend real HTTP/Postgres authority regression. |
| FIND-TASK-001-9 | REP-001, FUP-DOC-001 | REVISED / VIOLATION | Three new Debug methods, two test modules and finite durable/HTTP delivery account lack mandatory rustdoc | Docs only in the two new browser files; explain redaction/formatter errors and commit-versus-delivery/retry once using existing links. No runtime mechanism, duplicate prose or open-ended audit. |

### Prior-finding closure

- **FIND-TASK-001-1:** implementation cause closed. Shared source admission
  and reported exact installer negatives now cover fresh/cached/current paths.
  Actual signed extracted-runtime evidence remains a separate AC-001 limit;
  no new install-time Node/import verifier or installer redesign is requested.
- **FIND-TASK-001-2:** missing executable official nginx path addressed in
  source, execution closure pending. `production_ui_nginx_journey` and its
  focused managed leaf now exist. macOS refused the image before build; there
  is no successful image/source/public-origin result. R2 retains this ID for
  the focused supported-Linux run after source fixes, rather than creating a
  duplicate missing-test finding or asserting actual nginx failure.

No source finding is assigned to the approved human-directed session addition,
unrun platform checks alone, historical RED gaps, task-title/style preferences
or superseded architecture. Do not fabricate past proof or modify baseline
history to manufacture RED.

## Verification and limits

Available implementation evidence, bounded to its recorded source/branch:
merged-tree fmt/lints/codegen and CLI journey 45 passed/3 ignored; session-branch
SQL, principals/served OpenAPI, native identity and CLI refresh regressions,
UI check/22 focused tests/build. These reports were inspected against their
owners/assertions; they do not prove the uncovered direct/error/mismatch paths.
Reviewers did not rerun those lanes or mutate shared services.

Independent review execution was limited to nonmutating Bash failure probes
for the cleanup control flow. No Cargo/image builds, identity-service changes,
broad gate, live provider calls, deployment or human inspection were run here.
Unverified: real signed source-free extracted native runtimes/digests; native
Linux; Debian amd64/arm64 and Rocky amd64 image/provider/restart runtime;
focused nginx journey; human native/nginx desktop/mobile/light/dark/focus/touch
acceptance. Native second-replica/fresh-BFF continuity does not prove restarting
the same Rust process. A written ignored scenario is not an executed pass.

All required independent evidence exists for this audit, so the review is not
BLOCKED by missing reviewers or an immutable subject. Source corrections are
bounded under revision 10, so no SPEC_REVISION_REQUIRED is needed. **FIX_REQUIRED**
routes the single self-contained R2 task directly to `$wyrd-implement`. A later
PASS still requires source closure and credible required runtime/human evidence;
it is not granted by this review or by the recorded green happy paths.
