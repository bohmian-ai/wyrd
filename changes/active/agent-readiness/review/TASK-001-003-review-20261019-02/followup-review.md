# Focused follow-up: parent tracing sink failure

**RESOLVED** — healthy-file routing closes the ordinary path, but does not close FIND-TASK-002-1. The pinned subscriber has a reachable stderr fallback after a successful log open. INV-R2-001 identifies that remaining path correctly. This is source resolution for independent validation, not an overall task verdict.

## Subject and inspected evidence

Root `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`; cumulative base `1b962bb0052e58c65af8c45be3ee0d1a04d483b6`, candidate/observed HEAD `d6b7c75b88db5eb144e6c282263f51692371bff1`. Inspected the relevant cumulative CLI diff; R1 identifies the newly shared subscriber/file opener. Review scope is only the conflicting FIND-TASK-002-1 closure claims.

Read `.agents/skills/wyrd-task-review/SKILL.md`, repository agent rules and applicable specification/maintainer authority; inspected current design/doctrine protocol and client boundaries. No `.codegraph/` exists according to the immutable subject. Specific acceptance authority is approved `changes/active/agent-readiness/spec.md:219–239` (REQ-006): diagnostics go outside both machine streams; a failure emits one structured stderr document without appended prose. Original `tasks/TASK-002-machine-readable-local-tooling.md:65–85` preserves that whole-stream contract. Prior `review/TASK-001-003-review-20261018-01/findings-validation.md:41–53` and R1 diagnosis/acceptance require correcting the process producer without command-local scrubbers or a logging framework.

Compared current `task-review-behavior.md` closure row for FIND-TASK-002-1, `maintainer-review.md` closure row, `system-review.md` closure/failure-path rows, and `task-review-invariants.md` INV-R2-001. The first three establish selection of an ordinary file destination; they do not inspect the subscriber's later write-failure behavior. The system report's statement that later writes are best effort is consistent with intended diagnostic loss policy, but its claim of no fallback is false for the parent subscriber.

| Inspected source | Evidence |
|---|---|
| `crates/wyrd/wyrd-cli/src/main.rs:21–28` | Installs `install_tracing` before shared dispatch; an installation/open error produces one boundary error and returns. |
| `crates/wyrd/wyrd-cli/src/lib.rs:107–155` | JSON with valid `RUST_LOG` opens `logs/cli.log`, installs `fmt()` with `Mutex<File>`, and does not change the internal-error policy. The opener only creates the parent and opens for append; it does not write/reserve future space. Human subscriber is separate. |
| `crates/wyrd/wyrd-cli/src/lib.rs:163–194`; `src/error.rs:465–486` | Dispatch errors print one JSON problem. This consumer cannot retract subscriber fallback bytes written earlier. |
| `crates/wyrd/wyrd-cli/src/server/dev.rs:147–208` | Existing `Diagnostics::open` shares the opener. `progress` explicitly documents dropping failed writes; both progress and notice drop `writeln!` failures. Child stdout/stderr are attached to cloned log handles. There is no parent-stderr fallback in these methods. |
| `crates/wyrd/wyrd-cli/tests/card_lifecycle.rs:887–905,2434–2469` | Logged success and unreachable-get cases use `RUST_LOG=debug`, assert whole streams and normal-file retention/no credential. They use a normal temporary filesystem and never force a post-open write error. |
| `Cargo.lock:9550–9551`; CLI manifest line 54 | CLI consumes workspace tracing-subscriber, locked to 0.3.23. |

Exact dependency source root inspected: `/home/thorrester/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tracing-subscriber-0.3.23/`:

- `src/fmt/mod.rs:466–474`: `SubscriberBuilder::default` explicitly calls `.log_internal_errors(true)`.
- `src/fmt/mod.rs:656–674`: the existing public builder policy documents true by default and stderr fallback when writing fails; the setter forwards to the layer.
- `src/fmt/writer.rs:808–845`: `Mutex` yields a guard writer that forwards `write_all` to its underlying writer. It does not absorb file errors.
- `src/fmt/fmt_layer.rs:1027–1064`: `on_event` formats an ordinary event, calls `write_all`, and, when the enabled flag sees an error, directly invokes `eprintln!`. The formatter-error branch has the same failed-writer fallback. This bypasses the selected file writer.

## Reachability and consequence

A valid ambient `RUST_LOG=debug` selects the JSON file subscriber. Opening an append file successfully does not guarantee later writes: a filesystem can exhaust space/quota after open, including during a long foreground command. No race, credentials, malformed formatter, or platform-specific product feature is needed. A normal HTTP/TLS diagnostic event during required `get`/installer/dev work is then formatted; the failed file write reaches the dependency's raw stderr fallback. An unreachable get subsequently prints its structured failure, leaving stderr as prose plus JSON; a successful command may instead have unexpected nonempty stderr. The parent subscriber is shared by the required native commands, so the correction belongs once at its JSON construction, not at each command/error renderer. Embedded SDK callers own their own subscribers and are not new correction targets.

The invariant review's isolated `/dev/full` probe (21 stderr lines) corroborates this source path, but was not rerun here and its existing binary was not rebuilt. It is reported peer execution evidence, not fresh follow-up execution or a portable product/test prerequisite. Source alone establishes the failed-write path precisely.

## Source-local proposed finding

**FOLLOWUP-001**, classification **INCORRECT**, discovery relationship **INV-R2-001 / existing FIND-TASK-002-1**. Location: `crates/wyrd/wyrd-cli/src/lib.rs:132–136`. Violated obligation: REQ-006/AC-007 and R1's exclusive machine-stream diagnostic policy. Consequence: a reachable diagnostic write failure appends subscriber prose to machine stderr and defeats whole-document parsing. Preserve the stable prior finding ID; this is the same parent tracing producer's incomplete routing correction, not a new independent logging requirement.

Smallest supported correction: select the installed builder's `.log_internal_errors(false)` in the JSON subscriber branch. It prevents this built-in stderr fallback while continuing to attempt ordinary event writes to the chosen file. Preserve human stderr behavior, current pre-command structured refusal when the file cannot open, normal-file requested diagnostic retention, command errors/exits, and dev child ownership/readiness/routing. No alternate sink, second logger, public option, durability guarantee, fail-the-command callback, or new framework is necessary.

This does not redefine successful diagnostics as disposable: normal requested events still go to the file. It accepts loss when that diagnostic sink actually refuses a write, matching the already explicit `Diagnostics::progress`/`notice` policy. REQ-006 does not promise durable logging through disk failure or authorize fallback onto reserved machine streams. R1's instruction against suppressing diagnostics prevents disabling requested tracing as the normal solution; it does not require printing failed-write housekeeping onto machine stderr. The built-in policy also disables internal formatting-error reporting in this branch; it does not disable ordinary event formatting/writing or suppress domain errors.

Focused closure proof: exercise a writer that accepts setup and then returns an IO error when an actual event is written; assert clean complete machine streams, alongside existing normal-file retention/error/secret assertions. A small portable failing `Write` test seam can supply this failure; a Linux device must not become a product or cross-platform test dependency. There is no need for broad suites or new logging machinery to prove this boundary.

No source edits, Cargo execution, new runtime probe, broad checks, or unrelated finding review were performed. Remaining risk is machine stderr corruption on a post-open diagnostic sink failure until this policy is corrected; diagnostics may still be lost when their sink fails.
