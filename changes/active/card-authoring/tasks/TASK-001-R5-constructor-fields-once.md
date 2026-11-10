---
id: TASK-001-R5
kind: remediation
status: implemented
spec: SPEC-card-authoring
spec_revision: 5
parent_task: TASK-001-R4
remediates: []
---

# Keep constructor arguments constructor-only

## Outcome and value

Remove the redundant editing API. A value supplied as a fixed constructor
argument is supplied there once; it has no later setter or incremental edit.
Action constructors return Action and Check constructors return Check. Delete
EachCheck, AgentCheck and AgentAction, their exports and their conversions.
Mixed Check arrays no longer need conversions introduced for those types.
There is no replacement API.

The maintainer explicitly approved [spec revision 5](../spec.md) and requested
this remediation after rejecting R4's extra receiver types and duplicate edits.
This is a human-directed change of approved behavior, not a new implementation
finding. In particular it supersedes FIND-TASK-001-9's duplicate-setter target;
do not restore those setters to satisfy the historical review.

Starting point: R4 commits 42b3a7e58 and 9be515a10; exact previous authority is
[spec-revision-4.md](../spec-revision-4.md). Other R4 corrections stay intact.

## What changes and what must stay true

Apply the revision-5 support tables throughout the common Rust public API:

- Card name/version; Component alias/ref; Binding verifier; Step id/action;
  Requirement id/statement/check are constructor-only.
- Slot schema, formats, features and source values supplied by their constructor
  have no later edits. Remove add_format/add_feature too. Keep required and
  max_bytes where canonical slots support them.
- Every Action/Check fixed constructor argument is constructor-only, including
  extract's from, assert's path/operator/expected, each's each/operator/expected,
  agent's agent, code's source/entrypoint and trace/drift arguments.
- Keep fields not fixed by constructors and their collection additions:
  Verifier inputs/steps/requirements; Service components/verified_by;
  Component verified_by; agent-call inputs/workspace and other supported fields.
  Required spec fields such as Prompt provider/model remain editable because
  named Card constructors take only name/version. Python kwargs and TypeScript
  options do not make spec fields constructor-only.
- Preserve read-only CardRef accessors, canonical raw input support and the
  existing native types. Do not introduce another grammar, variant enum,
  replacement helper, alias, receiver type or validator.

Capture, canonical equality, duplicate refusal, deferred validation and file
reads, Artifact reference intent, local dependency admission/order, Artifact
root receipts, upload/declaration reuse and Model verification through Components
must retain their R4 behavior. No server, wire, registry or upload change.
This intentionally removes newly introduced authoring calls; no compatibility
aliases. Future Python/TypeScript projections must follow revision 5; their
unimplemented wrappers are outside this bounded Rust task.

## Approach

1. Remove duplicate public field methods and receiver types at their existing
   owners; retain the remaining methods and their existing capture behavior.
2. Adjust existing callers, public inventory and snapshot tests to construct
   fixed values directly. Remove only conversions necessitated by the deleted
   receivers; keep unrelated conversions and all native-family coverage.
3. Update the owning architecture subsection and affected rustdoc/examples to
   describe constructor-only fields and ordinary Action/Check return values.
4. Run focused compilation, regression and relevant journey checks; record
   exact commands and outcomes below before requesting review.

## Acceptance and proof

| Obligation | Required observation and proof |
|---|---|
| REQ-002 / AC-007/008: one entry for fixed fields | Source/export inspection finds no duplicate fixed-field setters, add_format/add_feature or three receiver types. Public inventory assigns agent/each constructor results directly to Action/Check and mixes each/other checks in a Check array without receiver conversions. |
| Remaining builders and grammar | Existing public native-family inventory still covers every family/operator/slot. Existing constructors, non-fixed field replacement and incremental edits produce their canonical values. No reduced variant coverage or new helpers. |
| Capture and deferred validation | Existing six authoring tests retain snapshot, active dependency, duplicate, raw malformed input, file/code parity and deferred-file assertions. Rewrite obsolete fixed-field edit assertions around direct construction; preserve independent non-fixed replacement tests. |
| R4 Artifact corrections | Existing real-server Artifact journey still declares a local Artifact root and nested schema/payload/Audit chain with the published schema UID. Preserve current error and receipt assertions. |
| Documentation and consumers | Owning architecture, rustdoc, SDK inventory and finance example compile/describe the same surface; file/native declaration callers retain behavior. |

This is removal of redundant API with a constructor return-type correction,
not a new runtime decision. No manufactured behavioral RED is required for
method absence. Before changing return types, add ordinary Action/Check type
assertions to the existing public inventory and record their compiler failure
against R4, then make them compile. Source review establishes deleted methods;
existing runtime regressions establish preserved semantics. Do not add a new
compile-fail harness or tests that merely mirror the deleted method list.

## Implementation reference

- `crates/shared/wyrd-client/src/authoring/verifier.rs`: fields! invocations
  expose duplicate InputSlot/Step/Requirement/Action/Check fields; the three
  receiver structs and their From implementations exist solely to expose
  constructor-name setters. Reuse existing Body/capture conversions directly.
- `authoring/mod.rs`: receiver reexports and card_metadata! name/version
  setters; retain labels/annotations and CardRef read-only name/version.
- `authoring/kinds.rs`: Component/Binding fields! expose constructor fields.
  Existing constructors already store those values; no new owner is needed.
- `crates/shared/wyrd-client/src/authoring/tests.rs` and
  `sdks/wyrd-sdk-rust/tests/integration/authoring.rs`: existing parity, capture,
  public-path and Artifact proof. The public inventory currently converts an
  EachCheck in its operator array and exercises the four superseded edits.
- `architecture/wyrd-design.md`, In-memory authoring: narrow documentation owner.
  `examples/finance-release/finance_release.rs`, Rust SDK reexports and packet
  language sketches are consumers to inspect; change only affected calls/docs.
  The packet sketches already use constructors without the rejected edits.

## Verification and evidence

Use repository mise tasks and exact nextest commands for each test named:

```sh
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=authoring::tests::offline_edits_preserve_snapshots) | test(=authoring::tests::replacements_keep_only_current_dependencies) | test(=authoring::tests::field_edits_defer_validation_and_preserve_snapshots) | test(=authoring::tests::yaml_and_code_share_diagnostics) | test(=authoring::tests::core_primitives_match_canonical_contracts) | test(=authoring::tests::declarations_match_file_authored_tree)'
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps
mise run fmt:check
mise exec -- git diff --check
```

Record the initial public compilation failure, resulting successful commands,
source/export deletion check and any affected consumer checks. Broaden only for
new changes or failures. Do not rerun unrelated language lifetimes or full gates.

## Implementation evidence

Commit `c24f5bae7` (on R4 `9be515a10`). Existing owners only. No new type, helper,
grammar, validator or dependency. One private `InputSlot::listing` constructor
step replaces the deleted `formats`/`features` setters that the `media`/`samples`
constructors had used internally.

**RED (pre-change, against R4):** the public inventory was annotated with
`let mut call: Action = Action::agent(..)` and `let mut check_call: Check = Check::agent(..)`,
and its `.into()` was dropped from `Check::each(..)` in the mixed operator array.
`cargo check --locked -p wyrd-ai --tests` failed with 3× E0308 at
`sdks/wyrd-sdk-rust/tests/integration/authoring.rs:1550:28`, `:1555:9`, `:1601:17`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 / AC-007/008: one entry for fixed fields | `authoring/verifier.rs`: `Step` id/action, all `Requirement` fields, `InputSlot` schema/formats/add_format/features/add_feature and `Action`/`Check` from/path/operator/expected/on/span_selector/source/entrypoint/spec setters removed; `Action::agent`, `Check::agent` and `Check::each` return `Self`; `AgentAction`/`AgentCheck`/`EachCheck` and their `From` impls deleted. `authoring/mod.rs`: their re-exports and the `card_methods!` name/version setters removed. `authoring/kinds.rs`: `Component` alias/ref and `Binding` verifier setters removed | `git grep` over `authoring/` and `sdks/wyrd-sdk-rust/src` finds no receiver type, add_format/add_feature or name/version setter, and no fixed-field `one` entry. The only hits are the editable spec fields Data `schema`, Operator `action` and Component `source`. The public inventory compiles with direct `Action`/`Check` bindings and a receiver-free mixed Check array | PASS |
| Remaining builders and grammar | Kept: inputs/workspace/runtime (Action), inputs/workspace/runtime/response_format (Check), required/max_bytes (InputSlot), Step depends_on/when/requirements/timeout_seconds/retries, Binding runs_on/on_failure/on_verdict_change, Component verified_by/source/config/credential_refs, labels/annotations, all spec setters | `authoring::public_paths_author_every_native_family` PASS (56-operator catalog, every family, slot and Check form unchanged) | PASS |
| Capture and deferred validation | No change to `Body`/capture code. The obsolete fixed-field edit block in the inventory is rewritten as direct construction and still asserts the discriminants, one declared Agent and one published mounted Artifact. Service whole-field replacement assertions are unchanged | All 6 named `wyrd-client` `authoring::tests` PASS without edits | PASS |
| R4 Artifact corrections | Untouched | `authoring::local_artifacts_reuse_and_keep_versions` journey PASS (real server + Postgres) | PASS |
| Documentation and consumers | `architecture/wyrd-design.md` In-memory authoring states fixed constructor arguments and ordinary Action/Check returns. Rustdoc for `card_methods!` updated. The finance example and the Rust SDK re-exports needed no change | clippy `-D warnings` over wyrd-client/loader/ai/rust-examples all-targets clean; `cargo doc` clean | PASS |

Commands (all passed):

```sh
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E '<six authoring::tests selectors above>'   # 6 passed
mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -E 'test(=authoring::public_paths_author_every_native_family)'   # 1 passed
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "... -E 'test(=authoring::local_artifacts_reuse_and_keep_versions)'"   # 1 passed
mise exec -- cargo clippy --locked -p wyrd-client -p wyrd-loader -p wyrd-ai -p wyrd-rust-examples --all-targets -- -D warnings
mise exec -- cargo doc --locked -p wyrd-client -p wyrd-loader -p wyrd-ai --no-deps
mise run fmt:check
git diff --check
```

Non-goals held: no server, wire, registry, upload, Python or TypeScript change.
No compatibility alias or replacement helper. Python's `verify/__init__.pyi`
`AgentCheck` is the wire TypedDict for the agent check shape, not the removed
Rust receiver, and is unchanged.

