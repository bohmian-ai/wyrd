# TASK-002 R12 repository-standards review

## Subject and authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`.
- Original base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`.
- Candidate: `2d57a6605da9bc1cee61140d7c09742cc4636efa`.
- Latest remediation diff: `bae424cc647dad4be80e7debec976d0b7b3e4cf8..2d57a6605da9bc1cee61140d7c09742cc4636efa`.
- Read `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/README.md`, the applicable spec-driven, Rust, testing, and architecture-pattern references, and `architecture/wyrd-security-posture.md`. `.codegraph/` is absent. Inspected the complete cumulative changed-file inventory and the R11 source diff, current implementation, SQL-capability types, callers, documentation, test, and boundary check. Candidate remained at the stated commit.

## Authority coverage

| Changed surface | Applicable authority | Result and source evidence |
|---|---|---|
| `wyrd-spec` auth wire types, schemas, shared verifier, client auth, CLI, public docs | `AGENTS.md` §§2–4, 8–9, 11–12, 16; `wyrd-design.md`; `wyrd-doctrine.mdx`; `wyrd-security-posture.md`; reference router contract/server/client/error guidance | PASS on cumulative source and recorded codegen, lint, and journey evidence; R11 introduces no contract edit. ID-token and workload profiles remain in their respective verifier paths. |
| `wyrd-auth` human connection, login/callback, refresh/revoke, resolvers, server auth handlers | `AGENTS.md` §§4–6, 9, 11–12, 16; agent rules on audit, SQL capabilities, RLS, rustdoc, SSRF, tests; security posture; Rust/testing references | PASS. `connections.rs:469–473` enters `begin_locked` before keyring refusal and uses existing `commit_refusal`; `pg_resolvers.rs:1–17` accurately documents `wyrd-auth`, `WyrdPostgres`, and tenant RLS. Handler tracing remains present. |
| `wyrd-sql` auth/slug queries, migration, Postgres owner and SQL integration tests | Agent rules on `TenantConn`, `OperatorPool`, tenant RLS, transaction ownership, pool construction; `AGENTS.md` §§3, 11–12; architecture patterns | PASS on changed production paths. The sole public slug resolution owner is `WyrdPostgres::resolve_tenant_slug`; auth, boot, and token exchange call it. SQL queries use the scoped capability; recorded SQL checks and tests pass. |
| Server audit plus Bifrost, cards, admin, gateway and query audit callers | Agent rules and `AGENTS.md` §§2, 9, 11–12, 16 on one canonical audit append, transactional decisions, scoped SQL, gateway exception; security posture and architecture patterns | PASS for changed production audit path. `record_audit(&ValaPostgres, ...)` gets a `TenantConn` through `ValaPostgres::tenant_conn`; `record_audit_owned(ValaPostgres, ...)` delegates to the same append on a spawned task. All 14 call sites use that owner. Gateway keeps its tracked background task and failure metric. |
| `identity_e2e`, SQL tests, gateway test fixes, test fixtures | `AGENTS.md` §11; agent testing rules; testing reference | PASS from recorded exact focused keyless/rotation results and 27/27 identity, principals, gateway, Bifrost journeys; R11 adds the keyless staged-row and no-promotion assertions without new test machinery. |
| `scripts/check_tenant_isolation.py`, pool allowlist, `mise.toml`, change packets, skills, docs | `AGENTS.md` §§1–2, 11–16; agent boundary and generated-artifact rules; spec-driven/testing references | PASS. Existing tenant-isolation check now includes server audit and rejects raw `PgPool`, `.app_pool()`, `.vala_pool()` there; construction allowlist stays separate. Implementation record says an injected raw signature failed and the restored check passed. Latest diff adds no new checker. |

## Rule results and R11 closure

| Rule | Result | Evidence |
|---|---|---|
| Every evaluated permission decision is appended through the canonical audit transaction; failure closes | PASS | `HumanConnections::activate` calls `begin_locked` first; missing key goes through `commit_refusal`. Focused Postgres test reads exactly one redacted allowed staging row and no Active connection. |
| Only scoped tenant or operator SQL capability crosses production auth/audit/query boundaries | PASS for R11 scope | No raw pool in changed audit signatures or their production callers. `record_audit` uses `ValaPostgres::tenant_conn`; `check:tenant-isolation` scans `src/audit/`. `WyrdPostgres::resolve_tenant_slug` remains the one slug owner. |
| One audit append and publisher, no duplicated machinery | PASS | Both standalone audit forms call existing `append_audit`; owned form calls `record_audit`. No new sink, writer, or publisher. |
| Rustdoc, top-level imports, struct ownership, async IO boundary, handler tracing | PASS | Corrected resolver header and changed audit/activation item docs describe owner, transaction, errors and side effects. Imports remain at module top. No new owner/trait or uninstrumented handler. |
| Test placement and verification gates | PASS with recorded evidence | Focused keyless and rotation tests, `test:identity:journey` 27/27, `test:principals:integration`, `test:gateway:native`, `test:bifrost:journey:server` 16/16, `check:tenant-isolation`, `check:from-pools-allowlist`, `fmt`, and `lints` reported green. Fresh cumulative `git diff --check` passed in this review. |

No material repository-rule finding in the R11 remediation or cumulative TASK-002 acceptance boundary. The unchanged `components/eval/resolver.rs::resolve_card_for_tenant(&PgPool)` is an existing raw-pool violation without a production caller; this PASS does not authorize it. Existing `ServerPostgres::vala_pool()` remains reachable outside the changed audit path and likewise is not proof of repository-wide raw-pool elimination. The R11 diff uses neither in production audit. Review did not rerun Postgres, Cargo, or provider tests; it inspected committed source and the implementation's recorded results.

**Overall: PASS.**
