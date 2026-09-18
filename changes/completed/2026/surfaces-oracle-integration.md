# Surfaces and Oracle integration

- Change: `surfaces-oracle-integration` (`SPEC-surfaces-oracle-integration`, approved revision 9)
- Completed: 2026-09-17
- Reviewed base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Reviewed target: `80f92c668296b5d5374bbbf7d7aa6f2f35aa77d2`
- Delivery reference: not supplied

The integration brings Surfaces' non-Bifrost Wyrd behavior and Oracle's Bifrost implementation into one client/server codebase. Cards, registration, WyrdState, Eval, Drift, and general Wyrd contracts remain Surfaces-authoritative; Redux, Scribe, Oracle, incorporated Forge, distributed queries, and analytical reliability remain Oracle-authoritative. The conflict inventory reconciled overlapping source, tests, CI, and generated contracts rather than retaining parallel implementations.

## Delivered behavior

`vala-bifrost-redux` is the sole Bifrost engine. The server owns durable behavior and exposes Bifrost through one `wyrd_client::Bifrost` facade projected by the Rust, Python, and TypeScript SDKs. HTTP, MCP, CLI, and generated contracts use the reconciled public errors and query semantics. Server-managed Bifrost paths derive from one locked data root.

Authorization decisions use one audit staging path and one server publisher into `vala.system.audit_log`. A tenant has a monotonic watermark and at most one frozen in-flight range: a growing tail cannot change that range's batch identity, replay is deduplicated, and successful settlement advances the watermark and drains staging atomically. Oracle read-decision audit is a tracked non-blocking commit, while permission decisions remain transactionally audited and fail closed when their audit append fails.

Affected-code pull requests select the relevant lanes; the complete non-credentialed suite runs nightly. The separate live-cloud workflow runs on merges to `main`, while local `mise.local.toml` S3, GCS, and Azure tasks provide pre-merge cloud proof. No live Oracle query UI, compatibility route, second Bifrost engine, or duplicate client implementation was added.

## Lasting decisions and approved deviations

Approved revisions 1–9 established the two authority domains, the single Redux/client topology, distributed query and Forge semantics, the audit outbox and frozen-range lifecycle, and local-cloud proof with merge-to-main cloud checks. The owner expressly waived literal Git ancestry from the named rewritten Surfaces commit; content preservation and the pinned Oracle merge input remained required and were reviewed. The owner accepted same-tree gate-child equivalence and required only the exact focused test for the final test-only audit-race remediations; previously passing broader lanes and local cloud proof were retained for unchanged behavior.

No production audit behavior changed in those final remediations. Feature-gated test support made the two publisher cycles' append ordering observable. The real-server journey now observes a crash after one Scribe append, a second append of the same frozen range after that crash while settlement is fenced, one retained copy of the frozen rows, later publication of the tail, and empty staging.

## Acceptance and evidence closure

| Obligations | Evidence |
|---|---|
| Surfaces preservation, Oracle authority, and conflict resolution | Completed merge ledger and inventory; Cards, WyrdState, SDK, CLI, Python, TypeScript, and contract journeys; final integrated review. |
| Bifrost, Forge, tenancy, query deadlines, and client boundaries | Redux/Oracle/Forge sources and user journeys; scoped owner lanes, boundary checks, code generation, and the recorded broad-gate child matrix. |
| Audit staging, retained history, replay, and drain | SQL staging tests and the real-server audit publication journey; the final focused selector passed six consecutive runs with one test selected each time. |
| CI, generated artifacts, storage, and final verification | Recorded local non-credentialed lanes, generated-artifact checks, local S3/GCS/Azure tasks, and independent cumulative review with no remaining material finding. |

## Current authorities and owners

- [Wyrd design](../../../architecture/wyrd-design.md), [doctrine](../../../architecture/wyrd-doctrine.mdx), [Bifrost design](../../../architecture/bifrost-design.md), and [security posture](../../../architecture/wyrd-security-posture.md).
- [Shared client](../../../crates/shared/wyrd-client/src/lib.rs), [Redux engine](../../../crates/vala/vala-bifrost-redux/src/lib.rs), and [server audit publisher](../../../crates/wyrd/wyrd-server/src/audit/publication.rs).
- [Audit staging owner](../../../crates/vala/vala-sql/src/queries/audit_staging.rs) and [real-server audit journey](../../../crates/wyrd/wyrd-testing/tests/bifrost/server/audit_publication.rs).
