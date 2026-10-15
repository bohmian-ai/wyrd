# Maintainer review

Result: **FAIL**

Subject: cumulative `2f1db2194f5630df850b10987fe0f360946a525c` → `fde7d396a3917964dc344ede2ed09046e179aa66`, TASK-001 under approved specification revision 9. HEAD was still the candidate when this report was prepared. This is a static maintainer audit, not a completion verdict.

## Authority and coverage

Read the review skill, immutable subject, approved specification, original task, AGENTS.md (including §5, §6, §7, §8, §11 and §16), agent rules, spec-driven-development and maintainer-style references. Applied the Client model and public-surface authority in Wyrd design/doctrine, Bifrost's public client boundary, and Rust, PyO3, Python API/stub and TypeScript guidance. Used CodeGraph before code lookup; its broad symbol matches needed narrowing with source and caller searches.

| Changed surface | Owner / caller / proof inspected | Assessment |
| --- | --- | --- |
| Shared client initialization | `WyrdClient::{connect_ambient,connected}`, existing `access_token`, auth cache, new shared-client test | Existing authentication owner is reused. No new auth cache, health probe, credential resolver or platform trait. |
| Rust HTTP handles and Platform | `Cards::{connect,connect_with,new}`, Gateway, Principals, OperatorConnections, Platform constructors and retained fields; CLI administration and server setup callers | Dependency-owning handles remain discoverable and concrete. Platform keeps its separate session derivation. Private `Cards::new` has real loader/test callers and preserves their lazy paths. |
| Bifrost shared and blocking facades | `BifrostOptions`, `connect`, `connect_with`, `assemble`, `QueryClient::list_tables`, blocking projection, TableConfig describe pair, query/writer composition | Options replace the former table/config constructor variants. Compact discovery delegates through the existing query transport and wire result. No catalog hierarchy or schema fan-out introduced. |
| Local loaders | Shared WyrdState and Workflow, Python/Node wrappers and migrated SDK/CLI/test callers | D1 names project the existing owners. Workflow keeps its existing registry-reference work rather than acquiring an eager handle just to rename a constructor. |
| Python lifecycle | Full `LazyConnection<T>` and all five consumers: Cards, Gateway, Principals, OperatorConnections, Bifrost; pure-Python Bifrost and AsyncBifrost | One small foreign-runtime initializer has multiple real consumers and retains meaningful initialization state; genericity is earned. Bifrost local accessor behavior has the material gap below. |
| Python public contracts | Auth registration/package rename; authored Cards/auth/Gateway/operators stubs; Bifrost inline annotations and public stub re-exports; export and constructor unit tests; new integration journeys | Optional-client construction signatures align with their wrappers. Pure-Python methods stay declared once inline. Removed module import is exercised. |
| TypeScript | Native client/Card/Gateway/principal/operator constructors, native Bifrost listing, public facade factories and existing `nativeHandle`, `clientOf`, `lifecycleValue`; both native declaration files | Existing structured-result pattern earns the new native connection records; public factories return concrete Promises. Declaration files are identical and match the changed native sources on static comparison. |
| Tests and migrations | New Rust/Python/TS construction journeys and surface tests; migrated shared-client transport/Bifrost tests; Rust SDK support and existing journeys; CLI/server/MCP/testing consumers and executable Rust/TS examples | Reviewed cumulative caller migration and relevant new assertion bodies. No optional test harness or new dependency is justified or requested. Rust test rustdoc has the mandatory omissions below. |
| Architecture and task evidence | All changed architecture/reference hunks; task's implementation-evidence appendix and mise lane definitions | Architecture additions describe the approved construction rule. Evidence claims are not independent execution proof. TASK-002 prose and generated docsite outputs are outside this task. |

## Material findings

### MAINT-001 — Local Python Bifrost accessors unexpectedly connect

- Classification: **REGRESSION / VIOLATION**.
- Changed locations: `sdks/wyrd-sdk-python/src/bifrost/mod.rs:656` (`use_table`), `:680` (`table`), `:892` (`dropped`), `:898` (`producer_count`). Their shared producer is `Bifrost::handle` at `:554`, calling `LazyConnection::get` in `src/client.rs:120`.
- Governing obligations: Python API and stubs §Python Code Style explicitly says to avoid hidden IO in property accessors; REQ-007 says initialization occurs on the first operation that needs the server; INV-001 preserves capabilities. Maintainer Style §Documentation and §Python and TypeScript require the documented behavior to match the public operation.
- Reachable path: `AsyncBifrost()` → inherited synchronous `.producer_count` (or `.table` / `.dropped`) → native getter → `handle(py)` → ambient resolution, token exchange and ingest dial. With no credentials, simply reading a local count now raises `WYRD_CLIENT_401_NO_CREDENTIALS`; with a configured but unavailable endpoint it can synchronously wait on network work. The synchronous `use_table` also starts that connection instead of merely changing the retained write target. All these operations previously inspected or changed existing local state. The new unit `FIRST_USE` at `tests/unit/test_sdk_client_instantiation.py:24` deliberately uses `producer_count` as initialization proof, so it currently preserves the regression.
- Concrete maintenance/public consequence: a caller cannot inspect an unopened handle's local state or select its table without configuring a server, and an async caller cannot rely on the documented local synchronous operations remaining IO-free. The public `AsyncBifrost` docstring still explicitly says `use_table` performs no IO (`python/wyrd/bifrost/_client.py:1027`). The boundary now hides a network transition behind local property syntax.
- Smallest correction: keep these local operations on the existing Python boundary's retained state until its first server operation initializes the native handle; once connected, use that same native owner. Counts before connection are zero and the table is the retained initial/current binding. Do not add a second transport, a second client or another public construction form. Preserve preconnection table changes in the options passed to the existing shared connect. Update the first-use unit case to use an actual server operation.
- Focused closure proof: under the existing no-credentials fixture, construct both Bifrost facades, read `table`, `dropped`, `producer_count`, and change the table without an error or connection; then an actual server operation must report the existing no-credentials error. A real-server case should confirm a table selected before first server use becomes the connected write target. This can use the current fixtures and tests.

### MAINT-002 — Changed Rust fallible/panicking items omit required failure rustdoc

- Classification: **VIOLATION**; mandatory documentation gate, not a proposed style refactor.
- Changed locations: Python Bifrost `use_table` (`sdks/wyrd-sdk-python/src/bifrost/mod.rs:656`), `table` (`:680`), `dropped` (`:892`) and `producer_count` (`:898`) now return `WyrdPyResult` and may resolve credentials/dial transports, but have no `# Errors`. New `client_for` (`crates/shared/wyrd-client/src/client.rs:535`) and `server_handles_connect_eagerly_and_share_the_client_auth_path` (`:551`) contain `expect`/assertion failure paths without `# Panics`. Materially changed `Deployment::cards` (`sdks/wyrd-sdk-rust/tests/integration/support/mod.rs:111`) now awaits eager connection and panics on connection failure, while its sole doc line repeats the name and has no `# Panics`.
- Rule: AGENTS.md §16 requires substantive rustdoc on new/materially modified Rust items including test helpers/functions, `# Errors` on every fallible function, and `# Panics` where a panic remains possible; it explicitly makes missing rustdoc a hard blocker. Maintainer Style §Documentation repeats that requirement.
- Concrete maintenance consequence: newly introduced connection failures are not part of the documented contracts at the accessor and shared test-helper boundaries. A maintainer moving a previously local getter or fixture call cannot discover its network/failure requirement from those items' documentation.
- Smallest correction: complete the failure/panic contract documentation at those changed items, following nearby `handle` and `Deployment::client` examples. If MAINT-001 makes the local accessors infallible again, remove their unnecessary fallible shape instead of documenting an unintended network transition. Document the remaining real eager IO and panic conditions; do not change tests merely to avoid documenting them.
- Closure proof: static review of the corrected items' rustdoc against the final signatures and bodies; run only the owning documentation/lint checks selected by the orchestrator. No new runtime test is needed for prose.

## Nonblocking preferences

None. Repeated native connection-result structs follow the existing N-API projection pattern and have distinct concrete handle types. The Python lazy generic has five real consumers. Neither is an actionable abstraction finding.

## Limits

No builds, tests, formatting or generators were run, as required for this immutable static review. Verification labels in the task appendix were inspected as claims only. This report does not independently certify auth/security or concurrency behavior beyond the traced maintainer consequences above, and it makes no change-completion conclusion. Only this assigned report was written.
