# TASK-011 repository standards review

Subject: `/home/thorrester/Documents/GitHub/wyrd-vcc-t005`, base `338f33235f81c30dfe3a570dc26934fe7bb77048`, candidate `6e3bac0370a31b19d767ac4d20830d430c3f2ff5`. I inspected the complete 34-file diff. `.codegraph/` is absent. This is a standards audit, not a task-acceptance verdict.

## Authority coverage

| Changed surface | Applicable authorities inspected |
|---|---|
| `vala-drift` PSI, SPC, baseline, feature, errors and reports | `AGENTS.md` §§3–7, 10–11, 15–16; `architecture/agent-rules.md`; `architecture/wyrd-design.md` Verifier/Drift; `architecture/wyrd-doctrine.mdx`; `architecture/references/{architecture/patterns,languages/rust-core,languages/testing-workflows,domain/vala-architecture,domain/drift-monitoring}.md` |
| `wyrd-spec` profile, validation, schemas and schema goldens | `AGENTS.md` §§3–5, 8–9, 11, 15–16; agent rules on generated artifacts and Rust docs; design/doctrine; reference patterns, Rust core, errors and testing |
| `wyrd-server` Drift query and result path | `AGENTS.md` §§3–6, 9–11, 15–16; agent rules on tenancy, audit, test tiers, Rust docs; design/doctrine; reference patterns, Rust core, Vala, drift, testing and analytical serving; `architecture/bifrost-design.md` for the retained query path |
| `wyrd-testing` and Rust SDK journey | `AGENTS.md` §§3–6, 11, 16; agent rules on fixtures and Postgres tests; reference patterns, Rust core and testing |
| Python and TypeScript SDK journeys | `AGENTS.md` §§3, 8–11, 16; agent rules on user journeys; references for Python API/stubs, TypeScript and testing |
| Design, drift architecture, generated public docs and task evidence | `AGENTS.md` §§9, 11, 14–16; `architecture/references/README.md` router; spec-driven development, implementation execution and testing references; approved spec revision 37 and TASK-011 |

## Rule results

| Rule | Result | Evidence |
|---|---|---|
| Owner and tier placement; `wyrd-spec` remains pure; SDKs do not score | PASS | Typed `SpcProfile` stays in `wyrd-spec`, algorithms in `vala-drift`, fixed query in `wyrd-server`; SDK diff changes tests only. No manifest/dependency/feature changes. |
| Struct-centered Rust, synchronous pure computation and existing query owner | PASS | `SpcScorer` and `DriftEngine` retain workflow ownership; new `ObservationWindow` methods render pure SQL synchronously; `Reader::complete` awaits the existing query stream. |
| Tenancy, authorization, audit and durable resource boundaries | PASS | Server diff retains `ObservationWindow` subject/window filtering and `Reader::fold` query-service path; no new SQL pool, auth path, migration, lease or audit publisher. Test fixture uses `tenant_conn`. |
| Rust documentation and fallible-item error sections | PASS | New/changed engine, contract, server and fixture items carry rustdoc; fallible new functions document error conditions. |
| Generated schemas and public documentation | PASS | Schema and golden changes are sourced from `SpcProfile` and `codegen:check` is recorded as passing; design and `llms-full.txt` are updated; `docs:check` passed. |
| Real Rust, Python and TypeScript journey tier; environment isolation | PASS | All three SDK journeys changed and their repository-managed lanes passed; Postgres fixture change remains in the existing test crate and server SQL tests remain in a test module. |
| Format, lint, targeted family/owner tests and diff check | PASS | TASK-011 records `fmt`, `lints`, `py:format`, `py:lints`, `ts:typecheck`, `test:vala`, `wyrd-spec`, three Drift journeys, server integration, `codegen:check`, `docs:check`, and diff check as exit 0. I did not rerun them. |
| Served OpenAPI contract gate for a public `ToSchema` change | FAIL | `SpcProfile` changes its served schema; TASK-011 omits `mise run test:principals:integration`. `test:bifrost:integration:server:inner` excludes `pg_openapi_contract` (see `mise.toml`); `test:principals:integration:inner` includes it. |
| Exact focused command for every specifically named test | FAIL | TASK-011 names 23 Vala tests but records only one template using `-E 'test(=<path>)'`, not 23 exact runnable commands. This also leaves no independently auditable per-test result mapping. |
| Task lifecycle metadata | FAIL | TASK-011 front matter still says `status: ready` although its implementation evidence claims an implemented candidate submitted for review. The spec-driven reference defines `review` as the submitted state. |
| No gate circumvention or unrelated source edits | PASS | No added lint suppression, ignored test, test deletion, compatibility path, or unrelated production source was found in the diff. |

## Material findings

### STANDARDS-011-1 — Served OpenAPI verification is missing

**Rule:** `AGENTS.md` §11 and `architecture/references/languages/testing-workflows.md` require `mise run test:principals:integration` to prove an OpenAPI change against the document actually served at `/openapi.json`. **Location:** `crates/wyrd-spec/src/card/drift.rs` `SpcProfile` and `changes/active/verified-change-contract/tasks/TASK-011-conventional-psi-spc.md` verification record. `SpcProfile` derives `utoipa::ToSchema` under the server feature and loses two public fields. **Consequence:** passing JSON schema generation does not verify that the composed server serves the changed profile contract; the recorded server lane does not contain `pg_openapi_contract`. **Correction:** run `mise run test:principals:integration` on this immutable implementation, record its result in TASK-011, and address any failure without weakening the gate.

### STANDARDS-011-2 — Named Vala tests lack exact focused commands

**Rule:** `AGENTS.md` §11 and `architecture/agent-rules.md` require each specifically named Rust test in a task artifact to include and run its exact focused `mise exec -- cargo nextest run` command with package, target and exact expression. **Location:** TASK-011 implementation evidence, “Focused, one exact command per named test” bullet. It records `-E 'test(=<path>)'` for 23 named `vala-drift` tests. **Consequence:** the task does not provide the exact reproducible command or individual outcome for any of those named tests, despite the claim that each ran alone. **Correction:** record the 23 actual fully qualified expressions and corresponding exact commands/results; run any named test whose exact invocation cannot be established from the current evidence.

### STANDARDS-011-3 — Task state remains ready after implementation

**Rule:** `architecture/references/languages/spec-driven-development.md` defines task states and moves submitted implementation through `review`. **Location:** TASK-011 front matter line 4, `status: ready`, versus its implementation evidence and the submitted candidate. **Consequence:** the tracked task packet gives a stale lifecycle state to later reviewers and planners. **Correction:** set the task status to `review` in the evidence update; do not change the approved specification or source code for this correction.

## Overall result

**FAIL.** The three bounded standards gaps above remain. The recorded test exits are credible for the lanes named, but they do not cover the required served OpenAPI gate or supply exact focused commands for the named Vala tests.
