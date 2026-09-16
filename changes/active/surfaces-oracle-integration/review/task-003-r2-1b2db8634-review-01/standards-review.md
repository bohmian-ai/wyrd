# Repository standards review — TASK-003-R2 cumulative candidate

## Subject and method

- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`.
- Candidate: `1b2db8634bf83c03ba210ebf55700983dd9091e6`, tree `52b3fe3243df7f74ba793eaa5be6caa84fba78ad`.
- Scope: complete cumulative diff, approved specification revision 9, original TASK-002/004/003 and R1/R2 remediation packets. I inspected changed source, manifests, generated surfaces, workflows and recorded verification. This is a repository-rules audit, not task-acceptance review. No `.codegraph/` index exists.
- Unrelated dirty `changes/active/verified-change-contract/` files are outside the candidate and were not touched. `git diff --check 861f8d86..1b2db8634` passed.

## Authority coverage

| Changed surface | Applicable authority | Source/evidence inspected |
|---|---|---|
| Shared client, Rust/Python/TypeScript SDKs, PyO3 and generated declarations | `AGENTS.md` §§2–9, 11–12, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/references/architecture/patterns.md`; `architecture/references/languages/{rust-core,pyo3-boundaries,python-api-and-stubs,typescript-guide,errors,testing-workflows}.md` | `wyrd-client` transport/cards/Bifrost and manifests; SDK roots, registration, exports, typings, tests; `codegen:check`, Python and TypeScript journey evidence |
| Server HTTP/gRPC/MCP, query deadline, identity, tenancy and audit | Same common authorities; `architecture/wyrd-security-posture.md`; `architecture/bifrost-design.md`; `architecture/references/domain/{vala-architecture,olap-serving,analytical-operations-reliability}.md` | Edge stack, query route/service/forwarding, auth/audit paths, query journey, typed errors/OpenAPI |
| Bifrost Scribe/Oracle/Forge, catalog, SQL migrations and storage | Same common/security/Bifrost authorities; `architecture/references/domain/{iceberg,analytical-operations-reliability}.md`; `architecture/operations/` for deployment/recovery | Changed Vala/SQL/storage owner files, Forge barrier and test, data-root boot, storage emulator/cloud lanes |
| CI, build, test, docs and generated artifacts | `AGENTS.md` §§11–16; `architecture/agent-rules.md`; `architecture/references/languages/{spec-driven-development,implementation-execution,testing-workflows}.md`; `architecture/references/README.md` router | `mise.toml`, GitHub Actions/scripts, `Cargo.toml`/lock and workspace-hack, docs/self-hosting, schemas/OpenAPI/stubs, R2 evidence table |

## Rule results

| Applicable rule | Result | Evidence |
|---|---|---|
| Server owns durable behavior; `wyrd-client` is the sole shared SDK-facing client; client tier avoids SQL/cloud/analytical engines | PASS | Cumulative client relocation and thin SDK packages use `wyrd-client`; server and Vala retain durable query/storage/Forge ownership. `check:client-tier` is included in the reported gate. |
| PyO3 aggregation and public Python/TypeScript projection, generated stubs/declarations and error codes | PASS | Python wrapper/package and TypeScript native/package roots are under `sdks/`; runtime journeys and `codegen:check`, `py:typecheck`, `ts:napi:check` are represented in the reported gate and named journeys. No parallel durable SDK implementation found in the reviewed seams. |
| Server authentication, tenant isolation, audit and error mapping; bounded query pre-Oracle admission | PASS | `apply_protected_edge` retains request ID, panic mapper, error mapper, load shedding, concurrency/body controls; `QueryAuthority::admit_capability` precedes handoff; `EdgeTimeout` still returns `Elapsed` through the existing mapper. Oracle owns the post-handoff deadline. Reported server journey exercises both timeout owners. |
| Storage configuration/workflow alignment, local and cloud proof | PASS | Storage selector uses `WYRD_STORAGE_URL`/optional endpoint; cloud workflow is push-to-`main` only and retains three OIDC jobs. R2 evidence records nonzero S3/GCS/Azure local selections on `117f668f6` and docs/CI/codegen/gate checks. No weekly cloud schedule remains. |
| Test integrity, named journeys, feature policy and generated artifacts | PASS with verification limit | R2 retains the Forge shutdown-only assertion, moves Azure dispatch proof into the Azurite owner, and selects the local handle test. The reported full gate, `test:shared`, server journey and language journeys all ran at source commit `117f668f6`/tree `e97a1e0a`; evidence commit `1b2db8634` changes only the R2 Markdown. I did not repeat the 25-minute gate or credentialed cloud tests, and the review packet contains summarized results rather than raw runner logs. |
| No gate circumvention, foreign-runtime test substitution, legacy compatibility route, or checked-in secret | PASS | Cumulative diff retains assertions and test tier routing; new server journeys are intentionally `#[ignore]`-gated and reported as run in their owning journey lane, not disabled to pass. No added `#[allow]`, second public query deadline, or checked-in `mise.local.toml`. |
| Required struct-centered Rust ownership and narrow async boundary | PASS in inspected owners | `HttpTransport`, `QueryAuthority`, `EdgeTimeout` and Forge observer own their corresponding state/IO; no new single-implementation trait or language-specific durable owner in the R2 corrections. |
| Rustdoc on every new/materially modified Rust item, including associated types, and `# Errors` on every fallible method (`AGENTS.md` §16; `architecture/agent-rules.md`) | **FAIL** | New `EdgeTimeout` implementation at `crates/wyrd/wyrd-server/src/http/middleware/edge_timeout.rs:59–103` lacks rustdoc on four associated types, and its two fallible methods have no `# Errors` section. See `STD-R2-1`. |

## Material finding

### `STD-R2-1` — new protected-edge service violates mandatory Rustdoc contract

- **Rule:** `AGENTS.md` §16 and `architecture/agent-rules.md` require rustdoc for every new Rust item, explicitly including associated types, and a `# Errors` section for every fallible function. Missing rustdoc is a hard pre-merge blocker even when tests pass.
- **Location/evidence:** `crates/wyrd/wyrd-server/src/http/middleware/edge_timeout.rs:60` (`Layer::Service`), `:89–91` (`Service::Response`, `Error`, `Future`), `:93–96` (`poll_ready`), and `:98–103` (`call`). The associated types have no documentation. `poll_ready` returns `Poll<Result<…>>` and can propagate an inner readiness error; `call` returns a future whose `Result` can be an inner service error or `Elapsed`, yet neither method documents `# Errors`.
- **Consequence:** The newly introduced protected-edge behavior does not meet the repository's explicit maintainability/completion standard. Compile, lint and runtime checks do not enforce this prose requirement.
- **Testable correction:** Document each associated type's role and add concise `# Errors` sections naming the propagated readiness/inner-service failure and edge-expiry `Elapsed` respectively; keep behavior unchanged. Source inspection plus `mise run fmt`/`mise run lints` is sufficient for this documentation-only correction.

## Result

**FAIL** — one bounded repository-rule finding (`STD-R2-1`). This does not assert a functional timeout failure. Verification evidence was not rerun because the candidate is immutable and disk space is limited.
