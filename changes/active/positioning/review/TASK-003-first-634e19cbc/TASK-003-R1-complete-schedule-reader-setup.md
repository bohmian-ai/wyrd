---
id: TASK-003-R1
kind: remediation
status: proposed
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-008, REQ-012, REQ-014, INV-004, INV-005, AC-007, AC-011, AC-013]
depends_on: [TASK-003]
parent_task: TASK-003
remediates: [FIND-TASK-003-1, FIND-TASK-003-2]
---

# Complete schedule reader setup and required Rust documentation

Route directly to `$wyrd-implement`. A reader following Setup directly into
“Check on a schedule” can run its documented CLI commands and TypeScript
readback without discovering missing setup. The existing four tutorial
programs, scheduler behavior and proof remain intact.

## Authority and immutable context

- Approved spec: `changes/active/positioning/spec.md`, revision 5.
- Original task: `changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`,
  working-file content pin `6846032793910170e76c0cc0ec9de2f50cb6ad6f`,
  SHA-256 `9299f0945b3a0ae1e8f2c3641d3bd44255f41b3338ca1e6381af3e61b02b6fb9`.
  This is a content hash, not a stored Git object. Preserve the original task.
- Base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
- Reviewed candidate: `634e19cbc517293ed2458f30ce73401714f3447d`.
- Verdict and independently validated ledger: sibling `verdict.md` and
  `findings-validation.md` in this review directory.
- Root AGENTS, docs AGENTS and `agent_docs/rust.md` remain governing rules.

## Diagnosis and selected correction

### FIND-TASK-003-1: the declared reader path omits its execution prerequisites

`docs/src/content/docs/build-and-verify/verify/monitor-drift.md:16` names
Setup as its prerequisite. Setup installs the CLI inside the Python/TS
project and teaches `uv run wyrd` / `pnpm wyrd`; it does not guarantee a global
`wyrd`. The schedule guide instead uses bare `wyrd` for declaration, pull and
principal creation (`:30`, `:39`, `:68`). Its embedded
`examples/scheduled-check/results.ts:3` also imports Zod, while Setup installs
only `wyrd-ai@0.4.2` and the schedule guide never installs Zod. The SDK does
not supply that direct dependency contract.

A reader with exactly the stated prerequisites cannot reliably complete the
required procedure. Existing schedule journeys run declaration/pull/principal
operations through SDK calls, bypassing the CLI commands. The TS journey
launches the repository-resident program, so its imports resolve from the
examples workspace, where Zod already exists. A reader cwd alone does not
isolate module resolution. Passing those tests therefore leaves this precise
setup obligation unproved; their runtime/scheduler assertions remain useful.

Correct the schedule guide at its existing source owner. Use the request
guide's existing language tabs for CLI commands: Python `uv run wyrd`, Rust
`wyrd`, TypeScript `pnpm wyrd`. Apply this to declaration, pull and runtime
principal creation. Add the investigation guide's existing direct Zod install
instruction to the TypeScript program setup, before its readback is run.
Preserve command arguments, credentials, workload restrictions, paths and the
actual query/parser. Regenerate derived agent text through the existing docs
generator.

This closes the producer of the missing setup, with no downstream guards.
Requiring the finance Quickstart, a global CLI install, program relocation, a
parser replacement or a new dependency in the repository would add scope and
cost. Removing TypeScript/readback would violate the approved tutorial outcome.
The chosen correction is limited guide text using established components and
already-used packages.

### FIND-TASK-003-2: new Rust items fail the explicit documentation gate

The new constants `ORDER_1042` at
`examples/refund-agent/refund_agent.rs:21`, `FAILED` at
`examples/refund-agent/investigate.rs:14`, and `RECENT` at
`examples/scheduled-check/results.rs:11` have no rustdoc. They respectively
feed the real/skipped order paths, retained-failure query and scheduled-result
query. `now()` at `examples/tests/scheduled_check.rs:96` has an `expect` on
the clock being after the Unix epoch; `clear_of_boundary()` at `:105` calls
it. Neither documents the possible panic. Root AGENTS explicitly makes missing
changed-item rustdoc merge-blocking, including private and test items. Green
compiler/lint runs do not satisfy this broader gate.

Add concise workflow/intent documentation to those three existing constants
and document the clock-before-epoch panic on both helpers under `# Panics`.
Preserve all values, SQL, signatures, clock behavior, waits and callers. Five
local documentation additions suffice; deleting needed behavior or refactoring
the clock would solve a different problem at greater cost. Batch this with the
reader correction rather than creating a separate style round. Regenerate
published text if its full-file embeds project the added comments.

## Constraints and preserved behavior

Keep all four routes, language coverage, exact output fixtures and existing
actual-program assertions. Preserve native pass/fail/inconclusive/error
handling, finished original spans, retained evidence/version attribution,
bounded visibility and tenant refusal. Keep the existing server schedule,
binding, workload grants, samples, natural minute windows and empty-window
behavior. No runtime, scheduler, public API, auth, schema, migration, hot-path
wait, framework, dependency or test-harness change is authorized.

No optional Rust selector harmonization is requested: independent validation
rejected that proposal as lacking a failing required tutorial path. Do not
modify unrelated dirty files or commit unless separately asked.

## Acceptance and focused proof

| Finding | Required closure |
|---|---|
| FIND-TASK-003-1 | A direct Setup → schedule reader has the documented local CLI invocation for each shown language and the direct dependencies needed by the actual TypeScript results program. No incidental workspace import/global CLI supplies missing setup. Generated text agrees with the corrected guide. |
| FIND-TASK-003-2 | All five named Rust items satisfy the explicit documentation rule, with no behavior change. |

These are documentation corrections, so use static/reader verification rather
than manufacturing a runtime RED. First record the existing missing imports,
commands and rustdoc, then demonstrate closure. For reader proof, use an isolated
location with only the documented dependencies and place the actual
`results.ts` there. Establish that its imports resolve from that location;
do not launch the repository-resident file and call its cwd isolation proof.
Check that documented Python/TS project-local CLI invocations reach the existing
commands. Record exact commands and observations. No new harness is needed and
no real minute wait is necessary to prove these setup corrections.

Use the existing owners and narrow lanes:

```bash
mise run docs:generate
mise run docs:check:commands
mise run docs:linkcheck
mise run docs:test
mise run fmt:check
git diff --check
```

Inspect the five Rust items against their rule. The docs content/index tests
cover the corrected publication and generated text; runtime behavior has no
new branch to test. Preserve supplied actual-program/scheduler proof and carry
forward unaffected acceptance rows at remediation review. Do not require full
three-language journeys, broad lint aggregates, benchmarks or live providers
for this write set; full journey suites belong to change review.

## Implementation Evidence

**Before the fix.** `monitor-drift.md` ran bare `wyrd` for declare, pull and
`create-principal` (lines 31, 32, 40) and never installed Zod, which
`results.ts` imports. The five named Rust items had no rustdoc or `# Panics`.
`which wyrd` on this host prints `wyrd not found`, so a Setup-only Python or
TypeScript reader cannot run the bare commands.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-1 | `monitor-drift.md` steps 1 and 2 now use per-language tabs: `uv run wyrd`, `wyrd` (Setup's `cargo install wyrd-cli`) and `pnpm wyrd`, with arguments unchanged. Step 3's TypeScript tab adds `pnpm add zod` before the programs. Regenerated `llms-full.txt` holds all three corrections | Isolated readers, each against a fresh `WyrdTestServer` and repository Postgres, driven by a session-scratchpad script outside the repository. **TypeScript:** a fresh `pnpm init` project holding only the packed `wyrd-ai@0.4.2`, its packed `wyrd-ai-darwin-arm64` addon (the platform package npm installs) and `zod`, plus a copy of `results.ts` and `checkout/`. Before `pnpm add zod`, `import('zod')` gives `ERR_MODULE_NOT_FOUND`. After it, `pnpm wyrd declare checkout/checkout.yaml` declares 3 Cards, `pnpm wyrd pull ... --output-dir .wyrd-bundle` pulls 3 Cards, `pnpm wyrd principal credential create-principal --name checkout-runtime --role workload --workload-name checkout` prints a credential, and `node results.ts` exits 0, printing `latency-budget@1.0.0 on checkout@1.0.0`. **Python:** the same three commands as `uv run wyrd ...`, through the `wyrd` entry point that `uv add wyrd-ai` installs (from the SDK project's environment), then `results.py`, with the same outcomes. This host has no global `wyrd`. **Generated text:** a second `docs:generate` leaves `llms-full.txt` byte-identical. Its diff carries the Python and TypeScript tabs for both steps, the Zod install and the three new rustdoc comments from full-file embeds; the Rust tab keeps the page's existing `wyrd ...` lines. `docs:check:commands`, `docs:linkcheck` and `docs:test` (102) pass | PASS |
| FIND-TASK-003-2 | Rustdoc on `ORDER_1042` (`refund_agent.rs`), `FAILED` (`investigate.rs`) and `RECENT` (`results.rs`); `# Panics` on `now()` and `clear_of_boundary()` (`tests/scheduled_check.rs`). Values, SQL, signatures and clock behavior are unchanged | Inspection against root AGENTS rustdoc rule; `fmt:check`; `cargo check --locked -p wyrd-rust-examples --bins --tests` | PASS |

`git diff --check` is clean. No runtime, test-harness, dependency or output
fixture changed, and no other file was touched. The four-tutorial proof carries
forward unchanged.
