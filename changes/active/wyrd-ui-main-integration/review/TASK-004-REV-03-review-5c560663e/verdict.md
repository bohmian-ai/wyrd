# TASK-004-REV-03 independent task verdict

**FIX_REQUIRED**

## Immutable subject and round

Repository: `/home/thorrester/Documents/GitHub/wyrd`.
Branch: `task/wyrd-ui-main-integration/TASK-004`.
Base: `e0354f9bdf8608046833029252d8a526e0bd999d` (revision-19 approval).
Candidate: `5c560663e77ae07c33d198862ba6d795b656097a`.
Range: `git diff e0354f9bd...5c560663e`, including merged Observe Scenario 5 and its merge at a9ae7a0b9.
Approved authority: `changes/active/wyrd-ui-main-integration/spec.md`, revision 19.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md` (`TASK-004-REV-03`).

This is the first full review of this task/range, not a remediation review despite its revision-directory name. No prior acceptance or finding closure is carried forward. The complete 95-file range was reviewed; the implementer's summary and task completion claims were not accepted as proof. Candidate, branch reference and tracked source remained unchanged throughout. Only this new review directory was written. No implementation, commit, push or secret-bearing artifact was created by review.

## Independent selection and coverage

[Navigation and selection](navigation.md) records the initial map; reviewers independently expanded it. Each role was a separate fresh harness agent. All selected reports are complete.

| Role | Why selected / coverage | Independent result |
|---|---|---|
| behavior-rev | Full task requirements, V1–V12, negative flows/non-goals, current UI/reference acceptance | [FAIL](task-review-behavior.md) |
| invariant-rev | Material password/code/session/key state, transactions, concurrency and sibling writers | [FAIL](task-review-invariants.md) |
| repo-rev | Broad Rust/SQL/Python/TS/UI/generated/tooling mandatory-rule coverage | [FAIL](standards-review.md) |
| maintainer-rev | New owning workflows and cross-tier reuse/abstraction comparison | [PASS, empty scoped ledger](maintainer-review.md) |
| security domain-rev | Material auth/RBAC/tenancy/credential and browser secret boundaries | [FAIL](domain-review-security.md) |
| system-rev | KDF/shared process limits, timeout/recovery, replicas and development startup | [FAIL](system-review.md) |
| followup-rev | Only unresolved architecture-amendment/ignore-criterion interpretation and exact additional panic-doc sibling | [RESOLVED](followup-review.md) |
| ponytail-rev | Independent source/materiality/correction validation of every proposal and selected coverage | [FIX_REQUIRED recommendation](findings-validation.md) |

[Claim reconciliation](reconciliation.md) groups shared causes without treating agreement as proof. Initial reports needed no conflict-vote pass. A single focused follow-up was later selected for specific authority coverage uncertainty and separate confirmation of the validator-observed documentation sibling; the validator independently assessed all FOL proposals. No validator-originated blocker is retained without separate source confirmation. No further factual conflict or missing selected report remains.

Root/nested repository rules, spec-driven workflow, maintainer and applicable Rust/error/agent/UI/testing guidance, architecture authority map and owning credential/session/permission/tenant sections were inspected. Whole-workflow reuse was independently compared, not inferred from leaf calls. No duplicate-owner DRIFT finding survived. Non-goals remain excluded.

## Reconciled acceptance matrix

Source paths below follow the behavior report's abbreviations: UI is `crates/wyrd/wyrd-server/wyrd-ui/`; auth is `crates/wyrd/wyrd-auth/src/`. A source/test-backed PASS is limited to its actual row. Companion FAIL rows are not waived by passing named selectors. Specialist/validator findings supersede the discovery matrix where noted.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-041/AC-024: protected provision-then-enroll; no anonymous admin or caller-selected role | `main.rs::setup/create_local_admin`; principal `accounts.rs::create_user_invitation/authorize_user_admin`; `passwords.rs::password_signup` resolves invitation authority and grants its Roles | V1/V2 exercise invitation, tenant binding, viewer denial, missing/revoked/expired/reused codes and concurrent redemption | PASS |
| Atomic redemption creates no partial User/grant/session | `password_signup` performs insert/consume/revalidation/grants/session in one TenantConn and commits last; SQL `consume_invitation` is conditional UPDATE | V2 concurrently redeems one code and counts exactly one resulting User; wrong tenant leaves none | PASS |
| Tenant-local canonical username, immutable identity, no SSO auto-link | `canonical_username`; `insert_local_user`; migration username regex/tenant-local unique index, unchanged issuer+subject path | V1 canonicalizes `Ada.Lovelace`; V2 collision and foreign tenant assertions; source keeps local/SSO identities distinct | PASS |
| NFC, 15–128 Unicode characters, 1024 raw bytes, case/spaces, no controls/full blocklist | `passwords.rs::normalized/new_password/is_common`, narrow installed normalizer dependency | Inline policy/verifier tests and V1 policy refusals establish server policy | PASS |
| Password-manager autocomplete/paste, visible policy, accessible reveal, Unicode policy preserved by forms | Login/signup/reset/account password inputs; signup policy copy | No reveal control exists; HTML maxlength counts UTF-16 units, not server policy characters. BEH-2 | FAIL |
| Argon2id PHC, random salt, baseline, rehash on successful obsolete verification, no reversible store | `hash_now/check_password`; existing `password_hash`; no new password store | Inline verifier baseline/wrong/dummy tests; V1 inspects PHC storage. Rehash branch inspected statically | PASS |
| Shared fixed DB-time failure windows; admission before actual verification on login/change | Windows / existing SQL failure owner; admission currently precedes the KDF queue | V2 sequential pass; independently validated stale queued work, FIND-TASK-004-REV-03-8 | FAIL |
| Browser gets meaningful stable 429 retry information | Rust mapper emits retry seconds/header, but BFF replaces it with static empty-details example | Source producer-to-render trace; no BFF rate-limit browser assertion. BEH-3 | FAIL |
| Exactly-one local/SSO/recovery session binding, opaque cookie, fixed 12h, current tenant/User/revision | Migration authority CHECK; SQL live session revision predicate; BrowserSessions existing issuer/current-principal checks, HTTP cookie owner | V1 old local sessions fail after change/reset; existing cookie/Origin/bearer tests remain; V12 covers existing SSO/replica behavior | PASS |
| AC-024/V1–V2 required local password change/reset across replicas | Shared SQL revision authority exists, but new local tests start only one in-process server | No A→B local-session replay test exists; existing V12 crosses SSO/recovery replicas. BEH-5 | FAIL |
| Operator reset is purpose/User/tenant/revision bound, 15m, one-use, verifier-only, no automatic login; API keys independent | Reset table/code hashes and conditional consume, local User lock/revision compare and set; no key revocation path | V1 stale-after-change/current/reset-reuse/no-cookie/old-session assertions; source preserves API keys | PASS |
| Local and SSO remain real Users; SSO cannot acquire/change local password | User rows retained; local credential constraint; password change requires Password session; account UI checks authentication method | V12 genuine provider Settings test asserts no Current password; V1 local profile; no email/name linking path added | PASS |
| Every new invalid wire value uses canonical validation problem | Shared DTOs deny unknown fields, but four new password handlers use bare Axum Json extraction | Malformed/missing/null/wrong-type requests bypass WyrdErrorResponse. BEH-1 | FAIL |
| REQ-042/AC-025 ordinary personal Settings without OIDC administration | New account load uses current Rust session `/principals/me`, page credential route; identity menu points Account separately from Settings administration | V4 editor account flow; V12 genuine reader SSO account flow | PASS |
| Server-derived self ownership plus configured personal read/write capability; Service gets no self rights | `TenantTokenIssuer::resolve` adds User-only capabilities before ceiling; principal routes use `is_self`, audit::authorize and principal-owner revocation | V3 restricted key cannot revoke without write; other User cannot list; V6–V9 User-vs-Service profile/metadata/revoke cases | PASS |
| Named explicit scoped expiring key; no unrestricted User body/default; preserve Service/Admin/platform shape | `mint_user_credential`; User-target request/body checks; SQL exactly-one owner/backfill; platform projection omits new fields | V3 body/ceiling/lifetime/Service-body/no-body tests, stored owner and sorted ceiling checks; migration source | PASS |
| Existing User exchange resolves current User roles/status and intersects ceiling; later grant cannot expand; JWT semantics unchanged | Existing key lookup joins User/Service owner; issuer resolves once and intersects PermissionSet; unchanged exchange token snapshot | V3 actual User profile, later editor-role grant still same ceiling, revoke/suspension; SDK own-key use exercises public client | PASS |
| Five-minute genuine human freshness, recovery/key reuse does not refresh | SQL creation-time recent predicate; credential-aware BrowserSession.recent; middleware extension; own issuance rejects missing proof | V3 old password and recovery refusal; V4 recent password creation; V12 genuine SSO creation | PASS |
| One-time transient create result, copy/dismiss, reload/list secrecy and no-store at public secret owner | BFF action is no-store and loads are safe; direct Rust issuance returns bare Json | V4 secrecy flow passes; FIND-TASK-004-REV-03-12 validates missing direct response header. No separate action-memory leak finding retained | FAIL |
| Metadata default50/max100, stable descending tuple cursor, tenant/principal binding, no cursor on final/empty page | `credential_page`; SQL `page_api_key_metadata`; shared query/response; SDK page normalizes omitted limit and sends one request | V3 tests only three keys and a two-page split, bad target and limit101; language/MCP tests have one key | PASS |
| Explicit required >100 keys/equal timestamps/all-cursor traversal proof in V3/V6–V9; complete legacy admin lists | Same owner supports paging; source appears correct but specified stress/tie/transport proof is absent | Every new SDK/MCP test is one page; V3 creates three timestamps independently. BEH-4 | FAIL |
| Thin Rust/Python/TS profile/page projections; no SDK password/session issuance; MCP metadata/revoke only, strict me input, mutation discovery scopes | Shared Principals.me/page; native bindings/stubs/declarations; MCP shared DTOs and configured operation owner; no secret-issuing tool | V6–V9 verify User identity/profile/metadata/refusals/revoke; strict CurrentUserArgs and descriptors source | PASS |
| Keep existing exchange verification working after forward ownership migration; repair affected fixtures | New `auth_api_keys_one_owner` requires one relational owner, while five existing exchange fixture inserts still omit it | Actual existing exchange test fails before reaching its no-refresh invariant. BEH-7 | FAIL |
| REQ-043/038/AC-026 setup optional username+stdin password, same creation/hashing owner; no overwrite/default production User | Clap paired requires; `local_user/new_password/hash_password/create_local_user`; normal absent path unchanged | Static source; dev lifecycle observes seeded account | PASS |
| Ephemeral real `dev` User/random20 password, private record cleanup, explicit reset warning | dev-server.sh random generation, local setup, 0600 record and trap removal | V5 asserts private record and deletion after Ctrl-C, password length and normal login | PASS |
| Early UI/API/human readiness, fixture auth disabled, preserve default500/concurrency8/current production wheel/source | mise dev:full readiness before app setup/batch, `WYRD_UI_LOCAL_AUTH=false`; defaults unchanged | V5 logs ready before terminal batch result and logs in through password form. Test is small-run by design; normal default500 is integrated-change proof | PASS |
| App setup failure leaves UI/server available; append batch and graceful cleanup | Existing lifecycle retained, setup failure branches do not terminate ready services; unchanged app-first cleanup | V5 refused setup/readiness/owned ports, real retained signal readback, append and logout/cleanup; source reviewed | PASS |
| REQ-044/AC-027 Main shell, selected borders, palette and 56px chart baseline, p95 lime/p50 periwinkle | Canonical palette new roles; shell nav token; Observe DURATIONS; shared compact Chart padding/height and full opacity | V10 computes selected theme/colors/plot heights/desktop baselines/mobile bounds; V11 roots/spans/query/group/live/detail behavior | PASS |
| Logs compact named severity fills, detail in same records work region, Wrap/Columns/exact trace return | Observe LEVELS, .work; page aside child; RecordTable/RecordDetail reused | V10 asserts same-region geometry in both sizes and fills/opacity; V11 logs investigation | PASS |
| Metrics two cards with reusable selection/exact summaries, no default raw table, bounded authorized explicit points inspector | Chart heading/snippet retains generic callers, separate summary frame; layout conditional bounded RecordTable load; Drawer opened by URL points state | V10 checks two frames/selection/row density/default absence/inspector and Escape; V11 recorded histogram/summary/precision behavior | PASS |
| TraceDrawer 884px, 300/110/flex/64 grid/28px rows; actual identity legend-backed palette/errors; preserve actions | TraceWaterfall uses SERIES_COLORS by recorded service; unchanged generic spanTree/selected tree actions; Drawer/page retained | V10 dimensions and first recorded service/error fills; V11 tree/details/Link/Expand/Escape; unit multi-service colors source | PASS |
| All six boards independently compared externally at desktop/mobile light/dark, verified light before every capture, individual human acceptance | V10 theme preconditions and optional screenshot hook; actual Spans render is covered functionally in V11 | No supplied current external comparison set, Spans V10 capture, seeded-human six-board matrix or individual human acceptance. BEH-6 | FAIL |
| REQ-007/015–017/020/030/034/039 inherited range/predicate/current data/empty-loading-denied-error-partial semantics, keyboard/touch/long content | Shared Chart/Table/FacetRail/Drawer/Detail/TimePicker retained; no replacement query/record owner; points uses existing metric list owner | Inspected V11 real journeys and existing component state/loader/typed-field tests; these supplement but do not replace missing visual gate | PASS |
| INV-001/002/004/007: no aliases/new auth framework/Node authorizer/durable secret cache, no fake success/secret captures; source-driven generated contracts and stable errors | Same existing crate/table/session/key/issuer owners; only installed normalizer dependency; no second key format or MCP issuance; tokens generated from palette | Diff/source audit, relevant narrow verification; canonical password error exception BEH-1 remains a failure under INV-001 | FAIL |
| Non-goals: no public anonymous workspace provisioner, email delivery/linking/MFA/OAuth-password grant, altered device/refresh/machine protocol, cookie MCP/gRPC/OTLP, username rename, exemplar precision work or saved-view/panel memory | Cumulative changed path/source map; edits confined to approved account/dev/Observe surfaces and projections; existing protocol owners retained | Static diff/caller review; existing regression owner V12 retained | PASS |
| Actual KDF concurrency stays at four through cancellation | Existing bounded/spawn_blocking lifetime | FIND-TASK-004-REV-03-9; deterministic cancellation proof absent | FAIL |
| Required UI startup failure reaches existing owned cleanup | New dev:full readiness predicate | FIND-TASK-004-REV-03-13; healthy V5 is not dead-UI proof | FAIL |
| No new unconditional allow; changed-item panic rustdoc | SQL insertion and four identified tests | FIND-TASK-004-REV-03-10/11; green format/Clippy does not waive explicit gates | FAIL |
| Required owning architecture amendments are coherent | Human sign-in and First-party browser sessions retained exhaustive statements | FIND-TASK-004-REV-03-14, independently discovered and validated | FAIL |
| Direct requested no-ignore-additions criterion | Two new journey attributes | FIND-TASK-004-REV-03-15; both tests ran, no gate evasion alleged | FAIL |


## Validated finding ledger

Only independently CONFIRMED/REVISED findings are retained. [findings-validation.md](findings-validation.md) is the full authoritative ledger for exact locations, violated obligations, reachable producer-to-consumer paths, materiality, correction burden, selected correction and focused proof.

| Stable ID | Validation | Retained cause / required proof |
|---|---|---|
| `FIND-TASK-004-REV-03-1` | CONFIRMED | Password body rejection bypasses the public error contract |
| `FIND-TASK-004-REV-03-2` | CONFIRMED | Password forms omit reveal and reject valid Unicode credentials |
| `FIND-TASK-004-REV-03-3` | CONFIRMED | BFF drops the password refusal's retry information |
| `FIND-TASK-004-REV-03-4` | CONFIRMED | Required large/tied continuation proof is missing |
| `FIND-TASK-004-REV-03-5` | CONFIRMED | Required local-session invalidation proof stops at one replica |
| `FIND-TASK-004-REV-03-6` | REVISED | Required visual delivery and individual human acceptance remain open |
| `FIND-TASK-004-REV-03-7` | CONFIRMED | Migration leaves five existing key fixture writers invalid |
| `FIND-TASK-004-REV-03-8` | REVISED | Queued password work can use stale failure admission |
| `FIND-TASK-004-REV-03-9` | CONFIRMED | Request cancellation returns a KDF permit before work completes |
| `FIND-TASK-004-REV-03-10` | CONFIRMED | New SQL insertion uses a prohibited unconditional allow |
| `FIND-TASK-004-REV-03-11` | REVISED | New password tests omit mandatory panic documentation |
| `FIND-TASK-004-REV-03-12` | CONFIRMED | Direct personal-key creation response lacks no-store |
| `FIND-TASK-004-REV-03-13` | CONFIRMED | Dead UI child leaves the new readiness wait indefinitely active |
| `FIND-TASK-004-REV-03-14` | REVISED | Required architecture amendments leave incompatible exhaustive rules |
| `FIND-TASK-004-REV-03-15` | CONFIRMED | New journey ignores fail the direct literal review criterion |

The direct no-ignore constraint is retained distinctly from gate evasion: both journeys actually ran under V6/V9. It is not a claim that ignored external-service journeys are categorically forbidden by repository convention. The correction preserves existing fast/provisioned lane boundaries.

Rejected scope: metadata-only no-store expansion; strict accounting of all in-flight attempts as failures; new reservation/limiter service or sweeper; inferred visual defects or cosmetic redesign. Successful login remains uncharged; only stale queued admission must be corrected. Human visual acceptance must be real, never fabricated. No retained correction needs a new product/API/security/architecture/persistent-authority decision, so SPEC_REVISION_REQUIRED is not warranted.

## Actual verification and limits

[verification.md](verification.md) records exact commands and actual reruns. All V1–V12 selectors passed, including real local browser/dev flows, Rust/Python/TS/MCP key journeys and real Keycloak/Dex replica/SSO Account Settings. UI check/build, 16 affected UI tests, three exact password unit tests, read-only Rust/Python formatting checks, workspace/shipped-server Clippy, Python/TS types/lints, native declaration parity, codegen, dependency and tenant-isolation checks passed. The served OpenAPI module selected 23 tests; all passed. Cargo-backed commands were centrally coordinated sequentially, including build prerequisites.

One additional existing targeted test failed against successfully migrated real Postgres:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-auth --lib -E "test(=exchange_api_key::pg_tests::api_key_exchange_issues_no_refresh_token_or_row)"'
```

It panics at exchange_api_key.rs:1102, SQLSTATE23514 `auth_api_keys_one_owner`, before its intended assertion. Finding 7 traces all five sibling fixture writers to that cause. No test was weakened, ignored or deleted to make review green.

Green selectors do not establish absent large/tied continuation, local cross-replica, cancellation/queued-work or startup-failure assertions. Runtime/source limitations are explicit in the reports. The current seeded-human six-board external comparison and separate human acceptance are incomplete. Default500 dev:full remains integrated change-review work exactly as the task directs; no broad gate or unrelated load replay was added. No secret values were printed or recorded in review artifacts.

## Closure and handoff

Prior-finding closure: not applicable, first review. Two-unsuccessful-remediation convergence assessment: not applicable. Existing unaffected source-backed acceptance can carry forward on the next bounded remediation review, subject to changed dependencies and new evidence.

Route [TASK-004-REV-03-R1-auth-and-delivery.md](TASK-004-REV-03-R1-auth-and-delivery.md) directly to `$wyrd-implement`. It packages only this validated ledger, preserves approved behavior and includes bounded documentation corrections. A later review checks closure, the remediation diff and affected consumers with this complete cumulative subject retained.

Skipped: implementation changes, default500 change-review replay and unrelated full aggregates. Risks remain the validated auth/runtime/public-contract defects, red fixture gate and incomplete required proof/human acceptance.
