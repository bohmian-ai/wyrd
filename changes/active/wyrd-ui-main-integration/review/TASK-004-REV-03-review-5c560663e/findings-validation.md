# Independent findings validation

Subject: base `e0354f9bdf8608046833029252d8a526e0bd999d`, candidate `5c560663e77ae07c33d198862ba6d795b656097a`; first full TASK-004-REV-03 review, approved specification revision 19. No prior acceptance or finding IDs are carried forward. Source remains immutable; this validator writes reports only.

Recommendation: **FIX_REQUIRED** for the independently validated implementation and required-proof gaps below. The visual human gate remains open. No retained correction requires changing approved product behavior or a new material specification decision.

## Method and coverage

Read the task-review skill and navigation, original task (including its normative public contracts, scenarios and required proof), revision-19 specification, root/directory rules, architecture authority map and owning Human sign-in/security sections, Rust/error/agent-surface/maintainer guides, spec-driven workflow, testing lanes and UI skill. Consulted CodeGraph before source searches. Independently read the actual Git diff and source at the affected producers, complete functions to be corrected, sibling writers/callers and tests. Read every selected discovery report: behavior, invariants, standards, maintainer, system and security; read reconciliation, focused followup-review.md and actual reruns in verification.md. Maintainer's empty scoped ledger is validated: existing BrowserSessions/User/issuer, principal/SQL/client and shared UI owners are extended; no independent duplicate-owner or unearned-framework blocker is established.

Verification is orchestrator-owned. No build, Cargo, generator, live auth probe or host load was run here. V1–V12 and listed checks/OpenAPI pass for the assertions actually present. The existing exchange fixture failure is a real failed rerun, not a claim inferred from code. HEAD was independently checked against the candidate; only the review directory is untracked.

## Proposal disposition and deduplication

| Discovery proposal | Independent disposition | Final ID |
|---|---|---|
| BEH-1, REP-3 | CONFIRMED, same extractor/error boundary | FIND-TASK-004-REV-03-1 |
| BEH-2 | CONFIRMED, password form parity/reveal | FIND-TASK-004-REV-03-2 |
| BEH-3 | CONFIRMED, safe retry projection loses approved information | FIND-TASK-004-REV-03-3 |
| BEH-4 | CONFIRMED as missing explicit proof, not a proven SQL paging bug | FIND-TASK-004-REV-03-4 |
| BEH-5 | CONFIRMED as missing explicit replica proof, not a proven SQL invalidation bug | FIND-TASK-004-REV-03-5 |
| BEH-6 | REVISED: required visual delivery/human gate remains open; do not presume a layout defect or prescribe redesign | FIND-TASK-004-REV-03-6 |
| BEH-7, INV-2 | CONFIRMED, same five obsolete fixture writers | FIND-TASK-004-REV-03-7 |
| INV-1, SYS-2, SEC-1 | REVISED to stale queued admission; reject stronger all-attempt/in-flight failure-budget contract | FIND-TASK-004-REV-03-8 |
| INV-3, SYS-1 | CONFIRMED, same KDF permit lifetime | FIND-TASK-004-REV-03-9 |
| REP-1 | CONFIRMED, prohibited unconditional allow | FIND-TASK-004-REV-03-10 |
| REP-2, FOL-3 | REVISED to include independently confirmed fourth test, same mandatory panic-doc omission | FIND-TASK-004-REV-03-11 |
| SEC-2 secret creation response | CONFIRMED | FIND-TASK-004-REV-03-12 |
| SEC-2 metadata-only extension | REJECTED: safe metadata is not a credential-bearing response; no applicable explicit no-store mandate or material cache exposure is proven for this sibling | — |
| SYS-3 | CONFIRMED, new readiness wait ignores the required UI child's exit | FIND-TASK-004-REV-03-13 |
| FOL-1 | REVISED to mandatory concrete authority contradictions; omit gratuitous rewrites of compatible sections | FIND-TASK-004-REV-03-14 |
| FOL-2 | CONFIRMED under the supplied direct literal no-ignore-additions criterion; not gate evasion | FIND-TASK-004-REV-03-15 |

No other proposed finding is silently dropped. Matching reports are corroboration; producer-to-consumer evidence below establishes the retained findings.

## Final deduplicated ledger

### FIND-TASK-004-REV-03-1 — Password body rejection bypasses the public error contract

**CONFIRMED; INCORRECT. Sources:** BEH-1, REP-3. **Obligation:** original task Public contracts requires every new invalid wire value to use the existing validation problem; Human sign-in excludes browser operations from the OAuth error exception; root/error guide requires the canonical mapper.

**Location/proof:** `crates/wyrd/wyrd-server/src/auth/browser.rs:579,625,675,729`, four newly added bare `Json<Password*Request>` extractors. The shared DTOs deny unknown fields. Wrong-type, missing-field and unknown-field JSON is rejected before any handler body/`WyrdErrorResponse` runs. `http/router.rs` edge error handling wraps service errors, not Axum rejection responses. The request-id layer cannot turn a plain rejection into a problem. Principal invitation/credential siblings instead retain `JsonRejection` and use `http/error.rs::body_rejection` (98–108).

**Consequence/materiality:** ordinary invalid requests get Axum status/text rather than stable `400 WYRD_SPEC_400_VALIDATION` problem+json. This violates an explicit new public contract.

**Smallest correction/burden:** reuse `body_rejection` at each of these four HTTP extraction boundaries; retain typed DTOs, same-origin enforcement and scrubbed instrumentation. No new mapper, dependency or OAuth change. Low local burden.

**Closure proof:** a small real-router table over all four routes with malformed/schema-invalid bodies asserts canonical status/content-type/code and no submitted secret echo. Keep named OpenAPI proof as the companion schema check; green OpenAPI alone does not close extraction behavior.

### FIND-TASK-004-REV-03-2 — Password forms omit reveal and reject valid Unicode credentials

**CONFIRMED; MISSING/INCORRECT. Source:** BEH-2. **Obligation:** task password section explicitly requires an accessible reveal control, password-manager/paste support and the server's 15–128 NFC Unicode-character policy.

**Location/proof:** UI `src/routes/t/[tenantKey]/{login,signup,reset}/+page@.svelte`, respectively password inputs at 15,16,14, and `settings/account/+page.svelte:62–64`. Each full page/Entry owner has fixed password inputs and no reveal action. Signup/reset/account additionally use HTML `minlength=15 maxlength=128`, while Rust `passwords::new_password` normalizes then counts `.chars()`. Sixty-five supplementary characters use 130 UTF-16 units yet only 65 server characters/260 UTF-8 bytes; these new-password forms preempt a valid input. Login/current-password inputs do not have that length bug.

**Consequence/materiality:** reachable normal human workflows lack explicitly requested interaction and cannot enter some explicitly supported credentials.

**Smallest correction/burden:** supply accessible keyboard-operable reveal/conceal in the existing password form owners and remove browser character-unit constraints that contradict Rust policy. Keep server policy authoritative, autocomplete, paste and exact value. A small existing-style control suffices; no form framework or client password-policy implementation. Low UI burden.

**Closure proof:** existing BFF/browser owner demonstrates keyboard reveal/conceal without value changes and submission of a valid supplementary/decomposed input through normal form flow; Rust remains the policy judge.

### FIND-TASK-004-REV-03-3 — BFF drops the password refusal's retry information

**CONFIRMED; INCORRECT. Source:** BEH-3. **Obligation:** task/security authority requires stable 429 with retry information, including the supported BFF human workflow.

**Location/proof:** UI `lib/server/auth/browser-sessions.ts:49–50` (`refuse`), `lib/server/problem.ts::{problemKind,safeProblem}`, `lib/server/auth/session.ts::reject`, login action and `settings/account` password action, `Entry.svelte:24`. Rust `Windows::admit` produces numeric `details.retry_after_seconds`; `http/error.rs:138–150` emits Retry-After. BFF reduces that body to a kind, reconstructs a generated example with empty details, then reconstructs again in safeProblem. Actions do not preserve Retry-After; Entry/account render the static remediation containing the literal details-field placeholder.

**Consequence/materiality:** the ordinary throttled user cannot determine when to retry despite required backend information being available.

**Smallest correction/burden:** extend the existing safe problem boundary to preserve only the recognized finite positive retry field for this canonical code, retain it through rejection/action projection, render a usable duration and return the applicable response retry header. Do not forward arbitrary upstream diagnostics. Small cross-BFF correction, no timer/cache/limiter.

**Closure proof:** existing form/BFF negative proof reaches throttling and checks 429, usable numeric retry information and rendered guidance, including password change through the shared path; no credential reflection.

### FIND-TASK-004-REV-03-4 — Required large/tied continuation proof is missing

**CONFIRMED; MISSING (verification). Source:** BEH-4. **Obligation:** task Scenario 3 expressly specifies V3/V6–V9 with >100 credentials, equal creation timestamps, all cursors, exact completeness/final omission, maximum and wrong-target refusals, plus complete administrator lists.

**Location/proof:** server `pg_browser_sessions.rs:1575–1640` creates three keys and a 2+1 split; Rust `principal_roles.rs:236–313`, Python `test_principal_roles.py:129–165`, TS `principal-roles.test.ts:97–133` and MCP `principals.rs:642–767` bootstrap one key and never send a continuation. Inspected `accounts::credential_page`, SQL `page_api_key_metadata`, shared `Principals::list_credentials_page` and projections: tuple ordering/limit+1/current tenant+principal binding appear correct; no actual omission defect is asserted. Passing these selectors cannot supply the requested continuation observation.

**Consequence/materiality:** explicit proof of a new cross-language persisted/public boundary is undelivered; first-page tests miss cursor transport, tie handling and accidental administrator truncation.

**Smallest correction/burden:** extend these existing journeys/fixture owners with >100 valid owner-bound credentials and a timestamp tie crossing a boundary; traverse public page calls on each named surface and compare the complete ID set exactly once and full administrator result. Retain max100/wrong-target negatives. Moderate test-only burden; no pagination service, new runner or repeated credential hashing is required.

**Closure proof:** run only the changed exact V3/V6–V9 selectors and assert one page per request, all expected IDs exactly once, no final cursor, and complete legacy lists. Do not prescribe a full journey aggregate.

### FIND-TASK-004-REV-03-5 — Required local-session invalidation proof stops at one replica

**CONFIRMED; MISSING (verification). Source:** BEH-5. **Obligation:** task Scenario 2 explicitly requires change/reset on A followed by old-cookie refusal on B using existing `start_replica`; AC-024 names this behavior.

**Location/proof:** `pg_browser_sessions.rs::local_signup_login_and_password_lifecycle` at 976 and new V2 use one `start_in_process`; this file contains no `start_replica` call. The SQL live-session revision equality and current issuer checks support the implementation; V12 proves existing SSO/recovery replicas, not local revision mutation. This is missing selected evidence, not a claim that revisions are process-local.

**Consequence/materiality:** the explicit regression proof for the new security authority variant is absent.

**Smallest correction/burden:** extend existing local lifecycle proof with `WyrdTestServer::start_replica` sharing its database, replay old local cookie at B after change and reset at A, and prove new credentials/session work. Preserve independent API keys. Small bounded test extension, no provider/new harness/restart suite.

**Closure proof:** rerun exact changed local lifecycle selector; directly observe refusal at the other replica for both transitions and successful fresh login.

### FIND-TASK-004-REV-03-6 — Required visual delivery and individual human acceptance remain open

**REVISED; MISSING (delivery/proof). Source:** BEH-6. **Obligation:** REQ-044/AC-027 and task V10 require the current six named boards under actual normal seeded-human login, 1440/390 verified light/dark rendering/comparison and separate human acceptance; captures stay external and secret-free.

**Location/proof:** UI `lib/ui-pages.integration.test.ts:495–633`: V10 calls existing API-key recovery `signIn`, captures only when an external directory is supplied, and captures traces/timepicker/waterfall/logs/metrics/points; no independent Spans capture is produced. V11 exercises Spans functionally; V5 earns real local-human functional dev proof. These do not establish the required normal-human six-board visual matrix. Task evidence expressly says human acceptance pending; orchestrator supplied no current external comparison/approval record.

**Consequence/materiality:** mandatory delivery acceptance remains unfulfilled. No source-proven layout defect, demanded redesign or fabricated human approval follows from that fact.

**Smallest correction/burden:** reuse existing Chromium/BFF and local enrollment/login owners to finish the six named visual checkpoints, including Spans, at both sizes/themes with theme preconditions, external no-secret captures/comparisons, and the existing required interaction/content/state observations. Present concrete boards for individual human acceptance and record actual decisions. Moderate existing-proof work plus a real human gate; no pixel-diff service or replacement lifecycle suite.

**Closure proof:** per-board current comparison/interaction records and actual individual human decisions. Automated geometry PASS is credited but cannot close this obligation alone. Keep normal default500 replay at integrated change review, as authorized by the task.

### FIND-TASK-004-REV-03-7 — Migration leaves five existing key fixture writers invalid

**CONFIRMED; REGRESSION/mandatory gate. Sources:** BEH-7, INV-2. **Obligation:** preserve Service/Admin exchange and affected regression proof; root testing rule requires fixing red gates and fixtures without weakening integrity/assertions.

**Location/proof:** migration `20261102000000_local_users_personal_keys.sql:145–158` adds exactly-one relational owner. `wyrd-auth/src/exchange_api_key.rs:840,928,1090,1415,1455` raw INSERTs omit both owner columns (NULL defaults). The backfill only repairs preexisting rows. All five complete fixture writers and their generated/existing Service owners were inspected; production `service_accounts::insert_api_key` already supplies `service_account_id`, and `insert_user_api_key` supplies `user_id`. Existing no-refresh exchange test now fails at 1102 with SQLSTATE23514 `auth_api_keys_one_owner`, as actually rerun in verification.md, before its auth assertions.

**Consequence/materiality:** existing required exchange/recovery regression coverage cannot execute after migration. No production insertion failure is claimed.

**Smallest correction/burden:** adapt all five fixture INSERTs to supply the actual existing Service/Admin owner. Reuse production insert where it preserves the intended lifecycle; keep narrow direct fixture writes for intentionally revoked/expired/wrong-verifier conditions. Do not relax CHECK/FKs, substitute principals, ignore tests or erase assertions. Very small fixture correction.

**Closure proof:** exact failed no-refresh selector plus affected `exchange_api_key::pg_tests` and BrowserSessions helper consumers under existing managed Postgres; no replacement harness/broad aggregate.

### FIND-TASK-004-REV-03-8 — Queued password work can use stale failure admission

**REVISED; INCORRECT. Sources:** INV-1, SYS-2, SEC-1. **Obligation:** task requires shared failure windows and rate admission before KDF allocation; security posture says further attempts after ten/name or hundred/source failures refuse until expiry. Accounting is expressly **failures**, not all login attempts.

**Location/proof:** `passwords.rs:534–548,608–624,738–772`; SQL `local_passwords.rs:227–286`. Both login/change read admission and commit before `check_password` waits for one of four KDF permits. Only completed refusal later atomically upserts failures. An arbitrarily sized request queue can therefore pass an empty/nine-failure SELECT, wait behind KDF work, and continue starting expensive guesses after the common window is exhausted. The atomic upsert does correctly prevent lost failure counts; signup/reset new hashing is not a comparison path. Ordinary re-login for key creation is the same login owner.

**Consequence/materiality:** shared throttling is bypassed by already-queued work; the arbitrary stale queue is a reachable brute-force/resource boundary failure, not just a race between the bounded checks already executing when the threshold changes.

**Smallest correction/burden:** move/revalidate existing shared-window admission after acquiring actual KDF capacity and immediately before starting password comparison, on both login/change through their shared password owner. Keep failure recording atomic, success uncharged, fixed DB-time expiry, generic refusal and request-level unavailable handling. Do not add a second limiter, charge successful attempts, reserve all attempts as failures, require new persistent reservation state or a background sweeper. The proposed strict bound on all in-flight attempts is **rejected**: that stronger concurrency contract is not specified by the approved failure-only policy. Low/moderate local sequencing burden; the source of stale admission can be corrected without that machinery.

**Closure proof:** deterministic queued-work proof occupies KDF capacity, queues an account/source comparison, exhausts its shared failure window via the existing SQL owner, then releases capacity; the queued comparison refuses with 429/Retry-After before its KDF closure begins. Use two DB clients/replica state as appropriate; cover both shared window keys, existing change/login paths, success remaining uncharged and expiry reopening. Do not assert that every concurrently executing operation must reserve the final failure slot, or generate host load.

### FIND-TASK-004-REV-03-9 — Request cancellation returns a KDF permit before work completes

**CONFIRMED; INCORRECT. Sources:** INV-3, SYS-1. **Obligation:** task/spec cap actual KDF jobs at four per process, including reachable failure paths.

**Location/proof:** `passwords.rs::bounded:229–235` owns `_permit` in its async future but moves only `work` into `spawn_blocking`. Full hash/check callers include signup/login/change/reset and setup, plus dummy/rehash work. Server `http/router.rs:122–143` applies protected edge to auth; `EdgeTimeout::call:143–180` returns timeout and drops its response future. Started blocking work continues after the awaiting future is dropped, while `_permit` immediately releases. Replacement requests can overlap additional allocations beyond four. This is an actual timeout owner, not an assumption about client-disconnect behavior.

**Consequence/materiality:** overload/timeout defeats the approved shared-process CPU/memory bound and can affect sibling capabilities.

**Smallest correction/burden:** move the existing permit into the existing blocking closure, retaining it through actual completion. The static semaphore already supports the needed lifetime. No worker service, dependency, shutdown policy or process termination. Tiny ownership correction preserves error conversion and all callers.

**Closure proof:** controlled in-module blocking jobs occupy capacity; cancel async waiters; a fifth job cannot start until an original job completes. No timing-based Argon2 benchmark, synthetic load or new harness.

### FIND-TASK-004-REV-03-10 — New SQL insertion uses a prohibited unconditional allow

**CONFIRMED; VIOLATION. Source:** REP-1. **Obligation:** root forbids gate circumvention by allow; Rust guide permits narrow `expect` as last resort, `allow` only when configuration-dependent.

**Location/proof:** `wyrd-sql/src/queries/auth/service_accounts.rs:401–404`, `insert_user_api_key`. The complete nine-argument insertion and caller `accounts::mint_user_credential` need its table values; attribute unconditionally suppresses `clippy::too_many_arguments`. Its reason describes table shape, not configuration variation.

**Consequence/materiality:** explicit mandatory lint policy violated; passing Clippy while suppressing the rule does not close it.

**Smallest correction/burden:** use permitted narrow `#[expect(..., reason = ...)]` for this earned table-shaped signature; no DTO/insert abstraction is required merely to avoid an argument count. One attribute correction, unchanged SQL/authority.

**Closure proof:** inspect attribute and rerun ordinary owning lint check; never widen workspace policy.

### FIND-TASK-004-REV-03-11 — New password tests omit mandatory panic documentation

**REVISED; VIOLATION. Sources:** REP-2, FOL-3. **Obligation:** root/Rust guide requires rustdoc and `# Panics` for every new/changed Rust item including tests, explicitly blocking merge.

**Location/proof:** `passwords.rs:785,794,816`, all three new inline tests, plus `sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:231–236::users_manage_personal_keys`. Complete bodies have assertions/unwraps/expect and the Rust journey has explicit panic at 272, but their intent docs lack `# Panics`; adjacent new server journeys demonstrate the required brief invariant wording. The journey sibling first noticed during validation was separately source-confirmed by the fresh follow-up reviewer before retention.

**Consequence/materiality:** explicit changed-item merge gate remains unmet, independent of Clippy's test exemptions. This is not an optional wording preference.

**Smallest correction/burden:** add concise truthful panic contracts to these four existing items; keep every assertion. Batch with substantive fixes, never a documentation-only remediation round. Negligible burden.

**Closure proof:** source inspection plus ordinary formatting/lints. No additional runtime test is needed for these comments.

### FIND-TASK-004-REV-03-12 — Direct personal-key creation response lacks no-store

**CONFIRMED; VIOLATION. Source:** SEC-2, narrowed to credential-bearing creation. **Obligation:** revision-19 One-time key secret explicitly selects no-store authenticated creation; security posture requires no-store for credential-bearing responses.

**Location/proof:** `components/principals/routes.rs::issue_credential:398–469` commits and returns bare `Json<IssuedCredential>` containing the one-time plaintext. Its router has no no-store layer; cookie middleware does not add one. UI hooks protect only the outer BFF action, not this approved direct HTTP operation. Sibling invitation/reset handlers at `accounts.rs:419,533` already use `auth::oauth::no_store`; the helper at `oauth.rs:142` adds Cache-Control no-store and Pragma no-cache.

**Consequence/materiality:** the required secret-response storage prohibition is absent at the Rust public owner. Actual intermediary storage/exfiltration is not claimed or required to establish the explicit contract failure.

**Smallest correction/burden:** wrap credential issuance at this HTTP response owner with the existing no_store helper, preserving 200/body and shared domain/MCP shapes; it can cover both User and existing Service/Admin secret results with no field/status changes. Very small local response correction.

**Closure proof:** existing direct credential-create HTTP journey asserts no-store for newly issued User secrets and retained administrative secret response behavior. Metadata-only listing is excluded: its safe fields contain no secret/session identifier, and the cited owner mandates session metadata/credential-bearing responses, not every authenticated metadata resource. No new metadata cache policy is authorized by this finding.

### FIND-TASK-004-REV-03-13 — Dead UI child leaves the new readiness wait indefinitely active

**CONFIRMED; REGRESSION. Source:** SYS-3. **Obligation:** REQ-043/AC-026 usable normal dev startup and retained owned-child lifecycle; task newly requires BFF/login readiness before continuing.

**Location/proof:** `mise.toml:1314–1320` starts UI in a background group and stores its PID in `pids`, but the new loop requires UI HTTP readiness and checks only `$server`. If frozen pnpm install/UI startup exits, background failure does not trigger parent's set-e; server remains alive and the loop never reaches existing wait-n or EXIT trap. The full dev:full owner, cleanup and setup/batch branches were read; setup/batch failures intentionally preserve already-ready services and are a different boundary.

**Consequence/materiality:** ordinary startup dependency failure hangs the launcher with owned services alive and no usable UI until manual intervention. This regression is introduced by the new readiness prerequisite.

**Smallest correction/burden:** watch the existing required UI child while waiting, report its startup failure and exit through the existing cleanup trap. Keep process groups, early successful readiness, deliberate post-readiness setup/batch inspectability and app-first drain. Small lifecycle correction; no supervisor, timeout framework or shared-service crash policy.

**Closure proof:** existing dev lifecycle test forces UI startup to exit before readiness and observes bounded nonzero launcher completion and owned-child cleanup. Retain successful readiness and rejected-app-setup proof; never record raw password-bearing output.

### FIND-TASK-004-REV-03-14 — Required architecture amendments leave incompatible exhaustive rules

**REVISED; MISSING/VIOLATION. Source:** FOL-1. **Obligation:** spec revision19 Authority amendments required with implementation (198–213) explicitly requires local exactly-one authority and the sole one-time Settings secret exception in existing architecture owners; root requires resolving contradictions in their owner.

**Location/proof:** `architecture/wyrd-design.md:953–956` and `architecture/wyrd-security-posture.md:335–337` still enumerate only connection revision or recovery credential as stored session authority; security:362–365 says human sessions require an Active connection revision. Actual new migration:25–36, SQL `BrowserSessionAuthority::Password` and auth resolver:386–408 establish local User/password revision as the third exclusive branch. Design:968–969 still absolutely excludes API keys from page data/JavaScript, whereas the expressly approved account POST action returns one secret and the page displays/copies it. New local/personal-key paragraphs do not state the narrow exception or repair those exhaustive sentences.

**Consequence/materiality:** normative authority still forbids two approved shipped behaviors. This is the exact amendment deliverable required by the approved spec, not a request to document every implementation detail or to revert valid behavior to stale prose.

**Smallest correction/burden:** amend the existing exhaustive sentences to enumerate exactly-one local User/revision, OIDC connection/revision, recovery key; qualify connection checks as SSO. Amend the absolute key prohibition to state solely the deliberate authenticated no-store POST/transient Settings creation display exception, retaining initial SSR/PageData, storage/history, URL, log/telemetry/capture prohibitions and all cookie/token boundaries. Self rights before ceiling intersection are already documented in security API keys and compatible with the unchanged PermissionCheck; no broad permission rewrite or gratuitous Client model hunk is required. Small documentation-only correction; no public/runtime/security decision.

**Closure proof:** inspect complete amended owning sections against existing three-way SQL/session resolver and account POST/display consumers; static evidence suffices. Do not claim or introduce a broader browser-secret exception.

### FIND-TASK-004-REV-03-15 — New journey ignores fail the direct literal review criterion

**CONFIRMED; VIOLATION of direct requested constraint. Source:** FOL-2. **Obligation:** supplied latest review request explicitly includes “no #[allow]/#[ignore] additions.” This is stronger than the root prohibition on circumventing a gate and prevails over conventions.

**Location/proof:** actual diff adds `#[ignore]` to Rust `principal_roles.rs:235::users_manage_personal_keys` and MCP `principals.rs:641::users_manage_personal_key_metadata`. Both complete bodies remain and V6/V9 actually run them with `--run-ignored=all`. Therefore no evaded assertion or missing execution is alleged. Nevertheless, both literal additions exist and the task command does not require ignored attributes: `all` selects ignored and nonignored tests alike.

**Consequence/materiality:** explicit requested diff constraint remains unmet; ignoring this because a convention permits it would silently weaken the user's criterion.

**Smallest correction/burden:** remove only these two new ignore attributes, preserving test bodies and exact V6/V9 selectors. No new feature, skip, global selector exclusion or lane redesign. The Rust SDK fast projection lane uses `--lib` (`mise.toml:310–313`), so this integration test does not enter it. MCP remains inside `pg_tests`; normal wyrd/shared family tasks already provision Postgres before their default nonignored tests (`mise.toml:75–83`, shared owner, `scripts/run-family-tests.sh`). Removing these ignores adds these tests to those existing provisioned family runs; it does not require credentials/Docker in an existing credential-free unit lane. A raw all-target cargo test is not a designated credential-free lane. Two-line correction; preserves required tier ownership.

**Closure proof:** diff has neither added ignore, exact V6/V9 still discover/run both tests, and existing unit/family routing remains as described. Do not broaden gates or add replacement exclusion globs to conceal the work.

## Acceptance and coverage reconciliation

| Obligation group | Reconciled recommendation |
|---|---|
| Protected enrollment, canonical local identity, Argon2 policy, verifier-only storage, atomic invite/reset consumption | Credit source/test-backed PASS in behavior/invariant/security matrices and actual V1/V2/pure-test reruns. No invitation/identity framework redesign. |
| Four KDF jobs, shared password failure policy, public typed password boundary and usable forms | FAIL pending 1/2/3/8/9. Preserve failure-only accounting, fixed windows and existing lazy cleanup; no all-attempt limiter or extra scheduler. |
| Existing browser authority, local revision change/reset, SSO/recovery/bearer/origin behavior | Credit implementation and V1/V12, but explicit local replica observation remains FAIL under 5. No sticky sessions or new store. |
| True User key owner, current roles intersected with ceiling, own capabilities, fresh human creation, Service/Admin compatibility | Credit V3/V4/V6–V9/V12 and source authority. FAIL only for required proof 4, fixture regression 7, and secret no-store 12; never weaken relational ownership. |
| Thin Rust/Python/TS and MCP metadata/revoke, strict me schema, no MCP secret creation | Credit source/projection/codegen/typecheck/catalog evidence, subject to missing continuation proof 4. |
| Setup/dev human account, private ephemeral credentials, early readiness, defaults/current wheel, app drain/failure inspectability | Credit V5/source; fix pre-readiness dead-UI branch 13. Default500 replay stays change-review work. |
| Observe tokens/composition/shared interactions | Credit V10/V11 and component/generator checks for exactly measured properties. Required external normal-human six-board and human acceptance remain FAIL/open under 6. No inferred cosmetic defect. |
| Repository shape/tier/SQL/RLS/generated parity | Scoped reports and actual checks support PASS for implementation boundaries; mandatory attribute/docs failures 10/11, incomplete authority amendments 14 and direct new-ignore constraint 15 remain. |

All retained findings are bounded implementation or explicitly required delivery/proof corrections. None calls for a new public protocol, identity profile, deployment service, lifecycle owner, persistent authority contract or product decision. If an implementation proposes stronger in-flight/all-attempt limits, a new reservation subsystem, browser-secret permission beyond the approved one-time result, or a new metadata cache policy, that is outside these corrections and must not silently enter remediation.

New journey ignores were explicitly examined and are not gate evasion: original task V6/V9 and existing mise owners use `--run-ignored=all`, and verification.md actually ran them. The separately supplied higher-priority literal user criterion still prohibits the two additions; finding 15 preserves that distinction and the credential-free/provisioned-lane boundaries.

The one focused follow-up resolved specific authority/ignore uncertainties and independently confirmed the exact fourth panic-doc sibling; every proposal is disposed above. No additional discovery pass or unresolved factual conflict remains. No validator-originated blocker is retained without separate confirmation. This is a first full review despite the REV-03 task name; the two-unsuccessful-remediation convergence rule does not apply.

Skipped: implementation changes, executable reruns and unrelated discovery. Risks remain the retained auth/runtime/public-contract defects, failed regression fixture gate and unfulfilled required proof/human acceptance.
