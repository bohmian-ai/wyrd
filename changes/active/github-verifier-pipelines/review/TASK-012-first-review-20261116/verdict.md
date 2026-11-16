# TASK-012 verdict

FIX_REQUIRED

Seven independently validated findings remain: FIND-012-1 through FIND-012-7. The integration preserves the principal pipeline and code-mode surfaces, but does not yet satisfy its explicit settlement and continuous resource-custody obligations or the mandatory changed-item rustdoc gate.

## Immutable subject and round

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004.
Branch: task/github-verifier-pipelines/TASK-012-integrate-code-mode.
Base: c24e1db8f6641cca9912376a4587e63e2fe74dd1, the parent of ee068f5a2.
Candidate: 9016393004dc0eb319075245c5f6a1d7a6c2cd46.
Reviewed range: git diff c24e1db8...901639300, the complete first-review range requested as ee068f5a2^..901639300.
Original task: changes/active/github-verifier-pipelines/tasks/012-integrate-code-mode.md.
Approved authority: changes/active/github-verifier-pipelines/spec.md revision 15, together with the task and amended inherited code-mode contract, current instructions, applicable contributor rules and owning architecture.

This is the first TASK-012 review. No prior TASK-012 acceptance, finding closure or remediation evidence is carried forward. Imported code-mode review history is historical context only. No convergence assessment applies. Both HEAD and the candidate branch remained at the pinned candidate throughout review.

Current maintainer directions govern: no benchmark here; TASK-013/014 own qualification on one combined server; PostgreSQL owns coordination time; server-clock gateway admission is accepted; architecture/wyrd-design.md is not edited. Every proof case struck in the task remains excluded. No new multi-pod, warm-direct-SQL, native-hook or inspection proof is prescribed.

## Independent selection and coverage

The complete navigation, producer/consumer map and initial specialist rationale are in scope.md. Independent validation confirmed that all selected reviews were required and completed:

| Boundary | Required report | Discovery result |
|---|---|---|
| Full task acceptance, complete port, dynamic pipelines and user journeys | task-review-behavior.md | FAIL |
| Complex shared invocation/resource lifecycle | task-review-invariants.md | FAIL |
| All changed language/tier authorities, generated projections and mandatory rules | standards-review.md | FAIL |
| Substantial ownership/abstractions, existing complete alternatives and correction burden | maintainer-review.md | FAIL |
| Deployed failure/recovery, process topology and added/moved external waits | system-review.md | FAIL |
| Security, accepted authority, tenancy and isolation | domain-review-security.md | PASS |
| Concurrency, admission and physical resource ownership | domain-review-concurrency.md | FAIL |
| Persistent state, ACK/fence custody and migration constraints | domain-review-records.md | FAIL |
| Fresh source/materiality/correction and coverage validation | findings-validation.md | FIX_REQUIRED recommendation |

These reports cover the affected unchanged dependencies as well as changed owners: borrowed walk cancellation, queued runner timeouts, provider media binding, SQL fencing and spill cleanup. No additional specialist is needed: SDK and diagnostic performance-tooling correctness is covered by behavior/standards/maintainer review, concurrency has its own domain review, benchmark qualification is excluded, and there is no UI change. Security's empty finding set is independently validated for its assigned boundary.

comparison.md groups overlapping claims by invariant. No focused follow-up was needed: there was no material authority conflict or unresolved discovery path. The fresh validator independently checked unique claims and source paths instead of treating agreement as proof. No validator-origin blocker required separate validation.

## Validated finding ledger

Full producer-to-consumer proof, proposal dispositions, materiality, correction burden and closure proof are preserved in findings-validation.md. These are the only retained findings:

| ID / status / classification | Obligation and exact source | Evidence and consequence | Smallest correction and closure |
|---|---|---|---|
| FIND-012-1 / REVISED / INCORRECT | OBL-3; engine.rs:840–867,1086,1870–1917,2021–2040; runner.rs:555–561,620 | Speculative cancellation and execution expiry drop the sole settlement owner while tracked blocking host work continues; checkpoint/terminal progress can outrun settlement. | Preserve existing settlement custody across engine and queued-runner cancellation; held host-work tests must block disposition/terminal until cleanup settles. |
| FIND-012-2 / CONFIRMED / MISSING | OBL-7, REQ-017; server verification/state.rs:272–322,364–369,421–477,635–659 | Live/recovered state has no charge; frozen state is cloned before charging; staged ACK returns its bytes while live values remain. Other consumers can promise retained memory again. | Account live growth incrementally at RunState/Writer and charge copies before allocation; verify held revision, later live growth, post-ACK custody, recovery and final release without engine persistence waits. |
| FIND-012-3 / CONFIRMED / INCORRECT | OBL-6/7; hosted/workspace.rs:308–315,1416–1423 | JSON staging and extraction build their full encoded Vec before asking the ledger; eventual Busy does not prevent the uncharged allocation. | Reuse existing serde counting mechanism and ledger before materialization; exercise both producers under insufficient encoding capacity and preserve successful bytes. |
| FIND-012-4 / CONFIRMED / INCORRECT | OBL-6/7; hosted/workspace.rs:1540–1567 | Replacement charges final-size delta while an entire temporary file exists beside the old file, bypassing shared scratch admission. | Charge temporary physical custody separately while retaining atomic replacement; prove refusal preserves old bytes, success ends charged once, and failure retains surviving-file custody. |
| FIND-012-5 / CONFIRMED / INCORRECT | OBL-6/7; hosted/workspace.rs:465–506 | Cleanup can log failed deletion and return the charge while files remain, advertising false free scratch to hosted/Bifrost consumers. | Reuse spill-file lifetime retention on unsuccessful/unknown deletion, covering partial opens and abandoned cleanup; verify failed and successful cleanup ownership. |
| FIND-012-6 / REVISED / INCORRECT | OBL-7, inherited REQ-027; hosted/evidence.rs:762–783; vala blocks/agent.rs:315–377,430–504 | media returns only MediaRef and drops its charge before provider binding retains copies in the prompt/model call. | Transfer runtime custody through the existing internal evidence/binding seam, without changing pure wire types; hold a provider call and verify custody until completion/cancellation. |
| FIND-012-7 / CONFIRMED / VIOLATION | Mandatory rustdoc gate; exact changed declarations listed in findings-validation.md | Added private fields/types, fallible helpers and changed/new tests omit required rustdoc/error/panic contracts. | Document the existing items in the same remediation; source inspection and scoped format/lint close this gate without new machinery. |

Source-local proposal IDs are reconciled in findings-validation.md. Related cancellation paths, state paths and scratch cleanup paths each have one correction boundary. No rejected proposal, optional suggestion or human-directed addition is represented as a validated finding.

## Reconciled acceptance

| Obligation | Accepted evidence and remaining exception | Result |
|---|---|---|
| OBL-1 port integrity | Source identities, adapted/excluded record, inherited history and protected-file range checks | PASS |
| OBL-2 contract reconciliation | Role/control owner mappings and current maintainer supersession of old wording | PASS |
| OBL-3 engine identities/scheduling/settlement | Identities and borrowed scheduling preserved; dropped-future settlement fails FIND-012-1 | FAIL |
| OBL-4 common direct/nested dispatcher | Source/assertions establish typed declarations, accepted authority, shared budgets, exclusions, recipients, citations and provider call identity | PASS |
| OBL-5 recovered spend | Monotonic persisted attempt counts, refusal/interruption allowance and focused test | PASS |
| OBL-6 workspace/sandbox custody | Fresh/reused sessions, suspended envelopes and immutable snapshots accepted; FIND-012-3/4/5 remain | FAIL |
| OBL-7 one continuous ledger | Shared root, retained-cache references, coalescing, group retry and ACK ordering accepted; FIND-012-2/3/4/5/6 remain | FAIL |
| OBL-8 direct behavior and public boundaries | Arrival deadline, target loading, queue-only refusal, cache/inspection/authz and capture/batch/fence source/assertions | PASS |
| OBL-9 six deployment targets/shared owner | Closed role mapping, pre-side-effect HTTP/MCP refusals and no idle reservation; struck proofs excluded | PASS |
| OBL-10 dependency/diagnostic constraints | Crates.io Monty, fork exclusions, diagnostic feature containment and passing dependency check | PASS |
| OBL-11 projection/history/control parity | Historical defaults, authored/generated declaration inspection and recorded generation/package evidence | PASS |
| OBL-12 pipeline journeys and negatives | Same-execution continuation, provider retry without recomputation, supersession/access fences and supported test repairs | PASS |
| OBL-13 inherited packet disposition | Historical verdicts retained, superseded/open states and qualification handoff preserved | PASS |
| AC-014 migrations | No task-range migration delta; contextual main comparison, permitted schema/RLS/cancel source and recorded fresh migration evidence | PASS with execution limit below |
| Mandatory repository rules | Broad rule coverage complete; changed-item rustdoc fails FIND-012-7 | FAIL |
| Explicit non-goals | Benchmark/multi-pod/architecture rewrite/extra inspection/new scheduler/foreign-library source work excluded | PASS |

The detailed assertion-level matrix and unaffected acceptance references are in the discovery reports and findings-validation.md. Scoped PASS rows do not waive the precise failing ownership obligations.

## Verification and limits

Fresh checks passed: check:deps, check:tenant-isolation with a temporary writable uv cache, py:lints, installed TypeScript Biome lint, range diff whitespace and shell/Node syntax checks, plus three focused nextest tests for recovered spend, charged outbox custody and coalesced state. Exact commands are in verification.md and the relevant reports. Initial default-cache failures were environment failures; writable-cache/equivalent installed checks succeeded.

A focused PostgreSQL test could not start because the repository wrapper was denied access to its configured Docker socket before provisioning. It is not recorded as a database test failure. Fresh migrations, complete SDK/identity/Bifrost suites, native rebuilds and generators were not repeated. Candidate-recorded lane evidence and inspected assertions remain available, distinguished from this review's executions. No benchmarks or synthetic host load were run. The findings are source-proven and do not depend on unavailable runtime reproduction.

No source, architecture, commits or Git configuration changed. The pre-existing modified task index and untracked TASK-011 remain untouched. This directory contains reports only.

## Remediation handoff

Write set and focused closure are packaged in TASK-012-R1-preserve-settlement-and-resource-custody.md in this directory. Route it directly to wyrd-implement. No new product, public wire, security, persistent-data or resource-policy decision is required. The next review checks these finding closures, the remediation delta and affected callers against this immutable candidate plus the original cumulative task; unchanged accepted behavior carries forward unless new material evidence invalidates it.
