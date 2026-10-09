# TASK-001 R3 maintainer review

**Subject:** `c46afdcac` → `437205debc628538ba6aa4ec828601c7c40145b4` in `/Users/stevenforrester/Documents/GitHub/wyrd-verification-closeout-principals`. The candidate remained at the named commit during this review. **Result: FAIL.**

## Changed-surface coverage

| Surface | Owning symbols and consumers inspected | Maintainer assessment |
|---|---|---|
| Principal persistence and contracts | `tenant_principals.rs`, `principal_directory.rs`, `role_assignments.rs`, the auth migration, issuance and login role readers, server principal integration tests | The source-specific write/read names and contract structs make the new provenance and paging paths discoverable. SQL stays with its existing tenant-scoped query owner. |
| Runtime authorization and attribution | `builtin_roles.rs`, `principal.rs`, auth issuance/refresh, Bifrost Gate's new `attribution.rs`, Scribe scope propagation, verification observation use, OTLP tests | The main authorization flow is traceable from principal scope through registry resolution to ingestion. The registry reader has a meaningful dependency-owning struct; its per-signal collectors sit with Gate. |
| Server and command surface | Principal router, removed auth grant route, `Principals` shared-client handle, CLI principal assignment command, server and CLI journeys | The `Principals` handle is the existing owner and the CLI uses it. Grant and revoke share a server write workflow while preserving distinct audit operations. Route and helper documentation explains the permission and idempotency rules. |
| Language SDKs and declarations | Python PyO3 principal wrapper, public package/stubs, TypeScript N-API wrapper and public `Principals`, Rust SDK export, Python/TypeScript/Rust principal journeys | Native wrappers delegate to the shared handle. One result field loses the existing typed CardRef contract across Python and TypeScript; see MNT-001. |
| Local client flow, tests, docs, build | Python, TypeScript and Rust gateway/OTLP adapters; local and signed-in journeys; local-development and authorization docs; touched manifests, packaging and `mise` lanes | Adapters use the client's token path and are located beside their owning SDK surfaces. The recorded verification covers the relevant lanes, but this review did not rerun them. |

The review used `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, `architecture/wyrd-security-posture.md`, `architecture/references/languages/spec-driven-development.md`, `architecture/references/languages/maintainer-style.md`, the approved spec, task, complete diff, and its recorded verification. I did not read other reviewers' reports or modify source. `git diff --check c46afdcac 437205deb` passed.

## Material finding

### MNT-001 — Principal discovery erases the CardRef type in two SDKs

**Changed locations:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts:1931`; `sdks/wyrd-sdk-python/python/wyrd/stubs/principals.pyi:50` and its assembled public declaration `python/wyrd/principals/__init__.pyi:52`.

`wyrd-spec::auth::PrincipalSummary.card_ref` is `Option<CardRef>` (`crates/wyrd-spec/src/auth/tenant_principals.rs:236`). The TypeScript principal result declares `Readonly<Record<string, string>> | null`; the Python stub declares `dict[str, str] | None`. Both SDKs already expose a CardRef type (`wyrd/src/card-types.ts:351` and Python `cards` stubs). A string map does not require `kind`, `name`, or `version`, permits arbitrary keys, and hides the typed reference needed when a caller takes a discovered Card-bound principal into a Cards operation. The TypeScript SDK's own nearby result at `index.ts:1121` uses `CardRef` for the same wire concept. The principal journeys only assert id and kind on discovery, so they do not catch this declaration mismatch.

**Governing principle and cost:** `maintainer-style.md` “Python and TypeScript: document the typed contract” requires public result types to describe the actual result; AGENTS.md §8 requires generated declaration parity. Maintainers must now infer the CardRef fields from another module or runtime data, and type checkers accept invalid references from this API.

**Smallest testable correction:** Type the TypeScript result with its existing `CardRef`. In the Python stub source, describe the returned mapping with the existing CardRef contract or a precise mapping type already used by the Python Card API, then regenerate the public `.pyi` through the normal generator. Add a focused type-level assertion or journey use that accesses the required CardRef fields from principal discovery; run the owning Python/TypeScript typecheck and codegen lanes. Do not hand-edit the assembled declaration.

## Calibration notes

The Python `GatewayAuth` flow methods omit annotations because they override both installed `httpx` and `httpx2` hooks with distinct request classes. This is an explicit compatibility choice in the task evidence; I found no concrete maintenance failure to elevate from it. The new Gate collector is larger than a typical helper, but its distinct input formats and one registry owner correspond to the actual ingest paths, so line count alone is not a finding.

**Verification limit:** I assessed source, callers, relevant tests, declarations, and the recorded green lane results; I did not independently run runtime or typecheck suites. No additional maintainer finding is asserted from that limit.
