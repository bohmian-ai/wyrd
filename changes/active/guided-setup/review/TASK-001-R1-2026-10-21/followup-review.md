# Focused follow-up: SavedTerminal field rustdoc

**RESOLVED.** The new private tuple field at `crates/wyrd/wyrd-cli/src/terminal.rs:107` lacks the field rustdoc explicitly required by `agent_docs/rust.md`. Its type-level documentation explains the representation and lifetime sufficiently for a reader, but does not satisfy the independently stated field-documentation requirement. Propose retaining MAINT-R1-001 for independent validation; no additional finding is proposed.

## Subject and scope

Base `ebd75dc224854413d17cb06b70da8181b78cde6b`; candidate `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`; latest remediation starts at `00b6148919b9ccd5f958f3b621ab6a89c6372001`. HEAD was checked against the candidate. The cumulative diff was requested, with source portions reread separately after output truncation; investigation is confined to the conflict, not a replacement full acceptance audit. The original task, review subject, skill, root instructions, Rust guide and maintainer guide informed this pass. CodeGraph preceded code navigation. No nested AGENTS applies to the CLI owner.

The conflict is between MAINT-R1-001 in maintainer-review.md and the standards-review.md statement that SavedTerminal/save/restore have meaningful item-level rustdoc. Both claims are partly accurate: the type and methods have meaningful documentation; the tuple field does not.

## Governing evidence

- Root `AGENTS.md:152–154`: every new or changed Rust item has rustdoc, and missing rustdoc blocks merge.
- `agent_docs/rust.md:235–241` makes the scope unambiguous: “Every new or materially modified item needs rustdoc regardless of visibility, including fields, variants, private helpers, test helpers, and tests.” No exception is stated for a private field, transparent wrapper, tuple struct or newtype.
- The same guide's struct example at line 41 explicitly says field docs were elided in the example and are required in code. That example is corroboration, not a demand to imitate its representation.
- The review skill explicitly preserves missing changed-item rustdoc as blocking at lines 174–175, while ordinary placement/wording preferences do not block.

This does not depend on whether Rust formally classifies a field as an item: the guide specifically names fields. Type documentation is attached to the type; it is not a documentation attribute on the tuple field.

## Source and lifetime trace

The full terminal owner was read. `terminal.rs:104–106` explains the saved terminal attributes, restoration after child reaping, and raw-mode recovery. `SavedTerminal::save` at lines 115–117 captures `tcgetattr(io::stdin())` into the sole `Termios` field. `restore` at lines 121–123 passes `&self.0` to `tcsetattr` on standard input and ignores a restoration failure as its method documentation states. These bodies reveal no second meaning or hidden invariant requiring invention.

The complete relevant caller bodies were traced: `DevServer::guide` saves the attributes before spawning the selected agent, retains the snapshot with the child in `Agent`, waits for agent exit or server/UI failure, and calls `Agent::restore` after the agent wait. `Agent::restore` delegates to the same saved snapshot. `DevServer::stop` takes the owned Agent, settles its child, then restores its retained terminal before finishing UI/server settlement. `settle` retains the existing signal/wait/deadline behavior. There is one snapshot producer and one restore implementation. This field-documentation correction would change none of those paths, callers, tests, exports or behavior.

## Actual owner patterns

The closest pattern is `EchoGuard` in the same module: its type-level documentation states restoration intent, while its `saved: Termios` field separately has “Attributes in effect before concealment.” `Terminal.device` likewise has field-specific rustdoc. The adjacent guided `Agent.terminal` field is separately documented.

Tuple syntax itself is supported by repository practice. `crates/shared/wyrd-client/src/auth.rs:132–136::AuthHttp` is a private one-field tuple wrapper with separate field rustdoc explaining its retained redirect-free Client. `crates/wyrd-spec/src/gateway/openai.rs:29–32::GatewayChatCompletionsRequest` also documents its sole tuple field.

There are counterexamples: `PlatformSession(SecretString)` at shared client `platform/handle.rs:35`, and multiple older identifier newtypes, carry type documentation without a tuple-field comment. They establish that the tree is not globally uniform, not an authorized exception to the explicit rule. Root authority expressly states that source implementation does not override governing documents. They do not justify a global documentation sweep, and none is proposed.

## Conflict resolution and smallest correction

**MAINT-R1-001 — proposed VIOLATION, retained for independent validation.** Exact changed location: the new `Termios` field at terminal.rs:107. The type documentation already communicates the field's runtime meaning, so there is no demonstrated runtime defect, public-contract defect or ambiguity to amplify. The concrete consequence is failure of the explicit changed-field rustdoc gate. The standards inventory documents the type and methods but omits this separately governed field.

The smallest correction is a meaningful rustdoc attribute on the existing tuple field identifying it as standard-input attributes captured before the owned agent takes over and used for restoration after settlement. Keep the tuple representation, save/restore behavior and all callers. No new abstraction, renaming, field conversion, test, Cargo run or runtime rerun is warranted; static source inspection closes this exact gap. No new FIND ID is assigned in this discovery report.

The instruction “Keep the rustdoc gate intact, but never create a style-only round” is consistent with this conclusion when read with the skill's explicit exception: naming, placement, wording preferences and equally clear representations cannot create another round; actual absent rustdoc on a changed field remains blocking. Calling the field omission merely style because its type is clear would erase the explicit field requirement and the preserved missing-rustdoc exception. Conversely, that exception grants no authority to demand richer prose elsewhere or to change this clear representation. This report does not decide the final task verdict or prescribe a separate review round; the independent validator must resolve the proposed finding under these authorities.

## Limits

Only this report was written; source and candidate remain unchanged. No Cargo commands or runtime tests were run. This focused finding alleges no terminal runtime risk and makes no claim about unexercised platforms or real paid agents.
