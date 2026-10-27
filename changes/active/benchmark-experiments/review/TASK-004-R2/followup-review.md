# TASK-004-R2 focused independent source validation

**RESOLVED — CONFIRMED**, limited to the new `Pending` tuple field under existing **FIND-TASK-004-5**. No new finding identity or broader documentation sweep.

## Subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Prior candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
Candidate: `c9d72729b85187f2399725e0ed9496c4c4a39298` (also HEAD when inspected).
Approved specification revision 8 and TASK-004 context carry forward as recorded in `scope.md`.

Inspected the task-review skill, root AGENTS, `agent_docs/rust.md` Rustdoc section, R2 scope, and prior F5 ledger/remediation only for the applicable correction boundary. CodeGraph preceded source navigation. Independently read the committed remediation diff and exact candidate source, including the complete `label_request` and `activate_request` bodies and every `Pending` occurrence in that source. Other review reports and conclusions were not used as proof.

## Source and rule proof

| Question | Independent evidence | Result |
|---|---|---|
| Is this a changed field? | The prior-to-candidate diff adds the entire `Pending` declaration. Candidate `crates/shared/wyrd-telemetry/src/timing.rs:172` is `struct Pending(Labels);`. The claim's approximate line 173 is blank in this immutable candidate. | New private tuple field; in scope. |
| Does the field already have rustdoc? | Lines 169–170 document the enclosing `Pending` type. The `Labels` token inside the tuple declaration has no field doc attribute/comment. `Labels` and its own case/phase fields are separately documented at lines 138–145. | Type and nested-type docs do not attach rustdoc to this tuple field. |
| Does authority require field docs? | `agent_docs/rust.md:235–241`: “Every new or materially modified item needs rustdoc regardless of visibility, including fields, variants, private helpers, test helpers, and tests.” Root `AGENTS.md:152–154` requires changed-item rustdoc and states “Missing rustdoc blocks merge.” Neither rule exempts private single-field wrappers or allows inherited field documentation. The prior F5 remediation explicitly includes its own new/changed items. | Explicit mandatory gate remains unmet. |
| Is the item used and what does its field mean? | `label_request` constructs closed `Labels` at lines 189–192 and inserts `Pending(labels)` into the request span extensions at line 198. `activate_request` removes/destructures that same wrapper at line 221, counts invalid labels if needed, and replaces the extension with active `Labels` at lines 224–227. These are the declaration, producer, and consumer; candidate source search finds no additional `Pending` occurrences in this module. | The field holds the declared classification awaiting successful-authentication activation. |

This is **VIOLATION**, with a mandatory merge-gate consequence; no runtime defect or security failure is alleged. The enclosing type documentation already makes the intended behavior understandable, but does not meet the separately explicit field requirement. Compiler or lint success cannot waive that source-level gate. The task-review skill specifically preserves mandatory changed-item rustdoc gates even where ordinary materiality would reject a style-only observation.

## Smallest correction and closure

Add rustdoc directly to the existing tuple field describing its real role, for example “The declared classification awaiting authenticated activation.” Preserve the type, existing enclosing docs, span-extension representation, producer/consumer bodies, and all behavior. This is one field comment plus conventional formatting in the existing owner file. Deleting the wrapper would conflate pending and active classification and is an unjustified behavioral correction; a new type, helper, test, dependency, or documentation sweep is unnecessary.

Closure is direct source inspection that the tuple field itself carries meaningful rustdoc, with the owning format/lint checks bundled into the behavioral remediation already required by other findings. No wording assertion or runtime rerun is needed for this documentation-only correction. Retain this omission within stable FIND-TASK-004-5; it does not justify a separate style-only round.

Only this narrow claim was inspected. No source edits, runtime/Cargo checks, or broader rustdoc audit were performed; this report makes no whole-task acceptance claim.
