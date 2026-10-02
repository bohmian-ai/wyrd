# Repository Standards Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `28473e049705595306f2934cf4bc664168254086`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 10
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`
- Production candidate: unchanged from `eb22b03f2bb766886d839bda23aafbd4ba130ab3`; the later candidate commits add the r1 review record and approved revision-10 packet updates.

Overall result: **FAIL**

## Review Findings

### Critical

None.

### Important

- **STD-001 — Function-scoped imports violate the mandatory module import rule.** `crates/skald/skald-workflow/src/workflow.rs:811-816`, `:1246-1258`, `:1473-1482`, `:1692-1694`, and `crates/skald/skald-workflow/src/workflow_surface.rs:848-850` add `use` statements inside test functions. `architecture/agent-rules.md` requires imports at the top of the module, allowing test dependencies at the top of the `#[cfg(test)] mod tests` scope but not inside individual tests. Move and deduplicate these imports in the two test-module import blocks.

- **STD-002 — Required docs and example verification is absent.** `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:465` records the scoped lanes but omits `mise run docs:check` for the changed documentation and `mise run check:examples` (or every touched example task) for the changed Rust/Python/YAML examples. `AGENTS.md` §11 requires these lanes for those surfaces. Run and record both lanes; the existing codegen and language tests do not prove docs build/link integrity or example compilation.

- **STD-003 — Four named Rust tests lack exact-selector evidence.** `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:463` names `agent_run_executes_openai_responses_tool_loop`, `responses_session_turns_seed_native_items`, `messages_roundtrip`, and `message_num_untagged_dispatch_per_provider`, but records only package/target shorthand. `AGENTS.md` §11 and `architecture/references/languages/testing-workflows.md` require an exact `mise exec -- cargo nextest run --locked` command with package, target, and `-E 'test(=...)'` for every specifically named Rust test. Run and record all four exact selectors.

- **STD-004 — New PyO3 behavior is implemented in the retained migration crate rather than the Python SDK owner.** `crates/skald/skald-workflow/src/python.rs:41-75`, `:347-438`, and `:558-646` add Python conversion, authoring/validation methods, and the `PyWorkflowRun` wrapper; `sdks/wyrd-sdk-python/src/lib.rs:48` still only delegates owner-crate registration. `AGENTS.md` §§7-8 and the PyO3/Python references allow existing owner-crate wrappers to remain as migration state, but require new or materially relocated wrappers and registration to live under `sdks/wyrd-sdk-python/src`. Keep Rust-native validation/execution on `Workflow`, but move this task's new Python boundary and registration to the SDK without duplicating core behavior.

- **STD-005 — Public Python declarations erase exact Workflow result types.** `sdks/wyrd-sdk-python/python/wyrd/agent/__init__.pyi:424-438` (and the generated duplicate in `python/wyrd/stubs/agent.pyi`) exposes `steps` as `dict[str, dict[str, Any]]`, `error` as `dict[str, Any] | None`, and the complete snapshot as `dict[str, Any]`, although `WorkflowStepResult`, `WorkflowRunError`, and `WorkflowRun` are exact public DTOs. The Python/stub and maintainer references require precise public types unless input is genuinely arbitrary. Update the SDK-owned annotation/generator source to emit typed projections for those DTOs, keep only JSON-valued fields broad, regenerate, and run codegen plus Python type checking.

- **STD-006 — New observer boundary methods lack required contract documentation.** `crates/skald/skald-observer/src/composite.rs:144-174` and `crates/skald/skald-observer/src/python.rs:280-340` add six Rust implementation methods without rustdoc, contrary to `AGENTS.md` §16 and `architecture/agent-rules.md`, which require substantive rustdoc for every new or materially modified Rust item regardless of visibility. The paired public Python hooks at `sdks/wyrd-sdk-python/python/wyrd/observer.py:160-188` also omit the signature-aligned `Args` sections required by the maintainer guide. Document the concrete fan-out/bridge behavior, callback failure handling, payload omission, attempt/error semantics, and duration units in place; no new abstraction is needed.

- **STD-007 — The workflow guide states a false step-ID contract.** `docs/src/content/docs/how-to/build-a-workflow.svx:19` says step IDs are Agent names, while `crates/skald/skald-workflow/src/workflow_surface.rs:611-637` sanitizes invalid characters, prefixes leading digits, synthesizes unnamed IDs, and suffixes collisions. Repository documentation must match the typed authoring contract. Describe IDs as deterministically derived from Agent names, explain these transformations, and direct callers to the resulting `Workflow.steps` ID when a name is not already a unique identifier.

- **STD-008 — Runtime secret bindings can set routing and reserved transport headers.** `crates/skald/skald-workflow/src/route.rs:152-185` validates only binding origins and header values, then `merged_headers` at `:460-475` forwards every `HeaderName`, including `Host`, framing/hop-by-hop, forwarding/proxy, and Wyrd-internal names. This conflicts with the approved spec's pre-dispatch forbidden-header contract and external-network safety: a bound `Host` can select another virtual backend behind a screened shared reverse proxy and carry the binding's credentials there. Reuse the existing route-header classification at `ExternalGatewayBindings::insert` to reject the transport/routing/internal subset while retaining credential headers such as `Authorization` and API-key/token names for secret bindings. Prove refusal before dispatch and acceptance of ordinary credential headers.

- **STD-009 — External-gateway refusal bodies can retain reflected credentials in a debuggable error.** `crates/skald/skald-providers/src/clients/external.rs:135-149` uses the shared status handler, which stores the complete bounded body at `clients/mod.rs:61-74` in `ProviderError::Status`; the enum's derived `Debug` at `error.rs:9-30` exposes that body. A gateway that receives a secret header can reflect it in a refusal, violating `architecture/wyrd-security-posture.md`'s rule that secrets never enter logs or errors. Sanitize non-success bodies only at the external-gateway boundary, retaining status/retry metadata and leaving native-provider behavior unchanged; prove a reflected canary is absent from `Display`, `Debug`, and Workflow projection.

- **STD-010 — The immutable candidate fails `git diff --check`.** The base-to-candidate check reports `changes/active/skald-workflow-runtime/review/TASK-001-r1/findings-validation.md:269: new blank line at EOF`. The task's recorded green `git diff --check` predates the r1 review commit and does not cover candidate `28473e049`. Remove the trailing blank line and rerun the check for the immutable cumulative range.

### Suggestions

None. Every retained issue above maps to an explicit repository or approved-spec rule; no optional refactor is requested.

## Revision-10 reconciliation

| Prior finding | Revision-10 authority | Candidate evidence | Standards result |
|---|---|---|---|
| `FIND-TASK-001-3` | `ExternalGatewayBinding.secret_headers` is `HashMap<HeaderName, SecretString>` because header order is unobservable and `HeaderName` has no `Ord`. | `crates/skald/skald-workflow/src/route.rs:100-112` uses exactly that public field type. | **CLOSED** — candidate matches the approved seam. |
| `FIND-TASK-001-4` | `ProviderError::RemoteProblem(Box<RemoteProblem>)` carries a public five-field payload and keeps the enum within the large-error lint. | `crates/skald/skald-providers/src/error.rs:59-87` defines exactly the tuple variant and public payload; direct consumers destructure/pass the boxed payload. | **CLOSED** — candidate matches the approved seam. |

Revision 10 changes only these two previously unimplementable public seams. It does not waive the remaining production, documentation, security, typing, or verification rules.

## Authority coverage

The complete 127-path cumulative diff was grouped by ownership surface; every changed path is covered by one row below. No `.codegraph/` directory exists, so repository navigation used Git and direct source inspection as instructed.

| Changed surface (path count) | Applicable authority | Result and evidence |
|---|---|---|
| Active spec/task/review packet (17) | Spec-driven development, approved revision 10, task evidence rules, repository formatting | FAIL: evidence omissions are `STD-002/003`; the cumulative packet fails `git diff --check` (`STD-010`). Revision-10 seam changes themselves pass. |
| `Cargo.lock` (1) and manifests in changed crates | Dependency-cost ownership, Cargo feature policy | PASS: no new third-party dependency or wildcard/profile block; the dependency movement stays within approved Skald/Wyrd owner edges. |
| `crates/wyrd-spec` contracts/errors/schemas (17) | `AGENTS.md` §§2-4, 9; `wyrd-design`; doctrine; Rust/errors/codegen references | PASS: pure typed Workflow/Card/run/error contracts remain IO/async/PyO3-free, schema sources and generated artifacts move together, and public errors remain derive-backed. |
| `crates/shared/wyrd-client` (1) | Client-tier ownership and dependency direction | PASS: consumer adjustment introduces no durable behavior or forbidden server/data dependencies. |
| `skald-agent` (4), `skald-spec` (3), `skald-runtime` (1) | Skald ownership, provider-native wire handling, Rust/async/test rules | PASS for placement and structure; exact evidence for four named tests remains missing (`STD-003`). |
| `skald-observer` (3) | Observer ownership, Rust documentation, PyO3 boundary | FAIL: missing Rust and Python contract documentation (`STD-006`). |
| `skald-providers` (6) | Provider ownership, endpoint/secret security, Rust errors | FAIL: external refusal bodies retain reflected secrets (`STD-009`). DNS screening/pinning, no-proxy, no-redirect, TLS, and bounded IO otherwise follow the applicable external-network rules. |
| `skald-workflow` (28) | Workflow owner, struct-centered Rust, async/lifecycle, PyO3 placement, security, tests | FAIL: local imports (`STD-001`), misplaced new PyO3 (`STD-004`), and secret reserved-header admission (`STD-008`). Cohesive execution/planning owners and synchronous pure validation otherwise pass structural rules. |
| Wyrd gateway/server/testing/CLI applications (9) | Server/application ownership, typed consumers, security/audit boundaries | PASS for repository placement: these files consume Skald/spec owners and do not add a second workflow engine, route, audit path, or durable client behavior. |
| Python SDK/package/tests/examples (19) | Python SDK ownership, generated stubs, public imports, Python runtime tests | FAIL: SDK ownership and precise type projection (`STD-004/005`), observer docs (`STD-006`), and missing example proof (`STD-002`). Public imports and Python-runtime test placement otherwise align. |
| TypeScript SDK projection (1) | TypeScript generated-error projection | PASS: the error-code union follows the derive-backed catalog; no TypeScript workflow surface or parallel behavior is added. |
| Documentation (3) | Doctrine/public contract parity, docs verification | FAIL: incorrect step-ID description (`STD-007`) and missing docs lane (`STD-002`). |
| Rust/Python/YAML examples (14) | Public authoring contract and example verification | FAIL only for absent recorded example verification (`STD-002`); no legacy compatibility vocabulary or credential-bearing example was found. |

## Per-rule results

| Rule area | Result | Source evidence |
|---|---|---|
| Wyrd/Skald ownership and dependency direction | PASS | Workflow/agent/provider behavior stays in Skald, pure wire contracts in `wyrd-spec`, and Wyrd application crates consume those owners. |
| Struct-centered Rust and abstraction discipline | PASS | Stateful workflow execution, plans, routes, ledgers, bindings, and provider clients have cohesive concrete owners; no speculative trait/factory or zero-sized utility owner was added. |
| Async/runtime discipline | PASS | Validation and planning remain synchronous; async methods await provider/tool/observer IO and owned scheduling; no ad hoc runtime is added. |
| Rust import placement | **FAIL** | `STD-001`. |
| Rust documentation | **FAIL** | `STD-006`. |
| PyO3 placement and SDK aggregation | **FAIL** | `STD-004`; the retained-feature exception does not authorize adding new wrappers there. |
| Python public typing and stub parity | **FAIL** | `STD-005`; runtime/stub imports otherwise move together and generated-file drift was recorded green at the production commit. |
| Stable public errors | PASS | Derive-backed Wyrd errors remain authoritative; revision-10 `RemoteProblem(Box<RemoteProblem>)` is matched exactly. |
| Generated schemas/error projections | PASS with evidence limit | Source, generator, schemas, Python stubs, and TS code union move together; recorded `codegen:check` passed for production commit `eb22b03f2`, not the later review-only packet. |
| Secret handling and external network safety | **FAIL** | `STD-008/009`; DNS resolve-screen-pin, prohibited address classes, TLS, proxy, redirect, and response bounds otherwise pass source inspection. |
| Test placement and gate integrity | PASS | No added `#[allow]`/`#[ignore]` circumvention; Rust-only and Python-lifetime tests stay in their owning runtimes. |
| Required verification selection and exact commands | **FAIL** | `STD-002/003/010`. |
| Public documentation accuracy | **FAIL** | `STD-007`. |
| Tenant SQL, persistence, and canonical audit rules | N/A | TASK-001 adds no SQL transaction, tenant-scoped durable write, authorization decision, or audit publisher; gateway consumers are adapted without a second audit path. |
| Bifrost architecture | N/A | No Bifrost ingest, query, storage, compaction, or analytical reliability behavior changes. |
| Legacy/compatibility scope | PASS | No predecessor route, alias, migration shim, new Card kind, or remote Python/TypeScript/MCP Workflow surface is introduced. |

## Open Questions

None. Revision 10 resolves the only two prior public-seam decisions. Every remaining standards finding has a bounded correction within approved authority.

## Verification Notes

- Confirmed `HEAD` was `28473e049705595306f2934cf4bc664168254086` before inspection and again after source review.
- Confirmed `git diff eb22b03f2bb766886d839bda23aafbd4ba130ab3..28473e049705595306f2934cf4bc664168254086 -- crates sdks docs examples Cargo.lock` is empty: production is unchanged from r1.
- `git diff --check a51af030b6039eea4b2914f3ebf2c31925d08721..28473e049705595306f2934cf4bc664168254086` fails only on the r1 validation report's trailing blank line (`STD-010`).
- A diff scan found no newly added `#[allow(...)]` or `#[ignore]` attributes.
- Recorded green lanes in the task were treated as available evidence, not rerun or assumed to cover omitted lanes. The review did not run Cargo-backed commands because this independent static lane did not need to contend with other review agents.
