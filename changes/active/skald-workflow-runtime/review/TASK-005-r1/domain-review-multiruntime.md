# Cross-runtime SDK, packaging, and selection domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `cc252339d5add2deff5c5cab826be92954076db0`
- Candidate: `f2f4b87dc783df790b831b6c818c6273106d661b`
- Approved authority: `changes/active/skald-workflow-runtime/spec.md`, Revision 14. Revision 14 supersedes TASK-005's Revision 12 references where they conflict.
- Task: `changes/active/skald-workflow-runtime/tasks/TASK-005-cli-and-integrated-proof.md`
- Review mode: source and recorded-evidence review only. No build or test command was run.

The candidate remained `f2f4b87dc783df790b831b6c818c6273106d661b` throughout this review.

## Reviewed boundary

This domain pass covered the public authored-file and registered-local Workflow journeys in Rust, Python, and TypeScript; their registration, identity, binding, output, and route evidence; Python fixture relocation; TypeScript native package dependency and release selection; and the separate compiled-CLI apply proof. It did not review server-run lifecycle semantics or the implementation of the shared Workflow engine except where needed to establish what the language journeys call.

The following approved decisions were treated as settled authority:

- Rust and TypeScript register through `Cards::register_from_path` / `Cards.registerFromPath`; the compiled `wyrd apply` and team-reuse proof is owned by `wyrd-cli`'s `workflow_file_apply_registered_local` journey.
- Python invokes the installed `wyrd apply` console entry point.
- The TypeScript SDK's Skald dependency/package closure and the updated CI-selection expectation are required.
- The dev-only wiremock allowlist entries are approved.
- The five TASK-004 r6 findings are deferred and are not TASK-005 findings.
- Revision 13/14 provider decisions and prior TASK-004 human decisions stand.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| Cross-language public Workflow API and one shared runtime | Spec REQ-054, REQ-057–058; INV-007, INV-016; `AGENTS.md` §§3, 8, 11; Python and TypeScript language guides | `sdks/wyrd-sdk-rust/src/lib.rs`; `crates/shared/wyrd-client/src/workflow/mod.rs`; Python `workflow.rs` and public stubs; TypeScript public integration imports and native manifest | PASS |
| Rust authored/registered-local journey | Spec REQ-054–058, AC-029–031; repository user-journey rules | `sdks/wyrd-sdk-rust/tests/workflow_loading.rs:1-737`; `sdks/wyrd-sdk-rust/Cargo.toml:15-33` | PASS |
| Python authored/registered-local journey and installed CLI apply | Spec REQ-054–058, AC-029–031; Python runtime ownership/test rules | `sdks/wyrd-sdk-python/tests/integration/cards/test_cards_crud.py:485-755`; `python/wyrd/cli/__init__.py`; `pyproject.toml:17-18`; `tests/integration/conftest.py:1-34` | PASS |
| TypeScript authored/registered-local journey | Spec REQ-054–058, AC-029–031; TypeScript N-API and journey rules | `sdks/wyrd-sdk-ts/wyrd/tests/integration/workflow-loading.test.ts:1-394`; `sdks/wyrd-sdk-ts/native/Cargo.toml`; public package metadata | PASS |
| Compiled-CLI apply/team-reuse complement | Approved Scenario 5 allocation; TASK-005 Scenario 2/5; AC-029 | `crates/wyrd/wyrd-cli/tests/workflow_journey.rs:755-780,883-1000`; `tests/cli.rs:21-22` | PASS |
| Journey selection and final proof | `AGENTS.md` §11; testing-workflows; TASK-005 verification contract | `mise.toml:286-289,1079-1101,1452-1471,1514-1537`; task-recorded focused and aggregate results | PASS |
| TypeScript package selection from Skald changes | Approved TypeScript packaging decision; repository-native dependency-closure selector | `crates/skald/*/Cargo.toml`; `crates/shared/wyrd-client/Cargo.toml:41-45`; `sdks/wyrd-sdk-ts/native/Cargo.toml:24`; `.github/scripts/select-ci.py:106-149,267-274`; `.github/scripts/tests/test-detect-changes.sh` changed expectation; `.github/workflows/release.yml:331-336` | PASS |

## Cross-runtime proof assessment

### Rust

The ignored Rust SDK journey uses the public `wyrd_sdk::Workflow` and `wyrd_sdk::cards::Cards` surfaces. Its pre-existing local, mixed-reference, authorization, inactive-card, selector, provenance, and no-floating assertions remain intact. The TASK-005 addition runs the checked-in example through the public Wyrd gateway and an execution-local external gateway binding, checks the named output, checks the external binding secret and absence of a Wyrd authorization header, proves registration leaves dispatch counts unchanged, and then executes the registered exact Workflow. The registered identity/UID and pinned Agent/Prompt relationships are asserted in the same journey (`workflow_loading.rs:203-271,681-735`).

The journey registers through `Cards::register_from_path`, as approved. The separate compiled-CLI journey invokes the compiled binary through the repository's existing `assert_cmd::Command::cargo_bin` path, runs `apply`, checks that apply makes no provider call, runs by exact identity and UID, retains the registered Workflow UID/version, and proves the team dependency does not float after a v2 registration (`workflow_journey.rs:755-780,883-1000`). No new cross-runtime emulation mechanism was added.

### Python

The journey imports `Workflow` from public `wyrd.agent` and `Cards` from public `wyrd.cards`. `_apply` resolves the installed `wyrd` console script next to `sys.executable`; `pyproject.toml` maps that script to the public wrapper over the native CLI. It uses that command for the team bundles, mixed Workflow, and checked-in example (`test_cards_crud.py:503-516,623-625,660-665,748-755`). This matches the approved Scenario 5 deviation.

The journey retains local-without-credentials, lazy external-ref resolution, missing/denied/inactive refusal, sibling/external provenance, wrong/mixed selector, locked exact relationship, no-floating, named output, and registered execution evidence. The TASK-005 route extension exercises public Wyrd gateway and external gateway execution, checks the named output, checks secret/header behavior, proves apply did not dispatch, and runs the registered example (`test_cards_crud.py:609-755`). The generic Card-envelope absence in the Python SDK is handled through the existing public HTTP Card route for exact stored refs and relationships rather than a new SDK-only reader (`test_cards_crud.py:548-571,668-681`).

Moving `gateway_server` from `tests/integration/gateway/conftest.py` to `tests/integration/conftest.py` makes the existing production-shaped fixture available to the Cards journey without duplicating it. Gateway tests remain beneath the new fixture scope. This is ordinary pytest fixture reuse, not a custom harness.

### TypeScript

The journey imports only the public `@wyrd/sdk` projection plus the established private `@wyrd/testing` harness. It preserves authored local/mixed execution, lazy credential and RBAC failures, inactive dependency, sibling/external provenance, malformed/mixed/wrong-kind selectors, exact stored Agent/Prompt relationships, UID loading, registered-run UID, outputs, and no-floating assertions (`workflow-loading.test.ts:172-324`).

The TASK-005 extension uses the same public `Workflow.fromPath`, `Cards.registerFromPath`, `cards.workflow.load`, and `Workflow.run` APIs for public-gateway and external-gateway execution. It asserts named outputs, binding-secret/header behavior, no dispatch during registration, and registered execution (`workflow-loading.test.ts:326-386`). The local Node recording server is a normal deterministic upstream fixture comparable to the repository's Rust wiremock and Python stdlib upstreams; it adds no product mechanism.

### Package and test selection

`wyrd-sdk-ts` normally depends on `wyrd-client`; `wyrd-client` normally depends on `skald-providers`, `skald-runtime`, `skald-tool`, and `skald-workflow`, with `skald-workflow` reaching the remaining Skald runtime graph. The repository selector computes transitive normal/build consumers as shipped output (`select-ci.py:129-149`) and selects `package_typescript` whenever `wyrd-sdk-ts` is in that shipped closure (`select-ci.py:267-270`). The changed test expectation therefore matches the actual package graph and the approved requirement; the selector itself was not weakened or special-cased.

Python and TypeScript journeys are permanently selected by their ordinary integration tasks (`py:test:integration` runs all integration-marked tests; `ts:test:integration` runs `tests/integration`). The Rust journey is intentionally ignored and outside the lib-only `test:wyrd-sdk` task; TASK-005 explicitly requires its exact repository-managed Postgres command as separate final proof. The compiled CLI Workflow journeys are likewise deliberately outside the aggregate and have exact recorded wrapper commands. No bespoke check or option was introduced to simulate selection.

## Recorded verification reviewed

The task records:

- the Rust SDK focused `workflow_loading_journey` command passing;
- the Python focused `test_workflow_loading_journey` command passing;
- the corrected TypeScript full-name selector passing (and accurately records that the earlier shorter anchored selector selected nothing);
- the relocated Python gateway fixture suite passing with 9 tests;
- `mise run gate` completing successfully at commit `b88102317`;
- all three ignored compiled-CLI Workflow journeys passing after the aggregate;
- clean `git diff --check` and no untracked implementation files at completion.

Source inspection confirms that the corrected TypeScript test name is the concatenation of `describe("Workflow loading")` and `it("workflow loading journey")`, that the normal TypeScript integration task selects the complete integration directory, and that the Python integration marker/file placement selects the Python journey.

## Verification limits

- Per the review instruction, this reviewer did not rerun builds, tests, package builds, dependency metadata, or CI selection. The assessment relies on the task's recorded command results and source-level confirmation of their selectors and covered paths.
- The focused Rust journey and the three ignored CLI journeys are completion evidence rather than permanently selected tests in the ordinary aggregate. This is an explicit approved/task-defined allocation, not an unrecorded coverage gap.
- No live provider credentials are exercised; all language journeys use deterministic local upstreams, as required by repository test policy.
- This domain review did not reassess the approved dev-only wiremock allowlist or the five explicitly deferred TASK-004 r6 findings.

## Material proposed findings

None.

No cross-runtime API mismatch, missing UID/binding/output/route proof, package-selection defect, disallowed runtime emulation, or unsupported bespoke mechanism was found in the reviewed candidate.

## Overall result

**PASS**
