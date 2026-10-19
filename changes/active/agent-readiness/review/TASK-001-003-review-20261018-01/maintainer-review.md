# Independent maintainer review

Result: **FAIL**. Proposed source IDs: **MAINT-001**, **MAINT-002**.

Subject: cumulative `1b962bb00...98d8ef46e61ea6aa5dae90d59572467a256a3ef3`,
TASK-001–003, approved agent-readiness specification revision 2. HEAD was still
the candidate at the final source check. Reviewed source was not edited.
Other reviewers' reports were not consulted.

## Authority and coverage

Applied AGENTS.md (especially §§2–9, 11, 16), architecture/agent-rules.md,
the reference router, maintainer-style, spec-driven-development, Rust,
PyO3, Python-stub and TypeScript guidance; design/doctrine requirements for
declaration, registry identity, client ownership and runtime loading; and the
approved specification/task packet. The checkout has no `.codegraph/`.

| Changed surface | Owners, callers and proof examined | Maintainer assessment |
|---|---|---|
| Local declaration/check and CLI verbs | `card.rs::{declare_check,check_tree,dispatch_check,declare_tree,read_envelope,pull_bundle}`, parser/dispatcher/commands exports; `declaration.rs`; loader's `DeclarationInput`, load and input builder; `card_lifecycle` local/real-server cases | Local check and declaration reuse the existing loader and saga. Check remains synchronous and credential-free; networking stays on `Cards`. Removing the one-Card public load also removes its result/projection rather than retaining an alias. |
| Shared declaration orchestration | `Cards::{declare_from_path,declare,declare_with_progress,get,pull}`, engine/context, all saga modules, typed progress and terminal renderer | Existing declaration saga is renamed in place; upload/completion and typed callbacks retain their owners. CLI adapters do not add a transport or a second durable state machine. |
| Graph publication and runtime load | Former hydrate versus pull modules compared directly across commits: graph traversal, bundle writer, workspace prepare/publish/abort; manifest DTOs; `BundleReader` and `WyrdState::from_path`; state fixtures/tests and SDK declare-and-pull callers | This is the existing complete graph workflow, not a leaf-helper reuse inside replacement orchestration. Writer/reader discriminator and `mode` agree. Existing runtime hydration remains distinct. Confined bundle publication/loading and metadata-only refusal are retained. |
| Server, SQL and audit vocabulary | Card routes/service/mapping/resolve, `DeclarationWriter`, request/receipt/replay/outcome DTOs, SQL declare/reconciliation/version owners and migration edits, audit enums/fixtures; route and SQL integration consumers | Server transaction and replay ownership remain server-side. Persisted operation names and typed result variants move together. Independent lifecycle and audit-created outcomes remain distinct. No new graph or persistence abstraction was added. |
| CLI machine streams and foreground lifetime | `main`, `json_requested`, `run_cli_code`, `WyrdCliError::{usage,details}`, `CliBoundaryError::print`, `Diagnostics`, `DevReady`, DevServer run/start/stop, installer result and MCP reports; CLI JSON/error/installation/local-dev tests | Foreground lifecycle remains on existing DevServer, while Diagnostics earns its retained file handle. Report rendering is separated from host installation. MAINT-001 and MAINT-002 remain at the process entry point. |
| MCP guidance and merged read | `GetCardArgs::read`, descriptors, handler dispatch, HTTP audited read owners, hosted built-in tool call, guide response and initialization instructions; MCP discovery/verification/proxy tests | Selector adapter delegates both alternatives to existing read workflows and refuses partial/ambiguous combinations. Guide extends the existing guide owner; no declaration/check/ingest engine is introduced. |
| Python public/native/stub closure | Generic and per-kind registry methods, `RegistryGetQuery`, `PythonCardRegistry`, `PyCardEnvelope::from_registry_card`, state hydrator, local Model/Data loaders; package exports/authored stubs; CLI testing native/functions/reexports; CRUD, local-state and declaration journeys | Native methods and authored declarations agree on declaration/pull names and removed eager-load parameters. Existing holder/interface conversion remains at the Python boundary. Required local path and removal of managed eager workspaces are coordinated across callers. |
| TypeScript/Rust SDK and generated closure | Native Cards, CLI native testing, public Cards/testing wrappers, N-API declarations, wire generator/source declarations, Rust reexports and integration callers | Public receipts and pull summaries retain their actual shapes. Native declarations match the renamed methods and changed CLI get arguments. No SDK transport reimplementation was introduced. Generated bulk was assessed against owning sources/projections rather than treated as an independent implementation. |
| Docs and examples | llms generator, page-text entry point/projection, existing `embedCodeFromFile`, index/page-text tests, setup/quickstart/start guidance, support-desk-app changes and executable Python quickstart journey, Rust library and TS workspace/check integration | Embed resolution reuses the site's existing full resolution workflow through a renderer callback. The added prose/component projection has a separate readable-text purpose. Rust/TS reference copies are explicitly described as compilation/typecheck proof, and the Python quickstart is the executed application. No general snippet framework was added. |
| Static authority, fixtures, ancillary consumers | Platform/maintainer/Rust authority edits, fixture directory rename, remaining examples and SDK journey references, UI vocabulary edits and mise selectors | These close vocabulary/build references rather than adding owners or alternate public paths. No preference finding is raised for equivalent naming/layout choices. |

## Material findings

### MAINT-001 — inherited tracing breaks the promised machine error stream

- Classification: **INCORRECT**; material public-contract consequence.
- Changed location: `crates/wyrd/wyrd-cli/src/main.rs:9–22`.
- Governing obligations: specification REQ-006 and AC-007; maintainer-style's
  requirement that public documentation and behavior describe the same
  operation. `json_requested`/`run_cli_code` explicitly document one error
  document on stderr, and `CliBoundaryError::print(true)` implements that
  locally.
- Evidence: `main` chooses `off` only as the fallback filter; a valid inherited
  `RUST_LOG` wins through `try_from_default_env()`, while the writer remains
  stderr. Therefore normal TLS and HTTP client diagnostics interleave with
  the error document. This affects all networked commands sharing the entry
  point, including get/pull/declaration/setup, rather than one error renderer.
  The new test subprocess setup removes `RUST_LOG` in
  `card_lifecycle.rs:39` and `pg_tests.rs:115`, concealing this condition.
- Concrete cost: an agent or script following the documented contract cannot
  deserialize the whole stderr value when the surrounding development shell
  has logging enabled. It must know an undocumented environment restriction
  or add a fragile last-line parser, defeating the new contract.
- Focused observed reproduction using the existing local binary, with HEAD
  at the candidate (no source modification or rebuild):

  ```sh
  mise exec -- env WYRD_CONFIG_HOME=/tmp/wyrd-review-missing-config \
    WYRD_ACCESS_TOKEN=review-bearer RUST_LOG=debug target/debug/wyrd get \
    --kind Prompt --space default --name absent \
    --server http://127.0.0.1:1 --format json
  ```

  It exited 1 and emitted ten DEBUG lines from rustls/reqwest/hyper before
  the `wyrd_cli_error` JSON document. Source independently establishes the
  filter/sink behavior; the prebuilt binary is supporting reproduction,
  not independently rebuilt candidate verification.
- Smallest correction: enforce the JSON stream policy at the existing entry
  point before applying the ambient tracing filter. An inherited `RUST_LOG`
  must not install a console logging sink for a JSON invocation. Preserve the
  existing foreground diagnostic-log destination and human-mode filtering;
  do not duplicate parsing, transports or startup ownership.
- Focused closure proof: extend the existing machine-stream subprocess proof
  with `RUST_LOG=debug` and a real attempted network operation (not just an
  early usage error). Assert empty stdout and whole-stderr JSON on refusal,
  and empty stderr for the successful JSON operation. Keep the existing
  secret assertions and foreground diagnostic-log assertions.

### MAINT-002 — changed process entry point lacks required item rustdoc

- Classification: **VIOLATION**, the explicit Rust documentation gate.
- Changed location: `crates/wyrd/wyrd-cli/src/main.rs:3–4`, `main`.
- Governing rule: AGENTS.md §16 requires substantive rustdoc on every
  materially changed Rust function regardless of visibility; the review
  skill explicitly retains missing rustdoc as blocking.
- Evidence: this function now establishes the JSON logging policy in
  addition to TLS/subscriber initialization and shared CLI dispatch, but has
  no item rustdoc. The file's `//!` documents the module, and the inline
  logging comment documents one branch; neither documents the function.
- Concrete cost: the owner of process-level machine-stream behavior has no
  item contract describing initialization, output policy and exit behavior.
  This is exactly the boundary whose ambient logging exception contradicts
  the documented library behavior in MAINT-001.
- Smallest correction: add substantive rustdoc to this changed entry point
  in the same bounded correction as MAINT-001. Explain TLS/subscriber setup,
  JSON versus human stream policy and shared dispatcher/exit behavior. Do
  not use this as a reason to document unrelated untouched entry points.
- Closure proof: source inspection of that item plus the existing applicable
  format/lint check; no behavior test is needed for documentation alone.

## Verification limits

No broad test suite, compilation, migration, code generation or live external
service was run by this reviewer. Task evidence reports focused CLI, SQL,
SDK, MCP, docs and type/codegen checks; those results were treated as reported
evidence, not substitutes for source review. The sole local reproduction
above used a deliberately unreachable loopback endpoint and a synthetic
bearer. Source and other agents' edits were preserved.

No optional style, module-size or abstraction-preference findings are proposed.
The existing owner reuse is satisfactory on the reviewed parser/loader,
transport, graph-publication and foreground-lifecycle paths; the outstanding
machine-stream defect and explicit rustdoc gate prevent PASS.
