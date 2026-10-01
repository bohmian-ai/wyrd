# TASK-002-r2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`

The candidate remained the checked-out `HEAD` throughout this validation. The
complete cumulative diff and all four Wave 1 reports were available.

## Wave 1 finding dispositions

| Wave 1 report | Proposed finding | Disposition | Reason |
|---|---|---|---|
| `task-review.md` | None | Validated empty | The task reviewer passed every mapped obligation and closed all seven prior findings. Source inspection found no contradictory task-acceptance defect. |
| `standards-review.md` | `STD-001` | **REVISED** as `FIND-TASK-002-8` | The cited omissions are present and violate an explicit hard rule. The correction is narrowed from a blanket re-documentation of the cumulative diff to the exact undocumented items independently demonstrated below. |
| `standards-review.md` | `STD-002` | **CONFIRMED** as `FIND-TASK-002-9` | All three local imports are newly added, reachable, outside both documented exceptions, and directly contradict the module-top import rule. |
| `domain-review-security.md` | None | Validated empty | The reviewed security boundary passes and prior security findings are closed. |
| `domain-review-tenancy-data.md` | None | Validated empty | The reviewed tenancy and persistence boundary passes and prior data findings are closed. |

The standards findings do not contradict the task or domain passes: they are
repository acceptance-rule violations, not defects in the tenant-login runtime
behavior.

## Independent reachability and correction-boundary validation

### Documentation omissions (`STD-001`)

- `Sha256Hex` is a public wire and persistence boundary. Its new `Display`,
  `Serialize`, `Deserialize`, `JsonSchema`, and feature-gated `PartialSchema`
  method bodies are at `crates/wyrd-spec/src/auth/oidc.rs:234-293`. Serde,
  Schemars, and Utoipa reach them through their standard traits; they are not
  dormant helpers. The serializer and deserializer return trait errors, and the
  deserializer also rejects a non-canonical digest through `Sha256Hex::new`.
- The four SQL constants at
  `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:29-64` are each used by
  exactly one complete state-transition body: insert at `:151-178`, consume at
  `:189-209`, complete at `:219-232`, and redeem at `:241-251`. Those production
  paths are called by the login, callback, and completion-redemption owners.
- `sign_id_token` at
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3081-3085` is reached by the
  callback-refusal journey's nonce, issuer, audience, signature, algorithm,
  `azp`, and successful mock-provider cases. Its two `expect` calls can panic,
  but its rustdoc has no `# Panics` section.
- The local `Mutation` alias at
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3362` shapes the immediately
  following reachable callback-refusal case table and has no rustdoc.

Adding documentation changes no caller, behavior, ownership boundary, or test
harness. No abstraction or runtime proof is justified.

### Function-scoped imports (`STD-002`)

- `Sha256Hex::digest` at `crates/wyrd-spec/src/auth/oidc.rs:213-218` contains
  `use sha2::Digest as _;`. Production callers include login-state creation in
  `wyrd-auth/src/login.rs`, callback lookup in `wyrd-auth/src/callback.rs`, and
  server callback hashing in `wyrd-server/src/auth/callback.rs`; the complete
  digest body remains a one-operation deterministic helper. Moving the trait
  import to the module import block changes none of those callers.
- `PartialSchema::schema` at
  `crates/wyrd-spec/src/auth/oidc.rs:278-293` locally imports the Utoipa schema
  builders. Utoipa reaches the complete body through the feature-gated trait.
  The imported `Schema` can be aliased at module scope to avoid collision with
  Schemars' `Schema`; no schema construction changes.
- `mount_mock_provider_advertising` at
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3045-3079` locally imports
  Wiremock matchers and response types. It is called by
  `mount_mock_provider` and directly by the unadvertised-algorithm case in the
  callback-refusal journey. Moving those names to the test module's top import
  block leaves the complete reset/discovery/JWKS/token mock workflow intact.

None is a generic function, and none is a `#[cfg(test)] mod tests` module import.
The documented exceptions therefore do not apply. The existing dependencies
already provide every name; no dependency or wrapper is needed.

## Final deduplicated finding ledger

### FIND-TASK-002-8 — Required Rust-item documentation is incomplete

- **Wave 1 source:** `STD-001`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** `AGENTS.md` section 16,
  `architecture/agent-rules.md`, and
  `architecture/references/languages/rust-core.md` require substantive rustdoc
  for every new or materially modified Rust item, `# Errors` for fallible
  operations, and `# Panics` where a panic remains. Missing documentation is an
  explicit hard blocker.
- **Exact location:**
  `crates/wyrd-spec/src/auth/oidc.rs:234-293`;
  `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:29-64`;
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3081-3085,3362`.
- **Evidence:** the new `Sha256Hex` trait methods have no item documentation;
  its fallible serialization/deserialization methods have no `# Errors`; the
  four state-transition SQL constants have no rustdoc; the signing helper's two
  `expect` calls are not documented under `# Panics`; and the local callback
  mutation alias has no rustdoc.
- **Observable consequence:** maintainers do not receive the repository-required
  contract for the canonical digest projections, login-state transition
  invariants, or test panic/type boundaries. The candidate therefore fails a
  hard acceptance rule despite green runtime evidence.
- **Decision-complete correction:** document only the cited new items in their
  existing owners. Explain each trait projection's canonical lowercase-hex
  contract and document the real serializer/deserializer error conditions;
  explain each SQL constant's transition and forced-RLS/one-use invariant; add
  `# Panics` naming invalid PEM or encoding failure to `sign_id_token`; and
  describe the local alias as the token-response mutation used by the refusal
  table. Do not change control flow, extract helpers, add lints, or document
  unrelated untouched code.
- **Focused closure proof:** inspect the cited items for complete rustdoc and
  required sections, then run `mise run fmt` and `mise run lints`. Static source
  review is the direct proof; a new runtime test or documentation checker would
  add no coverage.

### FIND-TASK-002-9 — New imports are hidden inside non-generic functions

- **Wave 1 source:** `STD-002`
- **Status:** CONFIRMED
- **Classification:** VIOLATION
- **Violated obligation:** `architecture/agent-rules.md` requires all imports
  at module top, except imports in a test submodule and the rare trait-as-blank
  import inside a single generic function. The Rust-core reference repeats the
  module-top rule.
- **Exact location:** `crates/wyrd-spec/src/auth/oidc.rs:216,281` and
  `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3052-3053`.
- **Evidence:** `Sha256Hex::digest` is non-generic; `PartialSchema::schema` uses
  ordinary concrete imports; and `mount_mock_provider_advertising` is a
  function in an external test crate rather than a `#[cfg(test)] mod tests`
  import block. None qualifies for an exception.
- **Observable consequence:** the two modules' import blocks cease to be their
  dependency manifests, violating the repository's mandatory source layout.
- **Decision-complete correction:** move `sha2::Digest as _` and the
  feature-gated Utoipa builder imports to `oidc.rs`'s top import block, aliasing
  Utoipa's `Schema` there to avoid the existing Schemars name; move the Wiremock
  matcher and type imports to `identity_e2e.rs`'s top import block. Preserve all
  three bodies and call sites exactly; add no wrapper or helper.
- **Focused closure proof:** source inspection finds no `use` in the three
  bodies, followed by `mise run fmt`, `mise run lints`, and
  `mise run codegen:check` to prove the feature-gated schema projection remains
  unchanged.

## Prior-finding closure

| Prior finding | Independently validated closure | Result |
|---|---|---|
| `FIND-TASK-002-1` | Public connection input and stored-row decode both require exact OIDC `sub`; same-email/different-subject proof remains. | CLOSED |
| `FIND-TASK-002-2` | The human callback enforces matching `azp`, including the multi-audience requirement, before identity persistence. | CLOSED |
| `FIND-TASK-002-3` | Changed role sets append one canonical role-sync event in the shared issuance transaction; unchanged sets do not, and audit failure rolls the transaction back. | CLOSED |
| `FIND-TASK-002-4` | The callback rejects an ID-token algorithm absent from fresh discovery's supported asymmetric set before persistence. | CLOSED |
| `FIND-TASK-002-5` | TenantConn state transitions rely on forced RLS without duplicate tenant predicates, with cross-tenant Postgres proof. | CLOSED |
| `FIND-TASK-002-6` | State-owner lookup is an inherent narrow `WyrdPostgres` operation over its private application pool; the callback passes only the typed hash. | CLOSED |
| `FIND-TASK-002-7` | The public login-state value stores the verifier as `SecretString`; the private decoded string is immediately wrapped and Debug proof excludes its sentinel. | CLOSED |

## Verification limits

- I inspected the complete cumulative diff, all Wave 1 reports, the complete
  bodies and use sites relevant to both proposed standards findings, and the
  prior-finding correction seams. The repository has no `.codegraph/` index,
  so inspection used Git and repository source.
- `git diff --check` passed for the immutable range. I did not rerun Cargo,
  Postgres, IdP, or documentation lanes; Wave 1 records passing focused and
  broader runtime checks. The retained findings are directly visible static
  violations, so those green lanes neither prove nor contradict them.
- No unavailable reviewer or missing source limited this validation.

## Validation result

**VALIDATED WITH FINDINGS.** The final ledger contains
`FIND-TASK-002-8` and `FIND-TASK-002-9`. Both are bounded implementation
corrections within existing owners; neither requires a specification revision.
