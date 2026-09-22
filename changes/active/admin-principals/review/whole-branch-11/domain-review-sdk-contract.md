# SDK and public-contract domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14
- Remediation tasks: `whole-branch-09/TASK-001-008-R9-close-validated-findings.md` and `whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
- Accepted limit excluded from review: the five-minute stateless revocation window.

## Reviewed boundary

This review traced the shared `WyrdClient` delegation owner, delegated-token cache and renewal path, secret redaction, UUID credential contracts, generated token-request schema, Rust/Python/TypeScript projections, and the public tests that claim those projections are usable.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| Shared ownership | `AGENTS.md` §§2–6, `architecture/references/architecture/patterns.md`, `rust-core.md` | `crates/shared/wyrd-client/src/{client,auth,transport/credential,transport/http,bifrost/scope}.rs` | PASS |
| Python projection | `AGENTS.md` §§7–8, `pyo3-boundaries.md`, `python-api-and-stubs.md` | `sdks/wyrd-sdk-python/src/{client,bifrost/mod}.rs`, public package exports/stubs, `tests/unit/client/test_client.py` | FAIL: SDK-1 |
| TypeScript projection | `AGENTS.md` §§2,8, `typescript-guide.md`, `errors.md` | `sdks/wyrd-sdk-ts/native/src/client.rs`, `wyrd/src/index.ts`, generated declarations, `wyrd-client.test.ts` | FAIL: SDK-1 |
| Credential UUID contract | R9 finding `FIND-admin-principals-R9-2`, typed-contract rules | `wyrd-spec::auth::{IssuedCredential,CredentialMetadata}`, shared principal/platform handles, CLI UUID parsing, served OpenAPI proof | PASS |
| RFC request schema | R10 RFC request/JWT obligations, generated-artifact rules | `wyrd-spec/src/auth/token.rs`, generated `auth_token_request.json`, codegen evidence | PASS |
| Journey proof | `AGENTS.md` §11 and `testing-workflows.md` | Rust Bifrost journey plus Python/TypeScript unit and integration inventories | FAIL: SDK-1 |

## Material finding

### SDK-1 — Python and TypeScript return a delegated client that cannot perform a delegated operation

- Classification: **MISSING / VIOLATION**
- Violated obligation: R10 requires the shared helper to be useful through every first-class SDK and requires public-surface proof; `AGENTS.md` §11 and the Python/TypeScript references require a real client→server→client journey for every new user-facing SDK capability.
- Exact locations:
  - `sdks/wyrd-sdk-python/src/client.rs:48-79` returns another `PyWyrdClient`, but that class exposes only construction and `on_behalf_of`.
  - `sdks/wyrd-sdk-python/src/bifrost/mod.rs:263-290` always constructs a new ambient/explicit client and has no path from `PyWyrdClient`.
  - `sdks/wyrd-sdk-ts/wyrd/src/index.ts:672-686` likewise creates `Bifrost` only from endpoint/credential options, while `WyrdClient.onBehalfOf` at `1024-1033` returns a private native client that no public operation consumes.
  - `sdks/wyrd-sdk-python/tests/unit/client/test_client.py:20-36` and `sdks/wyrd-sdk-ts/wyrd/tests/unit/wyrd-client.test.ts:8-24` prove only local audience rejection and an unreachable-server error; neither completes a successful exchange or spends the returned token.
- Evidence: Rust is reachable because `wyrd_client::Bifrost::query_only(&delegated)` is exercised by `query::service_b_acts_for_service_a_with_only_a_table_authority`. No corresponding Python or TypeScript API can pass its returned delegated client into the existing `Bifrost` facade, and neither `py:test:integration` nor `ts:test:integration` contains a delegation journey. The task evidence explicitly records that those journeys are absent.
- Observable consequence: a Python or TypeScript developer can pay for a successful RFC 8693 exchange and receive a `WyrdClient`, but cannot use that client to read or write Bifrost (or call any other protected capability), so the advertised helper is dead-ended in both languages and the tests would still pass if every successful result were unusable.
- Required correction: reuse the existing Rust `Bifrost::connect(&WyrdClient)`/`query_only` owner through the current Python and N-API Bifrost projections so each existing language facade can be constructed from the delegated `WyrdClient`; do not add another transport, token accessor, or language-owned exchange. Then add one Python and one TypeScript journey in their existing integration harnesses that configure B as the actor, call the public `on_behalf_of`/`onBehalfOf`, spend the returned client through public Bifrost, and prove A-authorized read succeeds while B-only write is denied before effect.
- Focused closure proof: `mise run py:test:integration` and `mise run ts:test:integration` must each select and pass a named A-read/B-read-write delegation scenario through only public package imports; retain the Rust primary journey and unit redaction/cache tests.

## Passing observations

- `WyrdClient::on_behalf_of` uses the acting client's existing `AuthMiddleware`, eagerly surfaces the first exchange failure, shares the HTTP pool, keeps the delegated token memory-only, refreshes through the same single-flight cache, and preserves the existing one-refusal retry.
- `ResolvedCredential::Debug`, `AuthMiddleware::Debug`, and `CachedToken::Debug` do not expose the subject, actor credential, or delegated bearer; the focused Rust test exercises subject redaction and memory-only caching.
- R9's credential IDs remain `Uuid` through DTOs, shared revoke methods, CLI parsing, server paths, OpenAPI, and MCP-facing contracts.
- Python and TypeScript wrappers call the Rust helper and preserve catalog-backed errors; neither duplicates exchange, cache, renewal, or HTTP behavior.

## Verification limits

- Reviewed the recorded passing format, lint, boundary, codegen, rustdoc, Rust journey, Python unit, and TypeScript unit evidence.
- Independently reran `mise exec -- uv run pytest tests/unit/client/test_client.py`: 3 selected, 3 passed; this confirms the tests pass but does not close SDK-1.
- Did not rerun the Postgres-backed Rust journey or broad repository lanes in this review-only pass.

## Overall result

**FAIL** — the Rust owner and contracts are sound in this scope, but the two foreign-language delegation surfaces are not operationally reachable and lack the mandatory public journeys.
