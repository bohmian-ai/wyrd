# Independent findings validation — TASK-003

Recommendation: **FIX_REQUIRED**. Fourteen bounded findings remain after deduplication. This is an independent validation of the first TASK-003 review, not an implementation or an approval of later commits.

## Immutable subject and method

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/verifier-building-blocks-task-003`.
Base: `41b0f01d9b47d9b07b078b2431fcdd4cba56a9b0`.
Candidate: `57c69c3579e4c331e8d3258e00b22ee7aa0155cb`.
Authority: approved revision-6 `spec-snapshot.md`, original `task-snapshot.md`, AGENTS.md, architecture/agent-rules.md, applicable Wyrd doctrine/design and repository reference rules. The subject's explicit instruction excludes later edits/commits; source below was obtained with `git show 57c69c357:<path>`. Current CodeGraph was navigation only, never evidence that overrides the pinned tree.

Read all discovery reports, claims-comparison, followup-review, and the three raw passing logs. Inspected the changed runtime owners, their complete correction-boundary bodies, shared callers, protected owners, existing storage/queue/test owners, and locked Monty 1.1.0 source. Compared the cumulative diff, including relocation, manifests, MCP visibility, toolchain and worker changes. No production or test source changed; no subagents or new harness were used. This report is the only write.

Path shorthand used below:

- `H/` = `crates/wyrd/wyrd-server/src/verification/hosted/`.
- `V/` = `crates/vala/vala-eval/`.
- `W/` = `crates/wyrd/wyrd-monty-worker/`.
- `S/` = `crates/wyrd/wyrd-server/`.
- Dependency paths are under the installed Cargo registry `index.crates.io-1949cf8c6b5b557f/`, version 1.1.0, matching the candidate lockfiles.

## Proposal-by-proposal disposition

Every proposed finding is represented here, including unique claims. REVISED retains only the supported claim/correction described in the ledger; rejected parts must not return as optional remediation.

| Discovery proposal(s) | Disposition | Final finding / correction validation |
|---|---|---|
| BEH-01, INV-01 | CONFIRMED | FIND-TASK-003-1. Both Artifact branches unconditionally refuse. Reuse authorized registry/inventory/storage owners; do not implement another registry or fetch path. |
| BEH-02, INV-03, RES-01, SYS-003-1 | REVISED | FIND-TASK-003-2. Hard aggregate enforcement is absent. Replace sampled enforcement with the native worker-only group boundary, and delete the superseded sampler rather than strengthening it. Unsupported isolation refuses Python capability, not unrelated server roles. |
| RES-04, SYS-003-2 | REVISED | Merged into FIND-TASK-003-2. Cancellation skips normal-return removal from `busy`; raw PID reuse can target another process. Deleting the sampler/PID authority closes this root without adding a second registration guard. |
| BEH-03, INV-02, RES-02, SEC-003-02, SYS-003-3 | CONFIRMED | FIND-TASK-003-3. Per-feed Monty accounting resets while the directory persists. Retain native mount/accounting state at workspace lifetime. |
| BEH-04, RES-03, SYS-003-4 | REVISED | FIND-TASK-003-4. Concurrency, cancellation and retry reset are supported; use the existing per-execution host and action identity, not new queue persistence. Do not change global Skald concurrency or reserve workers during model waits. |
| BEH-05, INV-04 | REVISED | FIND-TASK-003-5. Unbounded gridSpan expansion is proven. Bound that source expansion; reject an open-ended demand to redesign/audit every parser or move parsing to a new service. |
| BEH-06, INV-06, RES-05, MNT-003-01, SYS-003-5 | CONFIRMED | FIND-TASK-003-6. Arbitrary UTF-8 byte slicing panics in the parent callback. Existing safe truncation is sufficient. |
| BEH-07, INV-05 | REVISED | FIND-TASK-003-7. Suffix loss and inaccurate serialized sizing are real. Complete bounded continuation using the existing private unit cursor; do not create a new public citation/read format. Mere apparent EOF is unacceptable. |
| INV-07 | REVISED | FIND-TASK-003-8. Original slot mapping is invisible to tools. Expose the retained mapping for original files; do not claim a derived JSON pointer is an original-source location. |
| INV-08 | CONFIRMED | FIND-TASK-003-9. Text filtering incorrectly decides JSON-pointer existence. Null/empty values are valid source locations. |
| RES-06 | REVISED | FIND-TASK-003-10. `py_repr`, list/walk, search and final serialization allocate before the configured result refusal. Bound producers and final encoding; no new output service or blanket change to protected domain owners. |
| SEC-003-01 | REVISED | FIND-TASK-003-11. A new unmapped user namespace retains the inherited host user credential. Reject a requirement that a harmless primary GID must numerically differ. Preserve independent existing controls. |
| BEH-08 | REVISED | FIND-TASK-003-12. Required concrete cross-owner and recovery proof is unauthored. A two-call scripted citation test already exists; the absence is concrete hosted evidence/protected-owner coverage, not absence of any two-call test. Task-005 production wiring and complete journeys remain excluded. |
| REPO-003-01 | CONFIRMED | FIND-TASK-003-13. Four Postgres-backed tests are in the ordinary `tests` namespace. Use the existing sanctioned `pg_tests` home. |
| REPO-003-02, MNT-003-02 | CONFIRMED | FIND-TASK-003-14. Required rustdoc is missing on changed production/test items. Batch prose corrections; add no checker or unrelated rewrite. |
| Follow-up author-now/run-later conclusion | CONFIRMED with scope correction | Task scenario 6 expressly requires authoring real protected-owner and queued retry/restart regressions now. Run focused currently executable seams now; final new-contract ACK/queue composition and whole journeys remain TASK-005. See finding 12. |
| Follow-up inherited-UID conclusion | CONFIRMED with proof revision | Source and Linux semantics support finding 11. A separate namespace inode/overflow display proves no host credential transition. No distinct harmless GID or per-worker unique account is required. |
| Steady-state warm replenishment note | REJECTED as a finding | Prewarming exists at `Sandbox::start`; there is no independently established requirement here for permanent replacement prewarming. No replenishment implementation is required. |
| Orphan-process/parent-death speculation in system limits | REJECTED as an independent finding | No pinned final supervisor/container topology proves the proposed deployment failure. Do not add parent-death machinery from this note. Required component cancellation/recovery proof remains finding 12. |
| Function-local imports, qualified names and structural preferences | REJECTED as blockers | No independent behavioral consequence or deletable unearned workflow is established. Do not create a style-only task. Required rustdoc is the explicit exception. |
| Query cancellation concern | REJECTED | `BoundedQuery` consumes the existing stream owner with admission/cancellation cleanup guards; the fresh-token comment alone does not establish a leaked query. Do not replace that owner. |

## Source and ownership reconciliation

The new owners earn their existence. `AgentExecutor::invoke` uses `Agent::run_prompt`, the existing registry and Skald loop; its old judge sibling consumes relocated helpers through an adapter. The task explicitly forbids building on the old JudgeInvoker, so requiring that reuse would be wrong. `HostedTools::Invocation::call` first runs `audit::authorize` and then calls `get_card_for`, `get_card_by_ref_for`, `list_cards_for`, Bifrost service or `BoundedQuery`. Their existing domain authorization must remain. MCP changes share catalog/argument definitions; they add no loopback transport or second public tool catalog.

`TenantMedia` supplies bounded authorized URI media, but does not supply pinned slots, extraction, scratch or citation semantics. `TenantEvidence` appropriately reuses `StorageHandle::object_len/get_object_bounded` and tenant paths. `Extraction` is the one parser used by extract actions, workspace materialization and citation validation. `Workspaces`/`Workspace` own real permits/directories; `Sandbox` owns the real pool. None should be replaced with a platform trait, extra service, client implementation, generic policy framework or second parser.

The correction ladder therefore selects deletion for the duplicate PID watchdog, existing registry/storage/protected-domain owners for Artifact and tools, native kernel enforcement for worker group/credentials, installed Monty mount and object formatting facilities for IO/results, and small local fixes for UTF-8, pointer existence and rustdoc. No new general architecture is justified.

## Validated retained ledger

### FIND-TASK-003-1 — Exact Artifact evidence and Python code cannot execute

**Status/classification:** CONFIRMED / MISSING. Sources: BEH-01, INV-01.

**Obligation:** REQ-004 supports immutable inline or exact Artifact Python source; REQ-010 retains registered Artifact evidence with normal authorization. TASK-003 supplies concrete capabilities, not placeholder refusal branches.

**Locations:** `H/evidence.rs:115–126`, `H/sandbox.rs:349–362`.

**Trace/evidence:** `InputDeclaration::Artifact` reaches `TenantEvidence::read` from `media`, `execute`, `located` and `Workspaces::open`; the File-only destructure refuses it. `CodeSource::Artifact` reaches shared `CodeExecutor::run` from both action/check implementations; its Inline-only destructure refuses it. Citation lookup turns terminal read refusal into invalid citations. These are explicitly supported seam inputs despite final production composition being TASK-005.

**Consequence:** Accepted registered evidence/source cannot be extracted, mounted, bound as native media or executed; correct Artifact citations cannot survive.

**Smallest correction:** Complete the existing server evidence/source boundary using authorized exact Card resolution and server-authoritative artifact inventory/path selection, then bounded storage reads with stored size/digest validation. Reuse `components/cards/routes.rs::get_card_by_ref_for` (including UID match), Card artifact inventory, `wyrd-storage::service` metadata/path logic and `StorageHandle`. Storage's `download_init` itself assumes route authorization (`components/storage/routes.rs:405–420`, `:576–584`); do not call it as if it authorizes a principal, mint a URL just to fetch server-local bytes, or bypass the existing permission/audit owner. Feed immutable loaded bytes into the same extractor/media/code paths and preserve source digest provenance. No new public wire shape, fetch transport or registry.

**Focused closure:** Valid exact Artifact code and media/extract/workspace/citation paths; wrong tenant, missing grant, wrong kind/UID/path, oversized or mismatched bytes refuse before use. One shared loader proof plus consumer seam cases suffices; no SDK journey rerun here.

### FIND-TASK-003-2 — Worker-group enforcement is soft and owns stale raw PIDs

**Status/classification:** REVISED / VIOLATION plus INCORRECT cancellation. Sources: BEH-02, INV-03, RES-01/04, SYS-003-1/2.

**Obligation:** Deployment requires the hard combined worker memory ceiling, independent allocator/process controls, idle-worker inclusion, safe affected-operation failure and cancellation cleanup. Task scenario 5 forbids substituting unsupported isolation.

**Locations:** `H/sandbox.rs:80–105`, `:184–190`, `:234–239`, `:283–329`.

**Trace/evidence:** `Sandbox::start` constructs a pool and `watch`. `exec` records checkout PID only after checkout/cap and removes it after the running await. `watch` samples `/proc` RSS every 100 ms and kills one raw busy PID after exceeding the ceiling. Starting workers are outside the tracked sets; idle-only excess has no victim. Dropping `exec` bypasses removal. Monty `Checkout::drop` discards/kills the owned child and frees capacity, but cannot remove Wyrd's set entry. Repeated cancelled calls retain stale entries; reused PIDs can be counted/signalled as though still owned.

**Consequence:** Allocations can exceed the approved envelope before sampling, exposing colocated server capabilities to memory pressure. Cancellation also leaves an unbounded second process registry with stale kill authority. The one-byte watchdog test proves eventual killing, not containment.

**Smallest correction:** Replace the sampled memory authority with the conventional Linux worker-only memory cgroup boundary, with configured `memory.max` enforced before worker activity and all warm, starting, replacement and busy workers included; the parent stays outside. Delete `watch`, `resident`, `busy` and their lifecycle plumbing once superseded. Keep the existing pool/checkout/admission/deadline owner and independent per-process/allocator limits. A smaller polling interval, PID guard, per-worker static division of group memory, or server-wide cgroup is not the correction.

The mechanism needs deployment-supplied cgroup delegation; that is an explicit construction prerequisite, not permission to add a privileged server, helper daemon, second scheduler or new deployment topology. Refuse unsafe Python initialization/work when it is unavailable, preserving unrelated server capability. TASK-005 owns deployment wiring. If the allowed deployment cannot furnish this already required mechanism, obey the original task's unsupported-isolation stop; do not ship a fallback or hide a material provisioning decision in remediation.

**Focused closure:** Inspect actual kernel limit and membership, including idle/new/replacement workers and parent exclusion. With small valid limits and normal bounded concurrent workers, prove memory pressure fails affected Python work, parent/siblings survive, and a subsequent operation succeeds. Abort and repeat a bounded number of actual running calls and establish child exit/capacity recovery; source review must establish removal of the duplicate raw-PID kill authority. No host-pressure generator or PID-reuse experiment is needed.

### FIND-TASK-003-3 — Persistent workspace writes receive fresh quotas every call

**Status/classification:** CONFIRMED / INCORRECT. Sources: BEH-03, INV-02, RES-02, SEC-003-02, SYS-003-3.

**Obligation:** Deployment's cumulative `/work` write maximum and total scratch maximum per invocation; read-only inputs and within-invocation persistence.

**Locations:** `H/tools.rs:246–283` (especially `:263–267`); `H/workspace.rs:145–201`.

**Trace/evidence:** `HostedTools::open` retains one Workspace across its tools. Each `Invocation::run` supplies a new full-budget MountSpec to `Sandbox::exec`. Monty `checkout.rs::build_mount_table:2233` constructs a new `Mount`; `monty-fs::Mount::with_root:267` initializes `write_bytes_used: 0`; `common.rs::check_write_limit/commit_write_bytes:347–362` checks/charges only that mount. Workspace's `scratch` exists only during initial placement. Reads' per-file checks do not constrain writes or aggregate bytes. Two individually allowed 30-MiB writes can retain 60 MiB; overwrites also evade cumulative accounting across calls.

**Consequence:** Ordinary permitted code tools exceed both approved invocation disk envelopes, including a first write to near-full input scratch.

**Smallest correction:** Retain one capability-confined native Monty mount/accounting authority with the existing workspace across fresh interpreter feeds, and serialize its mutation at the parent OS-call boundary. Reuse installed `monty-fs::MountTable::handle_os_call`, `Mount::write_bytes_used`, descriptor roots and existing resume/error handling rather than reimplementing filesystem operations. Set its cumulative writable allowance no higher than both the `/work` maximum and scratch remaining after inputs; cumulative successful written bytes conservatively bound retained growth even across overwrites. Do not recreate that accounting per feed, estimate quota from result text, or restore it after a written call later fails. Keep input containment/read-only behavior, blocking IO strategy, fresh workers and eventual directory cleanup.

**Focused closure:** Real mounted writes across sequential and concurrent code calls: each fits alone, combined crossing refuses before the excess write and prior files remain. Include overwrite, successful write followed by Python failure/cancellation, and input bytes leaving less scratch than the work allowance. Small limits suffice. No second disk quota service or new configuration is needed.

### FIND-TASK-003-4 — Python time is copied instead of reserved and resets on retry

**Status/classification:** REVISED / INCORRECT. Sources: BEH-04, RES-03, SYS-003-4.

**Obligation:** Deployment expressly says waiting/execution budgets do not reset across tool calls or retries within the enclosing invocation; cumulative Agent Python time is bounded.

**Locations:** `H/tools.rs:117–124`, `:246–283`; producer `crates/skald/skald-agent/src/loop_runtime.rs:661`.

**Trace/evidence:** The actual Skald tool loop uses `buffer_unordered`. Concurrent calls copy the same remaining Duration, run/wait independently, and subtract only after the await. Two calls can each consume the same remainder. Cancellation before subtraction loses the elapsed charge. Each retry calls `AgentToolset::open` again (`V/src/blocks/tools.rs:35–40` documents attempt lifetime); `HostedTools::open` reinstates the full amount. `BlockCall.id` already identifies the action and executors/toolset are constructed per execution (`V/src/blocks.rs`), so this is repairable without queue state or a new public invocation identifier.

**Consequence:** One enclosing invocation consumes more Python service time than its configured cumulative share; retrying restores spent budget.

**Smallest correction:** Keep cumulative Python budget authority on the existing per-execution HostedTools, associated with the existing action identity across session opens. Reserve from that shared remaining amount atomically before sandbox checkout/wait; settle actual elapsed time, including wait and failure, and return only unused reservation on completion/drop. Reservation ownership must survive cancellation long enough to settle it. Retain the existing per-operation and enclosing deadlines; do not reset spent time on a retry or globally serialize Skald/non-Python tools. This is private accounting of an already approved limit, not a new durable retry resource or queue policy.

**Focused closure:** Concurrent code tools with a small cumulative allowance cannot together consume more than that allowance; actual unused time remains usable. A cancelled/failed operation does not refund consumed time. Opening a subsequent attempt for the same action receives only remaining budget, while a different action/execution is independent. Reuse component/tool seams; no production TASK-005 retry wiring is needed to exercise two opens.

### FIND-TASK-003-5 — Caller-controlled DOCX span expands outside parser bounds

**Status/classification:** REVISED / VIOLATION. Sources: BEH-05, INV-04.

**Obligation:** INV-007/task scenario 2 bounded extraction of semi-untrusted evidence; worker isolation does not cover parent parsing.

**Location:** `H/extract.rs:317–331`, `Extraction::docx_blocks`.

**Trace/evidence:** Small uploaded DOCX bytes pass evidence/zip limits and `Parts::xml`; `gridSpan/@val` parses as arbitrary `usize`. `repeat_n(String::new(), span - 1)` expands directly into the cell vector. Empty strings charge zero text bytes. Extract actions, workspace placement and citation extraction all use this same parent-side function. Blocking-pool execution contains neither allocation abort nor unbounded structural allocation.

**Consequence:** A tiny document requests huge parent memory and can abort the shared server before producing a typed refusal.

**Smallest correction:** Reject invalid/unsupported span sizes before expanding generated cells, accounting the derived structure under the existing extraction resource boundary and supported table geometry. Keep merged covered cells and locked output. Check the immediately related expansion path as part of this fix; do not invent a new service/parser, new public limit family or a general parser-hardening project. A `try_reserve` alone is not a bound.

**Focused closure:** Tiny malicious-span fixture returns a prompt terminal refusal without allocating its requested vector; normal merged and empty cells retain their locked shape. Run the existing nearby extractor cases. No host OOM experiment.

### FIND-TASK-003-6 — UTF-8 stdout truncation panics in the parent

**Status/classification:** CONFIRMED / INCORRECT. Sources: BEH-06, INV-06, RES-05, MNT-003-01, SYS-003-5.

**Obligation:** Safe bounded output; worker-produced values must not panic the parent task.

**Location:** `H/sandbox.rs:194–197`.

**Trace/evidence:** Both direct code action/check and hosted code.run reach `Sandbox::exec`. Its parent print callback slices `&str` at an arbitrary remaining byte count. One byte remaining followed by `é` splits a scalar and Rust panics. The existing ASCII flood test cannot establish Unicode safety. This is a parent-task unwind, not proof of whole-process death.

**Smallest correction:** Use standard character-boundary truncation, already demonstrated by `H/workspace.rs::truncate`, before append. Preserve byte maximum and typed operation behavior; no panic-catching protocol or dependency.

**Focused closure:** Actual worker prints multibyte text crossing a small output limit and at an exact boundary; output is valid bounded UTF-8 without unwind and a subsequent operation succeeds.

### FIND-TASK-003-7 — fs.read presents a truncated unit as complete

**Status/classification:** REVISED / INCORRECT. Sources: BEH-07, INV-05.

**Obligation:** REQ-008 bounded reads and task scenario 2 truncation; the existing tool description promises following `next` to continue.

**Locations:** `H/workspace.rs:421–451`; `H/tools.rs:383–387`.

**Trace/evidence:** `units` can produce a single line/PDF page longer than the result budget. `Workspace::read` truncates its first oversized text but sets `next` only on a later unit. A one-unit file therefore returns null; multiple units skip the omitted suffix. `start` indexes whole units. Estimated text+64 accounting also ignores JSON escaping and actual locator/path bytes; final RecordedTool can replace an otherwise readable response with a size error.

**Consequence:** The model cannot read the tail despite declared extractable evidence and is given an apparently complete original locator. Relevant evidence can be inaccessible.

**Smallest correction:** Make oversized units resumable through the existing private read/unit-index continuation contract, preserving each original locator, with actual serialized-byte budgeting. Deterministically subdividing display units lets the existing start/next cursor work; no new public cursor/citation format or source relabelling is required. Refuse explicitly if even fixed metadata cannot fit. Do not return a silent prefix/EOF or advance past unread text.

**Focused closure:** Read a single oversized line/page fully across bounded responses, reconstruct every character including a relevant suffix, retain locators and valid Unicode, and assert actual encoded response size with quote/control-heavy text. Keep ordinary multi-unit paging. This tests semantic completion; finding 10 separately bounds collection costs.

### FIND-TASK-003-8 — Filesystem tool results omit retained original-slot mappings

**Status/classification:** REVISED / MISSING. Source: INV-07.

**Obligation:** REQ-009 explicitly maps workspace paths back to original input slots without transient-path citations or hidden Prompt injection.

**Locations:** `H/workspace.rs:382–385`, `:391–451`, `:497–503`; consumer `H/tools.rs:224–237`.

**Trace/evidence:** Workspaces take `CallInput.sources` into `Input.sources`. `Workspace::sources` has only a test consumer. List/read/search return path/size or path/locator/text/snippet, without source slots; Invocation forwards unchanged. `AgentExecutor::bind` renders only authored inputs. A workspace-only file at `/inputs/manual.pdf` can derive from slot `policy`, with no model-visible mapping. The concrete validator accepts only original slot `policy`, not the path.

**Consequence:** The promised original-source citation cannot reliably be constructed from the concrete tools; correct file reading can lead to rejected citations/inconclusive downstream checks.

**Smallest correction:** Return the existing original-slot mapping with the existing filesystem results for original pinned files. Preserve ordinary citations supplied in derived output; do not advertise a derived JSON display pointer as a location in every upstream slot, infer mappings from filenames, or inject undeclared Prompt context. Reuse Workspace's retained state and existing Citation/Locator contract.

**Focused closure:** Deliberately different slot and workspace names; obtain mapping and original locator from real fs tool output, carry ordinary cited findings through an explicit downstream binding, and validate through real TenantEvidence. Derived data must not manufacture a location in an unrelated original slot.

### FIND-TASK-003-9 — Citation existence is confused with nonempty display text

**Status/classification:** CONFIRMED / INCORRECT. Source: INV-08.

**Obligation:** REQ-009 accepts locations that exist in pinned evidence; REQ-004 preserves empty/null data and canonical Office positions.

**Locations:** `H/extract.rs:535–568`; consumer `H/evidence.rs::located/cited:69–101,206–246`.

**Trace/evidence:** `leaves` omits null, empty strings and empty containers. `locates` only accepts these emitted leaves or ancestors. Existing `/answer`, `/comment`, `/items` in `{"answer":null,"comment":"","items":[]}` are rejected; an empty root has no location. Office empty paragraphs/merged cells and null error values use the same canonical projection. Agent check then downgrades a pass left without valid citations.

**Smallest correction:** Retain empty/null source locations in the existing shared location projection, or validate pointer existence against the same parsed value while retaining canonical metadata exclusions. Choose the former as the smallest local extension: explicit empty leaf/container locations with empty display text preserve the current validator path. Keep `format`/`kind`/`style` exclusions and out-of-binding filtering; no second parse or evidence wrapper.

**Focused closure:** Existing null/empty string/array/object/root pointers and canonical empty/error-cell pointers accepted; absent siblings/indices and unreachable slots refused. Keep ordinary text tools and invalid-cited-pass behavior.

### FIND-TASK-003-10 — Result maximum is checked after complete producer allocation

**Status/classification:** REVISED / INCORRECT. Source: RES-06.

**Obligation:** Deployment TOOL_RESULT_MAX_KIB is bounded while collecting output; parent buffers are outside worker-group accounting.

**Locations:** `H/tools.rs:285`; `V/src/blocks/tools.rs:172–188`; `H/workspace.rs:391–412`, `:466–503`, `:542–566`.

**Trace/evidence:** code.run calls `value.py_repr()` before building JSON. Monty `object.rs::ObjectRef::py_repr` creates an unlimited String and formats the complete value. RecordedTool subsequently creates another complete Vec through `serde_json::to_vec`, then refuses. `Workspace::list` collects all paths; its `walkdir` first creates a complete sorted vector. Search first collects all work-file units, even when the output budget would stop at an early match. Normal mounted code can create many zero-byte files; byte-write limits do not bound their count. Existing filesystem/service data can also produce large values before the final guard.

**Consequence:** A 64-KiB declared limit permits multi-megabyte or entry-count-driven parent collection before refusal; the worker memory limit cannot constrain those buffers.

**Smallest correction:** Bound representation/collection at hosted producers and final encoding at RecordedTool. Use installed Monty borrowed formatting into a bounded writer instead of eager `py_repr` (public `StringRepr` for strings and `Display` for other values; do not call its private `repr_fmt`), standard bounded serde writers for actual JSON encoding, and bounded/lazy traversal for filesystem results rather than collecting the whole tree/work set first. Preserve Python repr semantics and account for formatter traversal costs as well as retained string bytes; do not duplicate the interpreter or assume a private dependency method is callable. Return existing typed refusal/truncation where appropriate. Retain the final guard, but make its serialization bounded too. Reuse existing Card/Bifrost/BoundedQuery owners and their caps rather than copying their durable logic or demanding new query policy. Do not serialize protected outputs a second time unbounded just to measure them.

**Focused closure:** Large Python last-expression value and many small/empty workspace files reach bounded result/refusal without first building the complete repr/list; quote/control-heavy Unicode obeys actual encoded bytes. Source review plus small-limit tests demonstrates early stop without host load. Check read/search/list and code producer siblings together, and keep digest/returned-byte provenance exact for successful output.

### FIND-TASK-003-11 — Worker keeps the server's host user credential

**Status/classification:** REVISED / VIOLATION. Source: SEC-003-01; follow-up corroborates.

**Obligation:** REQ-008 and Deployment explicitly require a separate unprivileged worker identity as well as empty environment, network/filesystem restrictions and fresh interpreter state.

**Locations:** `W/src/main.rs:9–11`, `:76–115`; existing proof `S/tests/integration/sandbox_worker.rs:93–134`; parent launch `H/sandbox.rs::start` and locked `monty-pool::Worker::subprocess`.

**Trace/evidence:** Monty creates an empty-environment subprocess but changes no host credentials. Worker unshares NEWUSER/NEWNET/NEWIPC without UID/GID mappings and does no set-ID transition before protocol service. Limits, no_new_privs and Landlock remain meaningful controls, but do not change inherited host UID. Namespace comparison/no_new_privs assertions cannot establish a separate host user. The security report's parent-namespace `/proc` observation (UID 1000 in both parent and child) agrees with this source path; this validator did not rerun that experiment. Linux documents namespace ID translation/overflow display separately from credentials used for host checks: [user_namespaces(7)](https://man7.org/linux/man-pages/man7/user_namespaces.7.html).

**Consequence:** The explicitly required separate-user defense is missing for the native interpreter. No Python escape is established, and the other controls must not be weakened or described as absent.

**Smallest correction:** Establish a distinct unprivileged host user credential through conventional OS credential dropping at the existing launch/confinement boundary before serving, remove retained privileged/server-access group authority and capabilities, and verify/refuse when the transition cannot be established. Preserve parent-serviced mounts, empty environment, network/Landlock isolation, process/resource limits and fresh sessions. Ensure process-cap enforcement still works after separation (apply it before dropping the needed authority or inside the existing worker); do not silently lose `prlimit` protection because the parent no longer owns the child's UID.

This needs deployment-furnished ability to make that credential transition. It does not authorize running the credential-bearing server as root, introducing a setuid helper/privileged daemon/container architecture, or selecting new compatibility policy. Those would be material choices if proposed and require their own authority. Implement the approved boundary or obey the task's unsupported-isolation stop; a rootless same-UID namespace is not a fallback. Ordinary credential dropping and a fail-closed construction prerequisite implement an already approved requirement, so this finding itself does not require a spec change. If deployment cannot supply it, do not invent a provisioning architecture inside this remediation.

**Focused closure:** Actual worker observed from the parent host namespace has a distinct unprivileged host UID, no privileged/server-access supplementary group or capability authority, and fails before serving when separation is unavailable. Keep mount/network/session and memory tests. A harmless shared primary GID is allowed; no unique account per worker is required. Namespace inode or overflow UID alone is insufficient proof.

### FIND-TASK-003-12 — Required hosted cross-owner regressions are not authored

**Status/classification:** REVISED / MISSING. Source: BEH-08; invariant matrix and follow-up.

**Obligation:** TASK-003 scenarios 1–6, specifically scenario 6's “Author real protected-owner and queued retry/restart regressions for the final merged pass”. Scenario 2/5 separately require cancellation/resource proof. Deferring final execution is not deferring authoring.

**Locations:** `V/tests/integration/blocks_agent.rs:32–113,427–478`; `H/tools/tests.rs:163–196`; `H/workspace/tests.rs:222–263`; `S/tests/integration/sandbox_worker.rs`.

**Trace/evidence:** Agent tests resolve only inline Cards (`NoRegistry`) and FixedRead. The two-call `check_agent_keeps_only_valid_citations` test exists and passes, but uses PinnedDeck with preselected citations rather than concrete workspace/evidence mapping. Hosted tool tests instantiate an idle never-spawning sandbox; table decisions are explicitly left as prose for later. There is no authored registered-Agent query+code composition, complete catalog dispatch/undeclared-call proof, actual restricted/multi-table Oracle denial, narrowed delegation/distinct-tenant workspace isolation or recovered caller → hosted tool authorization regression in this cumulative diff. Workspace cleanup drops a finished workspace; it does not cancel materialization. Actual-worker cases omit cancellation during parent-serviced file work and independent process-cap breach. Finding-specific closure above supplies the cumulative/Unicode/extractor cases; do not duplicate them under this finding.

**Smallest correction:** Add focused current invoker/tool/evidence seam cases for the missing approved scenarios using existing scripted providers, resolvers and real hosted owners. Author gated real protected-owner and recovered-authority cases in existing integration homes. Reuse the existing accepted-authority/queue claim/reclaim owner (`crates/wyrd/wyrd-sql/tests/integration/pg_verifier_runs.rs`: accepted authority around :583, reclaimed lease :1443, retry :1552), then reconstruct the accepted Caller through the already available HostedTools constructor. Prove tool grants cannot widen Card/table/delegated scope after recovery, and distinct callers/workspaces stay independent. No new production enqueue/ACK route, new queue, test engine or invented future API. TASK-005 owns new-contract acceptance/ACK/queue wiring and complete restart/journey execution.

Run currently executable focused component/seam cases now. Record actual selectors and managed prerequisites for authored gated cases and the final merged pass; a prose promise or disabled placeholder does not count. Cancellation proof may establish eventual cleanup after an already-started blocking task returns; do not demand forcibly cancelling OS threads. No production cancellation mechanism defect beyond the retained findings is inferred solely from missing proof.

**Focused closure:** Review executable authored cases mapped to the exact omitted scenario obligations; use the existing worker, hosted and Agent targets. Add explicit process-cap and cancellation/cleanup cases and the real authority-recovery seam without making TASK-003 implement TASK-005. Final full journeys run once at change review.

### FIND-TASK-003-13 — New dependency-backed tests enter the credential-free lane

**Status/classification:** CONFIRMED / VIOLATION. Source: REPO-003-01.

**Obligation:** architecture/agent-rules.md requires Postgres-dependent tests in `pg_tests`/`pg_*`; TASK-003's non-pg selector must remain environment-free.

**Locations:** `H/tools.rs:453–454`, `H/tools/tests.rs:23,41,68,108,164,199,231`.

**Trace/evidence:** All four tests call `caller`, `state` or `invocation`. `caller` enters `test_tenant`; state enters `test_server_postgres/test_storage/test_catalog`; all enter `S/src/test_support.rs::shared/build_shared`, which starts PgFixture and a real catalog. Ordinary `mod tests` is selected even by `not test(/::pg_tests::/)`. The raw hosted log confirms the four names, but passing under managed Postgres does not make them unit-only.

**Smallest correction:** Move/rename this test module to the sanctioned `pg_tests` namespace and adjust selectors/evidence; preserve protected owner checks and existing managed wrapper. No new fixture/check and no weaker AppState double to evade gating.

**Focused closure:** nextest list shows all four cases in `verification::hosted::tools::pg_tests::`, non-pg selection excludes them, and exact cases pass with repository-managed Postgres/migrations.

### FIND-TASK-003-14 — Changed Rust items omit mandatory rustdoc contracts

**Status/classification:** CONFIRMED / VIOLATION, explicit hard documentation gate. Sources: REPO-003-02, MNT-003-02.

**Obligation:** AGENTS §16 and architecture/agent-rules.md require all changed Rust items, including tests/private helpers/constants, to have substantive rustdoc; fallible functions need Errors, remaining panics need Panics and applicable cancellation/partial-progress behavior must be documented. The review skill preserves this as blocking.

**Locations/evidence:**

- `S/src/verification/eval.rs:863,876`: materially changed resolver methods lack Errors; they propagate registry failures and wrong-kind refusal.
- `V/tests/integration/orchestrator_judge_skald.rs:61`: changed StaticResolver method lacks rustdoc entirely.
- `V/tests/integration/blocks_agent.rs:221,243`: fallible step/check helpers lack Errors; `:104,118,130,407` helpers retain expect without panic docs.
- `H/extract/tests.rs:178`, `H/workspace/tests.rs:29`, `S/tests/integration/sandbox_worker.rs:55`: fallible helpers lack Errors; the worker helper also explicitly panics on non-object input without Panics.
- `H/mod.rs:67` (MIB), `H/extract/tests.rs:75,138` (P/NS): undocumented new constants.
- `H/tools.rs:373` descriptor has catalog `expect` at :407 without Panics. New fixture helpers with expect/indexing and absent panic docs include `H/extract/tests.rs:14,29,74,119,137,183`; `H/workspace/tests.rs:14,24,34`; `H/evidence.rs:324`; `H/tools/tests.rs:68`.

**Consequence:** The candidate misses an explicit merge obligation on changed failure/invariant contracts; green Clippy does not replace this source gate.

**Smallest correction:** Complete the existing item docs in one pass, including actual errors/panic invariants and relevant asynchronous cancellation/partial-progress semantics (worker checkout destruction and detached blocking parsing/materialization). Keep behavior; do not rewrite unrelated historical helpers or introduce a prose checker. Update misleading worker `nobody`/quota docs with the corresponding behavioral fixes rather than a separate style round.

**Focused closure:** Source review of the complete changed-item set plus normal owning format/lint checks; no runtime test for prose alone.

## Reconciled acceptance matrix

| Original obligation / constraint | Pinned implementation and proof | Result |
|---|---|---|
| Shared executor seam, Vala/Skald/server ownership and source reuse | Agent::run_prompt, ToolRegistry, protected Card/Bifrost/BoundedQuery owners; narrow server parser/pool dependencies; actual diff | PASS |
| Exact Agent/Prompt resolution, explicit inputs, route replacement and check-position refusal | AgentExecutor complete methods; 9 passing Agent cases exercise inline/schema/refusal/route seams | PASS for implemented seams; required registered/composed proof FAIL (12) |
| Inline code action/check, fresh interpreter and ordinary worker containment | CodeExecutor/Sandbox, empty environment, mounts, namespaces/Landlock; 5 passing worker cases | PASS ordinary supported cases; Artifact FAIL (1), identity/resources/output FAIL (2–6,10–11) |
| Pinned file size/digest/format/native-media behavior | TenantEvidence shared read; raw hosted evidence cases pass | PASS uploaded File; Artifact FAIL (1) |
| Nine locked extraction shapes, canonical locators and safe format refusal | Shared Extraction, fixtures, passing hosted cases | PASS ordinary shapes; bounded DOCX expansion FAIL (5), empty citation locations FAIL (9) |
| Read-only declared inputs, safe archive/path handling and scratch ownership | Workspace/TempDir/permit, archive guards; passing existing cases | PASS ordinary paths; cumulative writable/total scratch FAIL (3), cancellation proof FAIL (12) |
| Bounded read/search/list and original-slot mapping | Shared units and tool dispatch | FAIL (7,8,10) |
| Ordinary cited preparation output → explicitly bound tool-less judge | Two-call double-based test exists; concrete cited/read mapping incomplete | FAIL concrete proof (8,12), location filtering FAIL (9) |
| Full closed built-in catalog, invoke before domain permission, accepted authority | Source uses correct protected owners/caller; current direct permission cases pass | Source behavior retained; required complete/real/recovered-authority proof FAIL (12) |
| 2 prewarmed / 8 maximum, bounded waiters and end-to-end operation deadline | Pool/admission/timeout source; passing saturation/time cases | PASS ordinary policy; cancellation/independent process proof incomplete (12) |
| Hard group, independent allocator/process, separate worker identity | Sampled watchdog and same host UID contradict requirements | FAIL (2,11) |
| Per-invocation cumulative Python/work/output limits | Fresh mount quotas, copied/deferred time debit, late result guard | FAIL (3,4,6,10) |
| Provenance route/code/extractor/tools and result digest/byte count | Existing provenance structs and recording path, assertions in passing Agent/extract tests | PASS supported normal paths; missing Artifact path (1), bounded producer proof (10) |
| Environment-free scoped tests and mandatory changed-item docs | PgFixture reached from ordinary module; exact omissions above | FAIL (13,14) |
| Excluded endpoints/ACK queues/schedules/Operators/SDK/graph implementation | Complete cumulative changed-file/diff review | PASS; leave TASK-005 production composition and final journeys excluded |
| Focused verification credibility | Raw logs: 9 Agent + 21 hosted + 5 actual-worker pass; migration setup passes | PASS existing cases; not evidence for omitted boundaries |

## Verification and decision limits

The three raw logs establish the tests actually selected and passing; they do not prove missing cases. The task snapshot's format/Clippy/dependency/workspace-hack claims remain reported evidence, not checks rerun by this validator. No failing test was diagnosed from a panic line, no unsafe load was generated, and no baseline checkout/bisect was used.

Remediation should run exact focused selectors through mise for newly authored cases, the managed Postgres wrapper for pg tests, the real-worker owning lane, and format/lint/dependency checks for its actual write set. Do not prescribe a full server/Vala/SDK journey suite or repository gate merely because this ledger is substantial. Whole-change integrated execution remains change review/TASK-005. No generated public contract change is selected by these corrections; if one is later proposed, obtain its governing authority and appropriate proof rather than silently adding it.

All fourteen retained findings concern required current behavior, proof or the explicit rustdoc gate. None requires weakening the approved spec. Kernel delegation and worker credential transition are prerequisites of the already approved isolation boundary; lack of support is a task stop, not an authorized fallback. A future proposal for privileged launch infrastructure, a changed public API/security boundary or different deployment architecture must return for material decision authority; this validation does not approve one. With those limits, the bounded correction outcomes above support FIX_REQUIRED rather than inventing a specification revision.

Skipped full suites, live providers/clouds and privileged/host-pressure experiments. Risk: the pinned candidate does not meet mandatory containment/resource limits, and green existing cases cannot justify shipping it.
