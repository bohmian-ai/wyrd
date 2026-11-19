# Full TASK-003 / SDK lifecycle review at d7685d129

## Immutable subject

Primary repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
Clean detached candidate checkout: `/tmp/wyrd-task003-review-d7685d129`.
Requested full range: base `634e19cbc517293ed2458f30ce73401714f3447d` →
candidate `d7685d129ffee870ab1c497cdcb1206a89bc80c4` (six commits, 88 paths).
Use `git diff 634e19cbc...d7685d129` for this full requested review.
Latest prior substantive candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`;
its remediation delta `git diff 480251a70 d7685d129` is available too.
Original TASK-003 cumulative base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`;
`git diff 5775c3795...d7685d129` provides original tutorial context.
No rebased ancestry; candidates are descendants.

The user explicitly requests full review of new six-commit changes, rather
than closure-only review. Reopen the entire requested range and affected
callers. Prior source-backed unchanged native judging, retention/query and
tenant isolation coverage remains usable with dependency checks. Do not
restart unrelated positioning tasks or invent new hardening requirements.

## Authority and prior context

Approved specifications at candidate: `changes/active/positioning/spec.md`
revision 7 and `changes/active/service-identity-and-telemetry/spec.md`
revision 17. Read their owning requirements and current architecture. The
user specifically authorizes default managed configure/opt-out, TS async,
Trace markers, writerless flush and first-write automatic startup. Revision 7
changes AC-016 so Rust `otel::start_telemetry` plus its handle shutdown is
sufficient for the refund example: do not preserve FIND-3's superseded demand
for extra Rust configure/state.flush calls.

Original task working document (outside detached checkout):
`changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`,
content pin `6846032793910170e76c0cc0ec9de2f50cb6ad6f`.
Prior R1 task/evidence in `review/TASK-003-first-634e19cbc/`, hash
`6f3484e808b5015efa78abdf0c2b52863082fea5`.
Prior R2 task in `review/TASK-003-R1-expanded-480251a70/`, hash
`49fd0a6a3cdbcb23bd13c80ff7b8c6b834090e31`.
These are read-only content pins, not necessarily stored Git objects. Original
tasks' older revision-5 metadata remains acceptance history, not permission
to override approved revision 7/current user direction. TASK-004 metadata is
outside implementation scope of this task review; no task execution is implied.

Read prior substantive `verdict.md`, `findings-validation.md` and relevant
reports under `../TASK-003-first-634e19cbc/` and
`../TASK-003-R1-expanded-480251a70/` in the primary repository. Prior finding
IDs: FIND-TASK-003-1 (schedule setup, already closed); FIND-TASK-003-2 (rustdoc,
original five then two new sites); FIND-TASK-003-3 (Rust calls, authority now
changed); FIND-TASK-003-4 (TS missing optional package unit proof). Preserve
IDs for closure; new finding numbers begin at 5. The subject-only BLOCKED
attempt is not an unsuccessful implementation review. If this substantive
remediation fails, include the required convergence assessment for the two
unsuccessful substantive remediation reviews, distinguishing expanded scope,
authority change and actual incomplete closure.

Primary worktree has uncommitted lifecycle/state/error/generated/test edits,
in addition to older planning/mockup changes and prior untracked reports.
They are excluded. Read and run source only in the clean detached candidate,
or through `git show d7685d129:<path>`. Do not use dirty implementations/tests
as candidate proof. Reports go only in the primary review directory. No
staging/commit, source fixes or edits to authorities. Candidate checkout/commit
and pinned task inputs must remain unchanged; recheck before verdict.

## Common starting navigation map

| Contract | Owners / producers | Callers, consumers and proof |
|---|---|---|
| Lazy first writer and explicit start coordination | `wyrd-client/src/observe/lifecycle.rs` phase/start gate/StartClaim; `state.rs::{bifrost_writer,start_bifrost_with_config,start_gated,complete_start,running_bifrost}` | synchronous started observations, async sample fallback, `RunBifrost::{write,write_to}`, use_table_by_name; clones/concurrent first writes; start fail/cancel/shutdown; Rust phase tests and TS observation integration |
| Sample staging and first producer span | `observe/mod.rs::{record_or_stage,stage,finish,enqueue_staged}`, `Evidence/Staged`, drift owner | Python `src/observe/mod.rs`, TS `native/src/cards.rs`, runtime trace attach/context, sample rows/correlation; scheduler record programs and observe integration |
| Managed configure and writerless flush | Python/TS configure/internal installer and global providers; native phase flush/shutdown | ASGI lifecycle, opt-out, missing packages/frozen config, support desk deploy; committed Python/TS units and integrations |
| Model/mapping Trace and original work | shared `authoring/verifier.rs`, Python `verify.Trace`, TS definitions.Trace and public/native projection | schema fields/mapping lowerer, model values, actual refund programs and all language journeys; unchanged whole-trace judgment and retained inspection |
| Prior fixes / published setup | scheduled guide CLI tabs/Zod, new Rust field/test docs, new TS missing-package case, revision-7 Rust exception | prior finding evidence, actual guide source/outputs and generated text |
| Declared-table rows and durable sample boundaries | `RunBifrost`, sole-declared active table selection, Bifrost writer facade/producer pool and existing table resolver | author-in-code/support desk programs no explicit startup; concurrent writes must land every row; namespace/grants/row attribution and flush/shutdown |
| Evidence visibility and claim accuracy | actual result outbox, public execution/Bifrost reads and staged write durability | changed technical questions/how-verification-works/investigation prose: off-request writes, batching up to 50ms, loss conditions; exact versions/native errors remain |
| Public/generated standards | PyO3/stubs, TS native/declarations, stable errors, docs/AGENTS, FinanceFlow, setup/MCP prompts | six-commit 88-path inventory, generated source ownership and all changed-item rustdoc; current setup dependencies and reader-native commands |

The map is a starting point. Each reviewer independently expands it against
source, siblings and owners. No `.codegraph/` exists in the primary repository.

## Specialist applicability before discovery

| Role | Selection / contract reason |
|---|---|
| behavior-rev | Required for user-requested full six-commit audit: four tutorials plus REQ-017/AC-016, public calls, negative journeys, reader procedures, prior closure and new authority |
| invariant-rev | Required: materially new shared start gate/phase/lifetime behavior; concurrent first writes, explicit races, shutdown, failure/cancel/retry, no duplicate/stranded writer and producer attribution |
| repo-rev | Required: 88 paths span Rust/PyO3/Python/TS/examples/docs/contract and generated surfaces; all local rules, tests/rustdoc/proof gates and hot-path benchmark applicability |
| maintainer-rev | Required: new orchestration/staging and lazy-start workflow must be compared with existing lifecycle/facade/producer and schema/provider owners; no duplicate owner or speculative machinery |
| system-rev | Required: implicit dependency startup can now fail/delay first write; failure propagation, cancellation, partial startup resource ownership and recovery, application startup/provider setup and durability/loss claims |
| domain-rev (evidence and write integrity) | Required sensitive boundary: first sample spans/rows and trace markers determine evidence/subject/run attribution; retained-result visibility/loss claims and ordinary table/tenant authorization must remain truthful. Concurrency specialization is independently covered by invariant plus system owners; do not duplicate it as another domain role |
| Additional auth/security, SQL/schema/persistent-engine specialists | Omitted provisionally: no auth implementation, migration, SQL capability or server storage protocol changes in the committed range. Domain covers existing grants/tenant and durability consumers; any material changed sensitive boundary discovered requires reassessment |

Each selected discovery agent gets this same map, original/spec context and
assigned report path, but no other current-review conclusions or intended
verdict. After discovery compare causes, use focused follow-up only for a
material uncertainty, then always a separate fresh ponytail validator. No
agent fills two roles. Review directory holds reports only, never source
copies/patches/diffs. No extra ad hoc navigation scripts.

## Verification and hot paths

Supplied current results: client lib 290, examples 9/9/build, Rust fmt/lints;
Python 668 units/lints and scheduled/support/authoring/declare/blocks/observe
integrations; TS 96 units/60 affected integration tests plus lint/format/types;
docs tests/commands/links/build/a11y/generated sync. They are claims to assess
against actual candidate source/tests, not a verdict. Broad gate/full journey
suites are intentionally not required at task review. Fresh checks are owned
by the orchestrator and recorded separately; no agent runs Cargo concurrently.

Every reviewer must distinguish startup misses from warm sample enqueue.
Assess actual execution paths against the user's defined hot paths, not the
implementation label. If a designated hot path is materially touched, PASS
requires a complete external-wait inventory matched to sanctioned waits and
full clean engine benchmark VALID compared to newest same-host/experiment
history. No benchmark waiver is granted. No synthetic host load. Authoring or
application lifecycle alone does not automatically imply an engine benchmark;
prove applicability from callers/source. Report material authority conflicts
rather than strengthening unapproved machinery.
