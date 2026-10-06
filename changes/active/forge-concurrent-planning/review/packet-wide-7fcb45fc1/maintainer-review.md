# Maintainer review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-forge`
- Original base: `c1508b375ba21a517f03ed6dd4d680dab4c3d12c`
- Prior review/remediation base: `e8d3cca13ccb799ec6dc5c69d09da3de40bffba9`
- Candidate: `7fcb45fc15ef2a43e8249af2a3dc7721fb55d517`
- Source was inspected with commit-scoped `git show` and `git diff`; the later checked-out HEAD was not treated as the candidate.

## Changed-surface coverage

| Surface | Owning implementation, callers, and proof inspected | Maintainer assessment |
|---|---|---|
| Forge leader term and promotion | `forge/leadership.rs`, `forge/scheduler.rs`, SQL leadership queries, notify/pull/report consumers, and leader/promotion tests | The term is represented by one concrete owner and one revocation token rather than another service layer; renewal, promotion, and maintenance have discoverable owners and no speculative interface. |
| Worker dispatch and shutdown | `forge/worker.rs`, dispatch/pull/report state, shutdown claim closure, server composition, and worker journeys | Claim settlement remains on the worker owner; the shutdown helper encodes one narrow terminal transition and does not introduce a second orchestration path. |
| Cleanup retention and destructive authority | `forge/cleanup.rs`, `expire.rs`, `orphan_gc.rs`, `table_authority.rs`, `vala-sql` authority/query modules, callers, and refusal/replay tests | The prepared candidate remains a domain state, while the transaction-borrowing authority is the service capability required by destructive effects; this separation makes the destructive trust boundary visible without a trait or compatibility layer. |
| Analytical query lifetime | `oracle/query_stream.rs`, `analytical.rs`, `analytical_supervisor.rs`, admission, transport, active-read acquisition/release, descendants, and lifecycle tests | The concrete `LeaderStreamOwners` and `AnalyticalGraphLifecycle` types correctly make drop order and graph revocation structural. One directly contradictory module description remains (`MNT-001`). |
| Compaction contract and SDK projections | Wire enums/defaults, `wyrd-client` re-export, Rust/Python/TypeScript public declarations, generated stubs, docs, and contract tests | One kebab-case closed enum and one documented default flow through the existing client facade; no parallel type or compatibility alias was added. |
| Python callbacks and declarations | Callback chains, loop runtime, PyO3 boundary, package exports, generated `.pyi` files, and public parity/runtime tests | Runtime outcomes and generated declarations agree: agent/model callback failure ends the run, while tool callback failure is localized to the tool result. The moved public-surface assertions preserve the deleted test's coverage. |
| In-pod worker restart and Forge metrics | `app/supervise.rs`, its three production call sites, readiness guards, Forge metric families, and focused tests | `restarting_worker` has three real consumers and owns only restart policy, so it is earned rather than a one-caller abstraction. Metrics use closed enums and the existing recorder instead of configurable registries. |
| Packet-wide standards sweep | The cumulative and remediation diffs, the 94-file rustdoc/import sweep, public and private changed items, tests, diff-check cleanup, and `scripts/checks/mocks-scope.sh` | Import cleanup and substantive item documentation generally match repository rules. The three mock allowlist entries are exact test-only seams using the check's sanctioned mechanism. The sweep nevertheless missed the stale ownership documentation in `MNT-001`. |

## Material findings

### `MNT-001` — the Analytical supervisor's module contract contradicts the enforced lifetime owner

- Location: `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs:1-9`; contradicted by `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs:2279-2295` and `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:530-561`.
- Rule: changed ownership documentation must state the actual owner and invariant; repository rustdoc is required to explain substantive behavior, not preserve obsolete architecture.
- Finding: the supervisor module still says that cleanup is not implied by the leader stream ending and that this module retains every driver and releases the graph, while the candidate deliberately removed the supervisor-held lifecycle task and makes the leader stream own `AnalyticalGraphLifecycle`, synchronously revoke followers, close exchanges, abort local drivers, and only then release the active-read claim.
- Concrete cost: this is the load-bearing R1 ownership rule stated backwards at the module entry point, so a maintainer following the module contract can reintroduce detached graph lifetime or place future cleanup on the wrong owner even though the implementation currently prevents it.
- Smallest correction: replace only the module-level ownership paragraph so it says that the supervisor indexes attempt/graph state and retains failed cleanup residue, while the leader stream is the sole lifecycle owner and synchronously revokes grants, exchanges, and local drivers before its active-read claim can release; note that only resource-envelope reclamation and already-consumerless remote I/O may continue after revocation.
- Classification: **DRIFT**; merge-blocking because it misdocuments a hard lifetime invariant, not because another abstraction or code refactor is needed.

## Calibration notes

- `LeaderStreamOwners` is justified even though it is small: explicit destruction order prevents generator layout from becoming an undeclared safety dependency.
- `AnalyticalGraphLifecycle` and `AnalyticalGraphRelease` are not duplicate owners: the former owns revocable execution, while the latter receives only residue after followers, grants, drivers, and claims are no longer reachable.
- The transaction-borrowing `ExclusiveTableAuthority` is an earned capability type because it prevents destructive work from being expressed without the live table lock; collapsing it into boolean checks would weaken the ownership model.
- The three-file wiremock allowlist is narrow, test-only, and uses the repository check's documented escape hatch; it does not broaden a boundary glob or hide production dependencies.
- The unrelated Bifrost-variant specification commit, the retained `capacity_refused` classification, and the choice to consolidate the Python public-surface test are packet-scope, domain, or test-policy questions rather than maintainability defects in this review.
- Recorded green verification supports the reviewed shape, but a green lint/gate cannot detect prose that states the ownership invariant backwards.

## Overall result

**FAIL** — the implementation uses the required leader-stream ownership shape, but the owning supervisor module still advertises the superseded detached-lifetime model; correcting that one paragraph is the smallest complete maintainer fix.
