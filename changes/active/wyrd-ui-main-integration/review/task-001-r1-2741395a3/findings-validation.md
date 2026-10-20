# Independent structured Ponytail validation

Recommendation: **FIX_REQUIRED**. Seven bounded source findings survive independent validation. No new product, security, deployment, persistence or concurrency decision is required. Required deployment and human evidence also remains unverified; missing runs are not evidence of a runtime bug.

Subject: repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `8651b96fd0c049ab5e63b0606c98326af1fc8848`; candidate `2741395a33d5e1e5ee5d7022916627917254a663`; remediation navigation base `deadf39af`. Approved authority is specification revision 10 and current architecture. The original task, retained TASK-002, previous verdict/ledger and TASK-001-R1 remain inputs, with their original subjects intact. Confidential Node OAuth and encrypted credential cookies are explicitly superseded. REQ-033 is human-directed work and receives no retroactive FIND ID.

## Inputs, scope and evidence

Read all nine discovery/comparison/follow-up reports in this directory: behavior, invariants, standards, maintainer, system, security, data, follow-up and claim comparison. Their agreement was not used as proof. Applied AGENTS.md, agent rules, spec-driven-development, maintainer, Rust documentation, relevant design/doctrine, security and deployment authority. No CodeGraph index exists. Reviewed cumulative changed-surface/deletion inventory and diffs, including the remediation range, and independently expanded the producer/caller paths below. Planning/reference additions are approved context, not shipped framework additions. Unrelated trace edits are excluded.

The full correction-owner bodies inspected are `forwardAuth`; Node `BrowserSessions`/`BrowserSession`, `call`, `refuse`, `relayCookies` and problem mapping; Rust browser lifecycle `begin`, `recover`, `recover_in`, `resolve`, `authorize`, `view`, `logout` and session creation; HTTP `login`, `signed_in`, `login_refused`, recovery, enumeration, `live_view`, logout and cookie helpers; callback completion and its HTTP dispatch; startup identity/common cleanup. Inspected their actual hooks/layout/action callers, session SQL and route resolver use, sibling device/connection-test callback outcomes, bearer/protocol consumers, native/nginx fixture producers and assertions. Installer closure was checked across fresh extraction, cache, active lookup, activation and DevArgs, against the concrete build producer and fixture proof. The removal inventory and retained consumer changes were assessed in the cumulative subject, not just R1.

Existing owners suffice: installer/DevServer/build, Rust connection/callback/issuer/key/session/SQL owners, Node forwarding/projection, startup common cleanup and managed identity fixtures. No new controller, cache, cookie key, client credential, verifier, dependency or general test harness is warranted.

No Cargo, image, identity service or broad test lane was run. One nonmutating Bash subprocess reproduction used the candidate EXIT sequence with a stub cleanup: exit 0 printed forwarder and common cleanup; exit 7 printed only forwarder cleanup and retained status 7. This proves the shell control-flow claim without touching Docker. Other findings are source proofs, not claimed runtime reproductions. Focused commands below are closure recommendations, not executions by this validator.

## Every proposed claim: disposition

| Discovery claim | Disposition | Independent resolution |
|---|---|---|
| BEH-R1-001 native public auth cookies | CONFIRMED → FIND-TASK-001-3 | Hook forwards public browser endpoints through `forwardAuth`, which deletes every tenant-session cookie. Internal Node calls bypass this helper and do not repair public native HTTP. |
| BEH-R1-002, INV-R1-001, SYS-R1-001, SEC-1, REP-002 metadata | CONFIRMED, consolidated → FIND-TASK-001-4 | One response consumer discards non-2xx and seeds earlier selected identity unconditionally. The same cause also fabricates selected identity after a successful enumeration containing only a sibling. One correction, not one finding per status/caller. |
| SEC-2 callback unavailable classification | REVISED → FIND-TASK-001-5 | `LoginCompletion::Refused` retains the typed error and audit; it is not intrinsically wrong. The browser HTTP adapter erases its classification. Preserve existing server-error status/code there, not by rewriting callback completion or retrying consumed state. |
| SYS-R1-002 EXIT cleanup | CONFIRMED → FIND-TASK-001-6 | Inherited errexit aborts the trap at the nonzero status-restoring subshell before common cleanup. Stub reproduction corroborates full source tracing. Forwarder removal failure can also abort it. |
| INV-R1-002 test-only environment | CONFIRMED → FIND-TASK-001-7 | New fixture variable has only the nginx test producer/consumer. INV-004 explicitly excludes it; existing `WYRD_UI_JOURNEY` transport suffices for the separately selected test. |
| DATA-001 selected logout binding | REVISED → FIND-TASK-001-8 | Possession of a valid Beta value is required; no theft or arbitrary revocation is claimed. Still, Acme-selected logout routes the mutation to Beta from an unchecked value prefix. Bind selected route to value tenant in the lifecycle owner before opening revocation. |
| REP-001 and FUP-DOC-001 | REVISED → FIND-TASK-001-9 | Five new undocumented items and finite durable-delivery/retry gaps are supported by explicit local documentation rules. Reject REP-001's open-ended cumulative documentation expansion; preserve already adequate linked transaction/callback docs. |
| Maintainer no material findings / documentation PASS implication | Revised only as above | Reuse/shape conclusions hold; general scope PASS does not document the five demonstrably undocumented new items. No structural refactor is demanded. |
| Missing extra custom action Origin validator | REJECTED as a proposed correction | Installed SvelteKit form-Origin enforcement and form-only action dispatch already cover the reachable production actions. No bypass is established. Do not duplicate the framework check. |
| CSS borders/fonts, task-name test title, fully qualified types | Non-blocking only | No behavior, security, durable contract or deletion consequence demonstrated. Do not create a style round. |
| Missing execution / historical RED artifacts | Evidence limits, not new runtime FIND IDs | Written executable proof and an executed pass are different. Do not fabricate RED runs, alter the baseline to manufacture them, or turn an unrun deployment test into an asserted deployment failure. |

## Final deduplicated stable ledger

### FIND-TASK-001-3 — CONFIRMED — INCORRECT: native forwarding drops public browser-session authority

Discovery: BEH-R1-001. Obligations: REQ-004, REQ-033 public typed browser endpoints, AC-003/012. Location: `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:69–75`; caller `src/hooks.server.ts:12`; Rust consumers `src/auth/browser.rs::session` and `logout`.

Rust creates `wyrd_session_<tenant>` and the browser presents it to the native public BFF. The hook forwards all `/auth/*`. `forwardAuth` retains only `wyrd_login`; therefore public `GET /auth/browser/session` reaches Rust without sessions and answers 401. Same-origin public `POST /auth/browser/logout` has its tenant body and Origin but no session: Rust returns 204 and clears the browser cookie without revoking the durable row. A saved opaque value still resolves. nginx instead routes auth directly to Rust, so deployments differ. REQ-004 explicitly preserves cookies, and revision 10 explicitly requires these public browser operations; they are not dormant APIs.

The only production `forwardAuth` caller is the hook. Node `read`, `metadata` and `logout` use their own existing `call` transport and correctly attach cookies, explaining why the page-action journey can pass. `/v1` selected-cookie forwarding and callback binding are siblings, not substitutes. The existing forwarding test deliberately asserts login-only cookies on `/auth/token`, so changing all protocol auth is unnecessary.

**Selected minimum correction:** extend the existing forwarder's cookie policy to carry Rust-owned tenant-session cookies to the browser session/read and logout operations; preserve login-binding forwarding and exclusion of unrelated browser cookies. Let Rust continue to own filtering, validation, revocation and refusals. Do not parse request bodies to build a second session selector, add a store, or widen machine authentication. Preserve trusted origin/header removal, method/body/path/query, manual redirects, separate Set-Cookie and error relay. The forwarder owns this transport invariant because it currently destroys the producer's authority before the real trust boundary receives it.

**Closure:** extend `upstream.test.ts` with public session GET (multiple cookies and tenant query) and logout POST authority forwarding, plus existing token/callback and hostile forwarded-header checks. Extend the existing native production driver with direct public enumeration and logout, demonstrating the saved selected value is refused afterward and siblings survive. The direct regression belongs in the existing journey; no new full suite is required in remediation. Use the UI focused file command below and the existing managed `test:identity:ui` leaf when the journey changes.

### FIND-TASK-001-4 — CONFIRMED — INCORRECT: metadata reuses identity after current authority is absent or unavailable

Discovery: BEH-R1-002, INV-R1-001, SYS-R1-001, SEC-1, REP-002. Obligations: REQ-033 current enumeration/refusal, INV-002, security First-party browser sessions, deployment unavailable-response contract. Location: `wyrd-ui/src/lib/server/auth/browser-sessions.ts:262–269`.

Hook `read` resolves Acme and produces a request-local `BrowserSession`. Tenant layout is the sole production `metadata` caller and performs a later all-cookie enumeration. Rust `session` → `live_view` correctly refuses a store outage (503), no remaining live authority (401), or returns 200 containing only independently valid siblings. Revocation/expiry between requests or failure while reading a sibling cookie makes each path reachable. Node relays cookies, converts every non-OK result to `[]`, seeds Acme from the earlier object and returns its earlier principal/expiry regardless of the current answer. Even 200 with only Beta fabricates Acme. The page renders signed-in metadata after current authority disappeared or was refused; subsequent Rust APIs still reauthorize, so this is not a write-authentication bypass.

`read`, begin/recovery and logout already refuse outages; local-development `sessions.metadata` is a distinct gated owner. `BrowserSession.context` supplies the earlier selected context, and no global Node cache exists. Rust enumeration, SQL liveness, independent tenant choices and cookie cleanup need no new guards.

**Selected minimum correction:** retain the existing one enumeration and cookie relay, then use existing `refuse`/safe-problem mapping for non-success. On success require the selected tenant among returned live views; otherwise reject unauthenticated, never select a sibling or reuse the prior identity. Derive the selected subject/expiry and tenant names from the accepted live view/set. Keep live sibling choices separate and preserve cookie clearing; 503 must not clear cookies or return cached success. This response consumer owns the invariant because it constructs the page projection; another resolver/cache/retry is unnecessary. No redesign of hook context or retained settings is required to close this diagnosed metadata path.

**Closure:** existing `browser-sessions.test.ts` fetch stub/jar can perform a successful selected read then metadata 503, 401 with a clearing cookie, and 200 containing only Beta. Assert existing upstream/authentication problem, no successful metadata, cookie preservation on 503 and clearing on dead authority. Positive multi-tenant enumeration returns current selected fields and all live choices without merging permissions. Run the focused UI file and check command below; no database-down orchestration is needed for this consumer defect.

### FIND-TASK-001-5 — REVISED — INCORRECT: callback adapter hides server failure as sign-in refusal

Discovery: SEC-2. Obligations: REQ-033 unavailable versus refused authority; stable existing error semantics. Location: `crates/wyrd/wyrd-server/src/auth/browser.rs:277–300` (`login_refused`); producer `wyrd-auth/src/callback.rs::complete`; dispatch `components/auth/routes.rs:494–499`.

`complete` consumes and commits state, then verifies/finishes. Store reads/insertion/commit can return `AuthVerifyUnavailable`; provider `temporarily_unavailable` returns `DiscoveryUnavailable` without needing a failing database. Browser failures become `LoginCompletion::Refused { tenant_id, error }`, preserving the error. `execute` stages the existing denied audit. HTTP callback dispatch calls `login_refused`, whose successful directory read discards `error` and redirects to `login=failed`; the login page treats that as unauthenticated. A healthy directory following provider failure or a transient/statement-specific store failure is sufficient. Permanent database outage is not required. Invalid-state/nonce/issuer/binding/provider-denial are legitimate refusal siblings; device and connection-test errors use the existing error response path.

**Selected minimum correction:** at this browser response adapter preserve server-failure classification using the error catalog's existing status/code and `WyrdErrorResponse`, with no-store and the consumed transaction cookie clearing. Continue the existing tenant login redirect for genuine credential/transaction refusals. Do not change the lifecycle enum, auditing, one-use state, directory trust, issuer validation, provider work, or add a retry. The adapter is the correct boundary: it alone currently chooses a user-facing refusal for every typed error.

**Closure:** extend the existing browser `pg_tests` owner with healthy known tenant directory and failed completion inputs for `AuthVerifyUnavailable`, `DiscoveryUnavailable`, and genuine invalid authority. Assert existing 503/code and no refusal redirect for unavailable cases, no session cookie, cleared login cookie and no-store; refusal retains its redirect. An HTTP callback with a seeded browser transaction and provider `temporarily_unavailable` is an existing deterministic end-to-adapter path if used instead; no outage infrastructure is needed. Select one focused proof boundary, not both duplicate suites. For a new named test use exact nextest selection as shown below after confirming its registered name.

### FIND-TASK-001-6 — CONFIRMED — INCORRECT: failing container proof skips common cleanup

Discovery: SYS-R1-002. Obligations: R1 guaranteed fixture cleanup, existing lifecycle preservation, REQ-005. Location: `scripts/server/test-startup-identity.sh:16`; owner `startup-common.sh::cleanup`.

Common startup enables `set -Eeuo pipefail` and owns app/Postgres containers, volumes, network, temporary directory, image tag and failure logs. Identity replaces its EXIT trap with forwarder deletion, `(exit "$status")`, then cleanup. On any post-installation nonzero exit, the status-restoring subshell fails under errexit, ending the trap before common cleanup. A failed docker forwarder removal can abort even earlier. The outer provider wrapper removes only Keycloak/Dex, not these resources. Success conceals the defect; retained app/grpc port/resources can also break the next run. The stub proof described above reproduces this exact shell behavior.

**Selected minimum correction:** keep common cleanup as the sole app/fixture lifecycle owner; have identity invoke it unconditionally after best-effort forwarder removal, passing the original status explicitly. Make common cleanup accept that optional status while retaining its existing no-argument EXIT callers. Preserve failure-log collection and final original exit status. Do not add another cleanup framework or controller, and do not place a failing status-restoring command before cleanup under errexit.

**Closure:** a small assert-based shell probe with stub Docker calls must exercise actual trap/common cleanup wiring after setup for deliberate exit 7 and success 0, including failed/absent forwarder removal. Assert both cleanup responsibilities and original status. No containers/image build are needed for this local control-flow proof. Run `bash -n` on the touched scripts. The existing Linux container identity leaf remains separately required deployment evidence.

### FIND-TASK-001-7 — CONFIRMED — DRIFT / VIOLATION: prohibited test-only fixture setting

Discovery: INV-R1-002. Obligation: INV-004's explicit prohibition on any new environment variable solely for tests. Locations: `scripts/server/test-startup-identity.sh:61`, `tests/integration/identity_ui_e2e.rs:703–717`.

The script produces serialized nginx fixture JSON in new `WYRD_UI_NGINX_JOURNEY`; the exact nginx integration test is its only reader. It has no production consumer. The same test owner already sends native fixture JSON through `WYRD_UI_JOURNEY` to its existing browser driver. These are separate exact test invocations, so they can use the existing fixture transport without a mode flag or collision. This is an explicit, deletable configuration violation, not a security leak claim.

**Selected minimum correction:** use established `WYRD_UI_JOURNEY` for the nginx fixture and update its reader/docs; delete the new setting completely. Keep fixture contents, public nginx origin, provider journey, restart assertions and image/source provenance intact. No alias, selector variable, new file protocol or generic fixture controller.

**Closure:** source search leaves no `WYRD_UI_NGINX_JOURNEY` producer/reader/docs. Confirm the focused nginx leaf consumes unchanged fixture content through the existing transport; its actual Linux run remains required, not an inferred pass from renaming. The exact scenario is `identity_ui_e2e::production_ui_nginx_journey`, already registered by the leaf below.

### FIND-TASK-001-8 — REVISED — INCORRECT: selected logout can mutate a relabeled different-tenant session

Discovery: DATA-001. Obligations: REQ-033 selected tenant session only; cookie labels never establish membership. Locations: HTTP `crates/wyrd/wyrd-server/src/auth/browser.rs:498–500`, lifecycle `crates/wyrd/wyrd-auth/src/browser_sessions.rs:288–301`.

Present Beta's possessed live value as `wyrd_session_acme=<Beta value>` with same-origin logout body selecting Acme. Handler matches the untrusted label and calls `sessions.logout(value)`. `cookie_key` parses Beta's tenant from that value; lifecycle opens Beta's TenantConn and revokes Beta's exact digest. RLS correctly protects Beta's transaction but cannot correct the earlier wrong route selection. Response clears Acme's label while Beta becomes revoked. No other user's unknown session can be revoked without possessing its value; the concrete violation is a selected Acme operation mutating a supplied Beta session.

The HTTP handler is the lifecycle logout's sole production caller; SQL revoke is called only there and correctly uses RLS/digest. Node root logout uses selected label/value and public native forwarding reaches the same handler after FIND-3 is corrected. Enumeration `live_view` already compares authoritative tenant with label; recovery `recover_in` already resolves the selected route through `WyrdPostgres::resolve_tenant_slug`. Neither read owner nor SQL needs a new tenant filter.

**Selected minimum correction:** pass selected TenantSlug to the existing logout lifecycle operation and reuse the route resolver used by recovery to compare it to `cookie_key`'s tenant before opening the revocation transaction. Malformed, unknown or mismatching values revoke nothing and still allow idempotent cookie clearing; store failures retain unavailable behavior. Do not perform current principal/connection authorization merely to log out, because that would prevent safe logout of otherwise dead authority. Preserve API keys, other sessions and valid same-tenant logout; no new persistence or locking contract.

**Closure:** extend existing `pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable` with live Acme/Beta values, relabel Beta under Acme, logout Acme and prove Beta still resolves under its correct label and Acme's independent session survives. Existing correct logout, repeated/dead logout, key-survival and outage behavior remain. Use the exact registered Postgres command below. Current mislabeled-value assertions cover enumeration only, not this mutation.

### FIND-TASK-001-9 — REVISED — VIOLATION: finite new-item and delivery semantics rustdoc gaps

Discovery: REP-001, FUP-DOC-001. Obligations: AGENTS.md §16 (including private and test items, fallible `# Errors`, relevant cancellation/partial progress/retry); agent-rules hard rustdoc gate. Missing rustdoc remains blocking under the task-review skill even without an inferred runtime defect.

Locations in `crates/wyrd/wyrd-auth/src/browser_sessions.rs`: new `Debug::fmt` at 71, 88, 106; undocumented `mod tests` at 486; durable `begin`, `recover`/linked `recover_in`, logout. In server `src/auth/browser.rs`: undocumented `mod tests` at 511 and login/recovery/logout/signed-in response delivery boundaries. All cited items are new in the cumulative candidate.

The three formatters omit sensitive/state details intentionally but have no item docs or Errors section. Inherited Debug docs explain ordinary formatter failures; they do not exempt these items from the explicit local requirement or document their selected redaction. Test function docs do not document the module item. `begin_bound` already explains persisted state/five-minute expiry/pre-commit cancellation; `create_session` already names caller-owned commit; callback `execute` already explains spent state and new login. Credit those rather than demanding duplication. The remaining durable gap is commit/undelivered-response ambiguity: recovery can commit a digest before its raw cookie is delivered; logout can commit revocation before clearing reaches the browser; begin can commit expiring state without delivering its binding. Their safe retry consequences are not explained in existing linked prose.

**Selected minimum correction:** docs only in those two new browser files. Add substantive short docs to the three formatters (including propagated formatter errors) and two test modules. Explain the missing durable-delivery/retry outcomes once on the lifecycle owners, retaining links from wrappers; a common server browser-module delivery account with links from affected handlers is sufficient. Lost raw cookie values cannot be reconstructed from digests; retry recovery creates a fresh session; retry same logout is safe; new login can replace undelivered initiation. Do not promise rollback around an uncertain commit. No idempotency keys, runtime retry, reconciliation, persistence, timeout, custom formatting helper or unspecified wider rustdoc sweep.

**Closure:** source inspection against the finite items/links and normal Rust formatting/lint checks for the actual write set. No runtime test is necessary to prove documentation. This resolves the follow-up conflict without turning maintainer preferences into another round.

## Prior IDs and honest closure

| Prior ID | Current disposition |
|---|---|
| FIND-TASK-001-1 | **Implementation cause closed.** `check_payload` is shared by extraction, cached pre-activation admission and active lookup. Nonempty adapter modules/generated server/client content/module metadata/dependency presence reject the identified sentinels. Invalid cache refuses before activation with useful removal recovery; healthy reuse remains. R1 records focused preservation/selection proof. Static checks do not prove arbitrary JavaScript or an actual signed source-free runtime: AC-001 runtime/digests remain unverified. Do not reopen the same eligibility cause or demand install-time Node/import resolution. |
| FIND-TASK-001-2 | **Executable gap addressed; execution closure pending.** Actual official nginx provider scenario and setup leaf now exist, including callback/session use/refusal/restart/logout and image provenance. It was refused before build on macOS; no image/public-origin execution result exists. Preserve this ID for the outstanding focused Linux proof, not a new invented missing-test finding. New cleanup/env defects are separately bounded above. |

Prior SPEC_REVISION_REQUIRED is preserved as a historical verdict; revision 10 resolves that architecture conflict. No present correction requires resurrecting it. Source-free native runtime and human inspection are distinct acceptance obligations, not extra source findings.

## Focused proof recommendations

These commands were selected from actual repository owners. They are not a mandate to rerun broad suites, a release matrix or the repository gate in remediation. New test names below are explicitly planned and must be confirmed with nextest list before reporting execution.

```sh
# FIND-3/4: existing UI owners and their added cases
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/upstream.test.ts src/lib/server/auth/browser-sessions.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check

# FIND-8: extend this registered exact test
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_browser_sessions::browser_session_authority_is_tenant_bound_and_revocable)'

# FIND-5: planned adapter regression in existing browser pg_tests owner
WYRD_LOG=info mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=auth::browser::pg_tests::callback_failures_preserve_unavailable_classification)'

# FIND-6/7: syntax after the small stubbed cleanup behavioral check
mise exec -- bash -n scripts/server/test-startup-identity.sh scripts/server/startup-common.sh
rg -n WYRD_UI_NGINX_JOURNEY scripts crates/wyrd/wyrd-server/tests

# Existing native journey owner, if changed for FIND-3 direct public proof
WYRD_LOG=info mise run test:identity:ui

# FIND-2 execution closure and FIND-6/7 actual deployed fixture: Linux only
WYRD_LOG=info mise run test:identity:ui:container

# Normal checks for the Rust remediation write set, including docs
mise run fmt
mise run lints
```

The search should find no occurrence of the deleted setting; its no-match exit is expected, not a test failure to suppress. The container leaf already invokes the exact `identity_ui_e2e::production_ui_nginx_journey` selector under managed providers; a bare nextest command without its fixture does not prove deployment. Record immutable image ID, source revision and public origin from the successful Linux run. A planned test command is not existing executable proof; use its actual registered name once implemented, and do not report it as run now. UI metadata and shell probes need no live service; callback/logout require only repository-managed Postgres, not the whole identity matrix. Preserve trace-first diagnosis and existing failure assertions.

## Reconciled acceptance and verdict recommendation

| Obligation | Reconciled result |
|---|---|
| REQ-001/002, AC-001 production payload, signed install and eligibility | Source producer and corrected eligibility supported; FIND-1 implementation closed. Real signed source-free extracted native runtime/digests on three targets remain unverified. |
| REQ-003, AC-002 native lifecycle | Existing owner extends both children, prerequisites/readiness/settlement; supplied Mac fixture/process results are bounded proof. Linux native completion remains unverified. |
| REQ-004/006 native browser workflow and public API | FAIL: FIND-3/4/5/8. Existing SSO/recovery/settings page happy-path results do not exercise these direct/error/mismatch paths. |
| REQ-005, AC-004 official deployment | Coherent source topology and executable provider proof exist; FAIL in cleanup FIND-6 and scope FIND-7; actual image/provider/restart execution remains unverified under prior FIND-2. |
| REQ-033 ownership, digest/RLS/time, correlation, current authority, API boundary and replicas | Supported in inspected owners/supplied tests except retained failures. No cookie bearer widening, new secret or duplicated provider verifier is required. Native replicas/fresh BFF do not prove restarting the same Rust process; written container restart proof is unexecuted. |
| REQ-033 metadata/unavailable/tenant-only logout | FAIL: FIND-4/5/8; no fallback or unavailable-classification acceptance can be inferred from green happy paths. |
| INV-003–006, AC-005 existing owners and exclusions | Existing owner reuse and no deferred domain/analytical integration supported; prohibited test-only setting is FAIL FIND-7. |
| AGENTS documentation | FAIL FIND-9, finite correction batched with runtime fixes. Other reported wording/style notes remain non-blocking. |
| REQ-032 retained/removal scope | No independently validated removal defect. Retained behavior inherits auth corrections; human responsive/light/dark/native/nginx inspection remains absent. |
| REQ-020, INV-007, AC-010 and final evidence | Preservation supported. Missing RED capture for S2–6 and incomplete historical diagnosis evidence remain documented limits; do not backfill imaginary runs or weaken tests. Human acceptance and selected native/image evidence are not certified. |

All required discovery and focused follow-up reports exist, and source resolves their conflicts. Thus **FIX_REQUIRED**, not BLOCKED for unavailable discovery or SPEC_REVISION_REQUIRED. A later PASS still needs the retained implementation findings closed plus credible required execution/human evidence. Bounded remediation must not repeat a broad platform matrix: retain missing wider evidence honestly for its owning acceptance stage.

Final HEAD check: `2741395a33d5e1e5ee5d7022916627917254a663`. Only this report was written by this validator; preserved trace edits and candidate source remain untouched.

Skipped builds, live services, broad suites and human inspection. Risk: source-proven fixes and required Linux/native/human acceptance are still outstanding.
