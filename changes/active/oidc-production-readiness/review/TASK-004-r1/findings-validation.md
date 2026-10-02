# TASK-004 R1 structured Ponytail validation

## Immutable subject and validation scope

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Authority: approved OIDC production-readiness spec revision 7, TASK-004,
  the three supplied TASK-003 human directions, `AGENTS.md`, agent rules,
  Wyrd design/doctrine/security authority, and applicable language references.
- Discovery inputs: both task reviews, standards, maintainer, system,
  security, concurrency/durability, data/tenancy, and focused follow-up reports
  in this directory.

`HEAD` resolved to the candidate before and after validation. `.codegraph/` is
absent, so validation used the complete cumulative diff, `rg`, and direct
source/caller inspection. The ledger below validates every proposed finding;
matching reports are evidence sources, not votes.

## Deduplicated validated ledger

### FIND-TASK-004-1 — Tenant selection does not bind every credential tier

- **Discovery sources:** `BEH-001`, `INV-REV-001`, `SYS-002`,
  `SEC-TASK-004-2`, `DATA-TEN-001`.
- **Validation:** **CONFIRMED**.
- **Classification:** INCORRECT.
- **Violated obligation:** TASK-004 requires a supplied tenant selector to
  constrain every selected authority at exchange/authorization, fail on
  mismatch, and never fall through. REQ-012 and REQ-015 require the intended
  server/tenant identity and prohibit cross-tenant substitution.
- **Exact source:** `crates/shared/wyrd-client/src/config.rs:194-212`,
  `crates/shared/wyrd-client/src/transport/credential.rs:232-250,271-286,310-325`,
  and `crates/shared/wyrd-client/src/auth.rs:520-552`.
- **Producer-to-consumer proof:** `ClientConfig::resolve_credential` carries
  `self.tenant` only into workload environment resolution and returns explicit,
  access-token environment, API-key environment, or credentials-file
  authorities without retaining a constraint. `workload_token_from_env`
  prefers ambient `WYRD_TENANT` over the constructor selector. The resulting
  `ResolvedCredential::{BearerToken,ApiKey}` has no selector; direct bearer use
  and API-key exchange never compare the issued authority with the requested
  tenant. The shared result feeds `WyrdClient`, CLI client assembly, Bifrost
  scope, HTTP/gRPC auth, and therefore all three SDK projections. Saved-login
  selection and saved refresh already enforce their tenant and are sibling
  behavior to preserve, not a substitute for the missing checks.
- **Observable consequence:** a caller selecting tenant A can supply or inherit
  valid tenant-B authority and perform an operation in B. Server tenant
  isolation remains intact, but the public selector's confused-identity safety
  contract is violated.
- **Smallest safe correction:** keep the approved precedence and the shared
  Rust owner. Carry the one configured selector with the resolved authority,
  prefer it over ambient workload routing, and compare it with the Wyrd tenant
  before the first application request: reuse the existing Wyrd-token tenant
  claim reader for bearer/issued tokens and the existing exchange paths for
  API-key/workload credentials. Mismatch must return the existing stable
  tenant-mismatch refusal without trying a lower tier. Do not add SDK-specific
  guards or a second credential chain.
- **Focused closure proof:** shared-client coverage for mismatched explicit
  bearer/API key, `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN` plus conflicting
  ambient `WYRD_TENANT`, `WYRD_API_KEY`, and credentials-file floor; prove no
  application request and no fallback. Retain matching explicit override and
  saved-login cases, plus one public SDK two-tenant journey proving the shared
  behavior.

### FIND-TASK-004-2 — Saved-login cache hits bypass durable generation/state revalidation

- **Discovery sources:** `BEH-002`, `INV-REV-004`, `SYS-001`, `DCD-1`.
- **Validation:** **CONFIRMED**.
- **Classification:** MISSING.
- **Violated obligation:** TASK-004 requires an in-memory cache to revalidate
  the saved record generation before reuse across processes and to observe
  newer, pending, unsafe, removed, or logged-out state fail-closed.
- **Exact source:** `crates/shared/wyrd-client/src/auth.rs:520-529` and
  `crates/shared/wyrd-client/src/saved_login.rs:300-306,364-453,573-621`.
- **Producer-to-consumer proof:** `SavedLoginSource::mint` correctly rereads the
  record under the stable OS lock, but `AuthMiddleware::bearer` returns every
  fresh cached `Renewable` token before invoking that source. The cache stores
  only bearer and expiry, while `SavedLoginSource` stores no observed
  generation. Every HTTP/gRPC consumer of the long-lived middleware therefore
  shares the bypass. Other real `AccessTokenSource` implementations are
  process-local minters and must not be burdened with saved-file semantics.
- **Observable consequence:** a running SDK process continues using cached
  authority after another process rotates, marks refresh pending, makes the
  store unsafe, or logs out/deletes the record, until access expiry or a 401.
- **Smallest safe correction:** keep `SavedLogins` as the sole durable owner and
  make only the saved-login renewable path validate the locked record's current
  generation/state before accepting its cache entry. Reuse a newer ready token;
  reject pending, logged-out/removed, corrupt, or unsafe state. Do not disable
  caching globally, reread language-specific stores, or change unrelated
  in-process renewable sources.
- **Focused closure proof:** keep one real client alive while a second process
  advances the record, leaves it pending, and logs it out; before cache expiry,
  prove the first client observes the newer ready token and then each
  fail-closed state instead of returning its old bearer.

### FIND-TASK-004-3 — Required uncertain-timeout and lock-timeout transitions are not proved

- **Discovery sources:** `BEH-003`, `DCD-2` (the invariant review records the
  same proof limits without a separate source ID).
- **Validation:** **CONFIRMED**.
- **Classification:** MISSING.
- **Violated obligation:** TASK-004's verification contract explicitly says
  `concurrent_saved_renewal` proves an uncertain network outcome and lock
  timeout, including durable pending-before-I/O and no replay/fallback.
- **Exact source:**
  `crates/shared/wyrd-client/tests/pg_auth_e2e_against_fixture.rs:200-215,262-265`
  and the unexercised production branches at
  `crates/shared/wyrd-client/src/saved_login.rs:401-429,487-518`.
- **Reachability and evidence:** production renewal persists `RefreshPending`
  before `TokenExchange::exchange` and preserves it on client transport
  failure. The journey directly rewrites a fixture into pending, so it tests
  only the later consumer, not the transition or an ambiguous accepted request.
  No located test holds the stable record lock through `LOCK_DEADLINE` and
  observes `lock_timeout`. The implementation record itself lists both
  omissions. These are named negative-flow proofs, not speculative hardening.
- **Observable consequence:** the identity lane remains green if the pending
  write moves after network I/O, a timeout retries or restores ready state, or
  lock contention stops failing within the promised bound.
- **Smallest safe correction:** extend the existing shared-client concurrent
  journey/supporting integration proof. Drive one production renewal whose
  response becomes uncertain after the pending write (including a possibly
  accepted server rotation), then prove a later process sends no second refresh
  and does not overwrite pending. Separately hold the existing record lock past
  its production deadline and prove stable `lock_timeout` with no source
  fallback. Do not add another lock or credential-store abstraction.
- **Focused closure proof:** the exact `concurrent_saved_renewal` selector must
  observe both transitions through production code and assert request count,
  durable state, stable error reason, and absence of fallback/replay.

### FIND-TASK-004-4 — Pending refresh authority is stored raw, but the required sealing design is undecided

- **Discovery source:** `INV-REV-003`.
- **Validation:** **REVISED — SPEC_REVISION_REQUIRED**.
- **Classification:** VIOLATION.
- **Violated obligation:** TASK-004 explicitly requires the refresh token left
  in `RefreshPending` to be sealed in that record while remaining available for
  per-login logout.
- **Exact source:** `crates/shared/wyrd-client/src/saved_login.rs:79-100,328-334,401-405,558-568`
  and `crates/wyrd-spec/src/auth/secret_bearer.rs:50-53`.
- **Producer-to-consumer proof:** renewal copies the live refresh token into
  `SavedLoginState::RefreshPending`; the record writer serializes the enum
  directly, and `SecretBearer` serialization emits its raw value. Logout later
  recovers that value from either ready or pending state and presents it to the
  server so only that login's refresh chain is revoked. File ownership and mode
  checks protect access but are not cryptographic sealing. No local-client
  sealing key owner, OS credential-store contract, key recovery/rotation rule,
  or cross-platform mechanism exists in `wyrd-client`; the repository
  `SealingKeyring` is deployment-configured server cryptography and is not a
  laptop key source.
- **Observable consequence:** a crash/uncertain outcome intentionally leaves a
  replay-sensitive refresh token in plaintext indefinitely, contrary to the
  approved packet.
- **Why bounded remediation cannot be prescribed:** deleting the token breaks
  logout; storing only a hash would require a new server revocation contract;
  reusing the server keyring is unavailable to laptop clients; and choosing an
  OS keychain, local key derivation/storage, supported-platform behavior,
  migration, and recovery semantics is a new security and persistent-data
  decision. The discovery correction's claimed “existing secret-sealing
  mechanism appropriate to local saved credentials” does not exist.
- **Required authority decision and closure proof:** revise the approved spec
  to select the local sealing/key-ownership and portability contract (or
  explicitly change the requirement). The later implementation must prove the
  raw pending token is absent on disk after reload, authorized logout can still
  revoke exactly that chain, and lost/unavailable sealing authority fails
  closed without broadening logout to the User's other chains.

### FIND-TASK-004-5 — Direct `TokenExchange` callers allow remote cleartext CLI secrets

- **Discovery source:** `SEC-TASK-004-1`.
- **Validation:** **REVISED**.
- **Classification:** VIOLATION.
- **Violated obligation:** TASK-004 requires the handoff, claim, returned Wyrd
  credential, renewal, and revocation secrets to cross the client/server
  boundary over TLS. The existing transport policy permits cleartext only on
  loopback.
- **Exact source:** `crates/shared/wyrd-client/src/auth.rs:143-168`,
  `crates/wyrd/wyrd-cli/src/auth/login.rs:73-80,191-220`, and
  `crates/shared/wyrd-client/src/transport/config.rs:201-229`.
- **Producer-to-consumer proof:** `LoginArgs.server`/`LogoutArgs.server` accept
  any `Url`; login and logout directly construct `TokenExchange`, whose
  constructor builds Reqwest without invoking `HttpConfig::validate`. The same
  direct constructor also serves CLI refresh and platform/test callers.
  Ordinary `WyrdClient::with_config` is narrower than the discovery report
  claimed: its later `HttpTransport::new` validates the config before any SDK
  request, so saved-login renewal through that assembled client is not a
  demonstrated cleartext path. The direct CLI handoff/claim/revoke paths are
  reachable and sufficient to retain the violation.
- **Observable consequence:** a remote `http://` CLI target can receive the
  poll verifier and return or receive renewable user authority in plaintext.
- **Smallest safe correction:** enforce the already-existing remote-cleartext
  rule once in `TokenExchange::new` (or the existing shared URL-validation
  helper it owns), preserving HTTPS and loopback HTTP. This closes every direct
  caller without repeating guards in commands.
- **Focused closure proof:** construction/use for a remote HTTP origin fails
  before a request for CLI login, logout, and renewal/exchange; HTTPS and
  `localhost`/loopback HTTP remain accepted.

### FIND-TASK-004-6 — The Python tenant projection is incomplete

- **Discovery sources:** `MAINT-TASK-004-1`, `FUP-TASK-004-1`.
- **Validation:** **REVISED**.
- **Classification:** MISSING / VIOLATION.
- **Violated obligation:** TASK-004 requires Python constructors to expose the
  optional tenant selector through public runtime and typed surfaces; AGENTS
  section 8 requires binding, public package, declarations, and tests to agree.
- **Exact source:**
  `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py:246-261,408-447` and
  `python/wyrd/stubs/{client,bifrost,state,cards,gateway,operators,verification}.pyi`
  at the constructor/describe signatures identified by the follow-up report;
  their assembled public `__init__.pyi` copies contain the same omissions.
- **Producer-to-consumer proof:** the PyO3 owners accept `tenant` and delegate
  to `wyrd-client`, but every listed source declaration omits it. The assembler
  copies those hand-authored declarations, so `codegen:check` faithfully
  reproduces stale types. Public `_BifrostBase.__init__` and
  `TableConfig.describe` additionally neither accept nor forward `tenant`, even
  though their native owners do. Root/module exports make all paths reachable;
  direct typing of the candidate journey reports the omitted argument.
- **Observable consequence:** type checkers reject valid tenant selection for
  native-backed handles, while public `Bifrost`, `AsyncBifrost`, and
  `TableConfig.describe` cannot select among same-server saved tenants at
  runtime.
- **Smallest safe correction:** add and forward the existing
  `tenant: str | None` option in the two public Bifrost wrapper entry points, update the
  existing hand-authored declaration/doc sources for every affected public
  constructor, and regenerate. Do not add a new generator or Python auth path.
- **Focused closure proof:** a small public-import typing case constructs
  `WyrdClient`, `Cards`, Bifrost, and `TableConfig.describe` with `tenant` and
  runs in `py:typecheck`; retain the runtime identity journey and
  `codegen:check`.

### FIND-TASK-004-7 — CLI refresh-chain revocation lacks canonical audit evidence

- **Discovery source:** `DATA-TEN-002`.
- **Validation:** **CONFIRMED**.
- **Classification:** MISSING.
- **Violated obligation:** the security posture classifies credential
  revocation as a security event requiring canonical audit when an
  authoritative tenant is available; Wyrd's audit rules require the one
  `vala.audit_staging` path and transactional failure for non-exempt durable
  decisions.
- **Exact source:** `crates/wyrd/wyrd-auth/src/cli_logins.rs:326-355` and
  `crates/wyrd/wyrd-server/src/auth/cli_login.rs:173-203`.
- **Producer-to-consumer proof:** the refresh JWT claim only routes; the RLS
  hash lookup establishes tenant, principal, kind, and row authority, then the
  existing family lock and `revoke_refresh_chain` mutate durable credential
  state and commit. Neither `CliLogins::end` nor its route appends an audit
  event or carries a request ID. Sibling browser logout uses the same SQL
  capability but does not make the candidate-added CLI path compliant; the SQL
  function itself is not the audit owner because it lacks operation/principal
  context.
- **Observable consequence:** successful `wyrd auth logout` revokes renewable
  authority with no attributable durable evidence, and audit failure cannot
  fail that revocation closed.
- **Smallest safe correction:** at `CliLogins::end`, after authoritative lookup
  and under the existing lock, append one redacted event through
  `append_auth_audit` in the same `TenantConn` transaction as the chain revoke,
  using the route's existing request-id mechanism. Preserve RFC 7009 no-op
  behavior for malformed/unknown tokens, per-login chain scope, and local
  tombstone/delete-on-server-failure behavior. Do not add an audit writer or
  move audit into the SQL query.
- **Focused closure proof:** a Postgres owner/route test proves exactly one
  canonical event and only the presented chain revoked on success; injected
  audit append failure rolls back server revocation, while the CLI still
  deletes the local record and warns as specified.

## Rejected proposal

### INV-REV-002 — Claim polling has no server throttle

- **Validation:** **REJECTED**.
- **Source evidence:** `crates/wyrd/wyrd-server/src/components/auth/routes.rs:40-74`
  mounts `begin`, `claim`, `cancel`, revoke, callback, and token routes behind
  one shared `tower_governor` per-peer-IP budget (`100ms`, burst 20), and the
  module documentation explicitly includes handoff polling. Pending responses
  additionally return the two-second client interval. The discovery report's
  premise that no middleware exists is false.
- **Ponytail result:** TASK-004 requires polling to be throttled, not a new
  durable per-handoff cadence table. The existing native server governor
  already supplies bounded admission. Adding last-poll state, a second
  limiter, or claim-specific persistence would duplicate an existing owner
  without an approved need.

## Required production-wheel boundary assessment

Commit `feac127a0` **strengthens rather than weakens**
`check:py-wheel-no-testing`.

The prior command ran `import wyrd.testing` in the testing-enabled editable
environment installed by its own `py:setup` dependency, so it inspected the
wrong artifact. The candidate builds a new wheel from the default Maturin
feature set (which excludes Cargo feature `testing`), passes that exact wheel
to `uv run --isolated --no-project`, positively imports `wyrd`, and then fails
if `wyrd.testing` imports. The positive control prevents an absent/broken wheel
from satisfying the negative assertion; isolation prevents the preceding
editable testing build from satisfying either import. No feature, allowlist,
or failure condition was relaxed. I independently ran
`mise run check:py-wheel-no-testing` on the immutable candidate; it built and
imported the default wheel, rejected `wyrd.testing`, and passed in 36.00s.

## Validation disposition

- Retained bounded findings: `FIND-TASK-004-1`, `FIND-TASK-004-2`,
  `FIND-TASK-004-3`, `FIND-TASK-004-5`, `FIND-TASK-004-6`,
  `FIND-TASK-004-7`.
- Retained authority gap: `FIND-TASK-004-4` requires a spec revision because
  no approved local sealing/key-ownership contract exists.
- Rejected: `INV-REV-002`.
- Overall validation result: **SPEC_REVISION_REQUIRED**.
