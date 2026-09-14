# TASK-002 R2 Findings Validation

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-bifrost-surfaces`
- Review worktree: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior reviews: `task-002-f66a33769-review-01` and `task-002-r1-4d9d74b34-review-01`
- R2 remediation: `task-002-r1-4d9d74b34-review-01/TASK-002-R2-close-r1-review-findings.md`

The complete cumulative range was inspected, including every Wave 1 report.
The detached worktree remained clean and fixed at the candidate throughout
validation. No `.codegraph/` index exists in the worktree, so caller tracing
used direct source and repository search.

Two current user instructions resolve otherwise ambiguous review scope. First,
every commit in this candidate is authorized TASK-002 content; a finding cannot
rest only on a commit being unrelated or owned by a sibling domain. Second, the
package-local Python SDK Cargo release profile is expressly allowed for this
review. Those instructions outrank the remediation task and repository defaults.

## Wave 1 disposition

| Source ID | Validation | Final finding | Reason |
|---|---|---|---|
| `TASK-R2-001` | REJECTED | — | Its only defect is that authorized commits cover additional domains. Current user authority expressly permits that cumulative content. Concrete defects in those commits were considered separately. |
| `TASK-R2-002` | REJECTED | — | The user expressly allows the package-local release profile for this review. `STD-001` is the same proposal. |
| `TASK-R2-003` | CONFIRMED | `FIND-TASK-002-15` | The exact required base-to-candidate check fails at two committed Markdown EOFs; an empty worktree check is not the required proof. |
| `STD-001` | REJECTED | — | Duplicate of the expressly allowed package-profile proposal. |
| `STD-002` | REVISED | `FIND-TASK-002-7` | The R2 documentation pass added prose but still omitted mandatory `# Errors` and relevant cancellation/partial-progress contracts from live materially relocated fallible workflows. This reopens the existing documentation finding. |
| `STD-003` | REVISED | `FIND-TASK-002-8` | Qualified types remain in materially relocated fields/signatures and a function-scoped import remains. This is incomplete closure of the existing import finding, not a new defect class. |
| `STD-004` | CONFIRMED | `FIND-TASK-002-15` | Same exact cumulative diff failure as `TASK-R2-003`. |
| `STD-005` | CONFIRMED | `FIND-TASK-002-18` | Twenty-two candidate commits contain AI `Co-Authored-By` trailers, directly violating `AGENTS.md` section 13. The configured author is otherwise correct. |
| `SEC-R2-001` | REJECTED | — | The claimed immutable-migration violation conflicts with the more specific approved spec. REQ-042 says this integration targets a never-shipped greenfield database and makes the cleaned migration baseline authoritative; INV-015 prohibits compatibility migrations for nonexistent deployments. Changing the baseline in place is therefore authorized here. No separate reachable security defect was established. |
| `SEC-R2-002` | REJECTED | — | Static tracing found a parameterized tenant-RLS lookup that remains fail closed. The proposal identifies only topic/scope drift, which current user authority rejects as a finding. |
| `PERSIST-R2-01` | REJECTED | — | Its persistence objection is the same scope argument plus an upgrade premise contradicted by spec REQ-042. The new UUIDv7 sentinel and WAL audit-table restriction are internally consistent in the inspected greenfield paths; no separate regression was proved. |
| `PERSIST-R2-02` | REJECTED | — | The Forge seam is reachable only under `test-support`, but the proposal establishes only that its topic is outside R2. Current user authority permits the commit, and no production or test correctness failure was shown. |
| `STREAM-R2-001` | CONFIRMED | `FIND-TASK-002-16` | The direct Arrow path checks an atomic flag without joining the producer-map admission boundary or any in-flight drain owner. A direct send can remain unsettled after successful shutdown. |
| `STREAM-R2-002` | CONFIRMED | `FIND-TASK-002-17` | A failure found while draining a healthy stream writes `Broken` back after `settle` installed `Settled`; a second public call therefore cancels and polls again. |
| Public-contract empty ledger | VALIDATED, with limit below | — | The three assigned R2 contract findings are closed. The disclosed TypeScript empty-URL path is reachable, but it does not violate an approved error-classification obligation. |

## Disclosed TypeScript empty-URL behavior

`Bifrost.connect` and `TableConfig.describe` in TypeScript both call the shared
Rust `client_from_options`; the Python Bifrost constructors use that same owner.
An empty explicit URL therefore behaves consistently across the Bifrost
language projections: construction succeeds and the first request reports the
catalog-backed transport-down error. The contrasting Python proof uses
`Cards(server_url="")`, whose distinct Cards configuration owner explicitly
rejects an empty value before construction.

REQ-023 through REQ-025 require shared catalog identity, structured projection,
and the TypeScript error union; they do not require every client capability to
classify empty endpoints as `Config`, and `client_from_options`/`with_config`
documents only no-credential and transport-construction failures. The R2
finding required preserving metadata for `Config` and `TransportDown` variants
that are produced, which the candidate does. Making `WyrdClient::with_config`
or `client_from_options` invoke `HttpConfig::validate` would be a new shared
constructor contract, not closure of a proven TASK-002 requirement. The
disclosed behavior is therefore rejected as a finding, not waived because it
was called out as out of scope.

## Validated finding ledger

### FIND-TASK-002-7 — REVISED — VIOLATION: materially relocated fallible Rust workflows remain incompletely documented

- **Wave 1 source:** `STD-002`; prior stable finding `FIND-TASK-002-7`.
- **Violated obligation:** `architecture/agent-rules.md` makes rustdoc a hard acceptance rule for every new or materially modified Rust item, requires `# Errors` on every fallible function, and requires cancellation or partial-progress behavior when applicable. The R2 task explicitly required every added or materially relocated item in cumulative touched modules to satisfy that rule.
- **Exact location:** `crates/shared/wyrd-client/src/cards/config.rs:15`; `cards/saga/build_submission.rs:23`; `cards/saga/complete.rs:17,33,55,69,96`; `cards/saga/hash_artifacts.rs:16`; `cards/saga/upload.rs:25`; and `crates/wyrd/wyrd-auth/src/pg_resolvers.rs:118`.
- **Caller and reachability trace:** `Cards` construction calls `cards::config::load`; the registration saga calls `prepare`, then `validate_and_stamp`, `upload_artifacts`, and `complete_uploaded_cards`; the complete bodies await filesystem, upload, and registration HTTP work and propagate typed failures. `PgIssuerResolver::trusted_issuer` is called by both the external-token verifier and login issuer resolution through `IssuerConfigResolver`. These are live workflows, not dormant helpers. The cited `complete.rs` helpers are called in sequence by `complete_uploaded_cards` and each can return a typed validation error.
- **Evidence:** each cited item has descriptive rustdoc but lacks the mandatory `# Errors`; the async hashing, upload, completion, and issuer workflows also omit cancellation/partial-progress behavior where IO or uploads can already have occurred. The reported added-line scan cannot prove materially relocated signatures whose signature lines were not newly added by the documentation commit.
- **Observable consequence:** the candidate still violates a repository-defined hard completion rule and its evidence falsely claims zero undocumented relocated items.
- **Decision-complete minimum correction:** finish the documentation-only change on the existing functions. Add accurate `# Errors` sections to every cited fallible item and cancellation/partial-progress text only where the full body can have opened a file, transferred an artifact, completed a Card, or performed network/database IO before cancellation. Do not move code, add a check, add an allow, or extract helpers.
- **Focused closure proof:** enumerate items by cumulative relocation/current symbol rather than added signature lines; review every result; run `mise run fmt`, `mise run lints`, and the narrow `wyrd-client`, `wyrd-sdk-python`, and auth documentation/build checks covering the cited modules.

### FIND-TASK-002-8 — REVISED — VIOLATION: cumulative changed Rust still hides type dependencies in qualified signatures and a function body

- **Wave 1 source:** `STD-003`; prior stable finding `FIND-TASK-002-8`.
- **Violated obligation:** `architecture/agent-rules.md` requires types in fields, parameters, returns, bounds, and `where` clauses to be imported at module scope and written by bare name. It also requires every ordinary `use` declaration at module scope.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/facade.rs:121`; representative materially relocated TypeScript-native sites in `sdks/wyrd-sdk-ts/native/src/lib.rs:89,106,313,338,349,1074,1083,1091`; changed test signatures at `crates/shared/wyrd-client/tests/pg_bifrost_e2e.rs:258` and `crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs:1390`; and the function-scoped import at `crates/vala/vala-bifrost-redux/src/scribe/wal.rs:4373`.
- **Caller and reachability trace:** `Bifrost::with_sink` is the live constructor used by `query_only` and facade tests. `NativeLifecycleResult`, `NativeTableConfig`, Arrow encode/decode, problem projection, and `napi_error` are used by the exported N-API functions and TypeScript facade. The two external test helpers are called by their real-server tests. The WAL import is used by the compiled decoder regression test. Qualified spelling does not change reachability; these are precisely the signature/field/import locations governed by the rule.
- **Evidence:** examples include `Arc<dyn wyrd_queue::BatchSink<wyrd_queue::ClientByteGuard>>`, `serde_json::Value`, `serde::Serialize`, `napi::Result`, `napi::Error`, `std::fmt::Display`, qualified `wyrd_client::Bifrost`/`tokio::task::JoinHandle`, and `use crate::namespaces::BifrostNamespace` inside a test function.
- **Observable consequence:** R2 corrected only three previously cited spellings while leaving the same mandatory dependency-manifest rule violated throughout materially relocated and changed code.
- **Decision-complete minimum correction:** use the existing module-top import blocks, adding non-conflicting aliases only where the same short name would collide, and replace the governed qualified types with bare names. Move `BifrostNamespace` to the WAL test module import block. Inspect the complete cumulative changed Rust item set so closure is not limited to these examples; add no wrapper, helper, or module.
- **Focused closure proof:** source-audit every cumulative added/materially relocated Rust field, signature, bound, and `use`; then run `mise run fmt` and `mise run lints`.

### FIND-TASK-002-15 — CONFIRMED — VIOLATION: the required cumulative diff check fails

- **Wave 1 source:** `TASK-R2-003`, `STD-004`.
- **Violated obligation:** TASK-002 and TASK-002-R2 both require `git diff --check`; remediation review must prove the immutable base-to-candidate range rather than only uncommitted changes.
- **Exact location:** `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/findings-validation.md:183` and `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/verdict.md:88`.
- **Evidence:** `git diff --check 861f8d86cc3f9d7e70fb59489e80f8be62afddbf..f345cd8a4fdb5566ae6828ffb4f29d64f7f17599` reports `new blank line at EOF` at both locations. An unqualified check on a clean worktree examines no committed candidate changes.
- **Observable consequence:** the immutable candidate fails an explicit completion command and the appended evidence records a result that does not prove the reviewed range.
- **Decision-complete minimum correction:** delete only the extra terminal blank line in each existing review artifact. Add no formatter or permanent check.
- **Focused closure proof:** run the exact base-to-new-candidate `git diff --check` command and require empty output with exit status zero.

### FIND-TASK-002-16 — CONFIRMED — INCORRECT: successful shutdown can precede an admitted direct Arrow write

- **Wave 1 source:** `STREAM-R2-001`.
- **Violated obligation:** spec REQ-019, REQ-060, AC-004, and AC-021 require direct Arrow writes plus flush/shutdown durability to retain bounded ownership and drain behavior. `architecture/bifrost-design.md` requires shutdown to close admission before draining admitted work.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/handle.rs:199-211,266-287`; public facade at `bifrost/facade.rs:331-339,371-391`; direct settlement owner at `crates/shared/wyrd-queue/src/sealed_sender.rs:75-114`.
- **Caller and reachability trace:** public Rust `Bifrost::write_batch` delegates directly to `WriterPool::write_batch`; Python `PyBifrost::write_batch` and TypeScript `NativeBifrost::write_batch` call that public facade. Public shutdown in all three languages delegates to `Bifrost::shutdown`, which moves only `WriterPool::shutdown` onto a blocking worker. The complete direct-write body reads `closed`, then independently encodes, reserves the shared live-batch/byte budget, and awaits the sink. The complete shutdown body sets `closed` while holding the producer-map lock and drains only its buffered-producer snapshot. It neither serializes the direct read-to-reserve transition nor tracks a direct sender already past the atomic read.
- **Reachable interleaving:** a direct write observes `closed == false` and pauses before `SealedBatchSender::send`; an empty-pool shutdown stores `true`, snapshots no producers, and returns `Ok(())`; the direct write then reserves ownership and sends. No existing direct-write test synchronizes this interleaving.
- **Observable consequence:** shutdown reports a completed drain while an admitted batch can still own live-batch/byte capacity and await or later receive its durable acknowledgement.
- **Decision-complete minimum correction:** make the existing `WriterPool` the one lifecycle owner for direct-send admission and in-flight settlement. Shutdown must first refuse later direct sends and then wait for every already admitted direct send before returning success, while preserving the current one-batch/one-ID sender, retry semantics, shared budget, and buffered producer drain. Add no parallel lifecycle service.
- **Focused closure proof:** one deterministic stalled-sink test races direct admission with shutdown and proves the write is either refused before admission or shutdown waits for its acknowledgement; successful shutdown must leave zero live batches and dynamic bytes. Run that exact test plus the existing shutdown-admission and direct-write tests.

### FIND-TASK-002-17 — CONFIRMED — INCORRECT: a failed healthy drain makes completed settlement re-enterable

- **Wave 1 source:** `STREAM-R2-002`.
- **Violated obligation:** spec REQ-056 and the Bifrost settlement authority require dropped or broken streams to settle bounded ownership exactly once. `QueryResultStream::settle` publicly promises that a second call is a no-op and cancellation occurs at most once.
- **Exact location:** `crates/shared/wyrd-client/src/bifrost/query.rs:713-830,787-795,889-976`; current settlement proof at `query.rs:1394-1480`.
- **Caller and reachability trace:** `settle_with` is used by bounded collection exits; direct Rust callers can invoke public `settle`; Python/TypeScript stream owners ultimately delegate lifecycle settlement to the same stream. The complete `settle` body replaces the state with `Settled`, sends cancellation, and for a previously healthy body calls `drain_to_terminal`. That drain calls `next_batch`; malformed input, a frame after terminal, transport failure, or incomplete EOF calls `mark_broken`, which writes `Broken` back into `self.settlement`. `settle` then polls status and returns without restoring the terminal `Settled` state. A second call swaps out `Broken`, sends cancellation again, and repeats polling.
- **Observable consequence:** a completed public settlement can issue more than one cancellation and repeat bounded status work, contradicting its exactly-once contract. Server cancellation idempotency prevents demonstrated double release but does not satisfy the client promise.
- **Decision-complete minimum correction:** preserve resumability when the settlement future itself is cancelled, but make the existing `QueryResultStream` owner terminally settled after a completed drain-failure/status cycle. Do not add another state machine or cancellation owner.
- **Focused closure proof:** construct one healthy stream whose settlement drain encounters a malformed or trailing frame, call `settle()` twice, and assert exactly one cancellation and one completed settlement cycle. Run that exact test plus the existing `query_result_stream_settles_every_incomplete_exit_once` proof.

### FIND-TASK-002-18 — CONFIRMED — VIOLATION: candidate commits carry prohibited AI co-author trailers

- **Wave 1 source:** `STD-005`.
- **Violated obligation:** `AGENTS.md` section 13 says to use the configured Git identity and never add AI co-author trailers.
- **Exact location:** commit messages for `f345cd8a4`, `2dc538f81`, `c0a12085c`, `c73bb414e`, `305762044`, `5d21c9dd5`, `afdc3b798`, `3e8b6efb3`, `5cfe7b6b9`, `7fc756f09`, `c0d4dc924`, `35ba64fe3`, `f4ec1161d`, `4d9d74b34`, `2d2e818cd`, `96b531c08`, `da0c13a56`, `06783f749`, `f61cf03ad`, `6c603d141`, `b335fd7df`, and `dfdc60699`.
- **Evidence:** each named message contains an AI `Co-Authored-By` trailer. Commit authors are otherwise the required `Thorrester <sjforrester32@gmail.com>`.
- **Observable consequence:** the candidate history violates the repository's explicit contributor-attribution policy even though its source tree is unaffected.
- **Decision-complete minimum correction:** establish a new immutable candidate with equivalent reviewed tree content and the prohibited trailers absent, preserving the configured contributor identity. Because this changes commit identities, execution requires the caller's explicit authorization for the chosen history operation; review itself must not rewrite the branch.
- **Focused closure proof:** enumerate every commit in the new base-to-candidate range and require no AI `Co-Authored-By` trailer; re-establish the new candidate hash and rerun the source verification affected by any changed tree content.

## Prior-finding closure

| Stable finding | Result |
|---|---|
| `FIND-TASK-002-1` | CLOSED — safe `{field, reason}`, `{transport}`, and `{}` shapes now flow through the one shared projector and public language boundaries. |
| `FIND-TASK-002-3` | CLOSED — all seven operations and generated OpenAPI agree with the runtime `application/problem+json` mapper. |
| `FIND-TASK-002-7` | REOPENED — descriptive prose exists, but mandatory fallibility and cancellation/partial-progress documentation is incomplete. |
| `FIND-TASK-002-8` | REOPENED — the named Cards/upload paths are fixed, but the cumulative changed item set still contains the same qualified-signature/function-import violation. |
| `FIND-TASK-002-14` | CLOSED — the canonical reading page uses the existing public Python and TypeScript Bifrost facades. |
| `FIND-TASK-002-2`, `-4`, `-5`, `-6`, `-9`, `-10`, `-11`, `-12`, `-13` | Remain closed; the new direct-write and failed-drain paths are distinct reachable defects rather than failures of the prior corrections' focused boundaries. |

## Verification limits

- Source, full-body, caller, authority, history, and cumulative-diff inspection independently establishes the retained findings. No implementation or generated file was changed.
- The exact cumulative `git diff --check` was rerun and fails at the two locations in `FIND-TASK-002-15`.
- The recorded focused Rust, Python, TypeScript, codegen, docs, lint, and boundary passes were inspected but not rerun in Wave 2. None exercises the two newly identified lifecycle interleavings.
- The greenfield migration authority in spec REQ-042 resolves the migration-checksum disagreement; no deployment compatibility test is required for a nonexistent shipped database.
- Per TASK-002, the Bifrost aggregate was not run.

## Recommendation

**FIX_REQUIRED**

The deduplicated ledger contains reopened `FIND-TASK-002-7` and
`FIND-TASK-002-8`, plus new `FIND-TASK-002-15`, `FIND-TASK-002-16`,
`FIND-TASK-002-17`, and `FIND-TASK-002-18`. The source corrections reuse the
existing documentation/import blocks and the existing `WriterPool` and
`QueryResultStream` owners. The commit-metadata correction requires explicit
caller authority before any history-changing operation; this review performs
none.
