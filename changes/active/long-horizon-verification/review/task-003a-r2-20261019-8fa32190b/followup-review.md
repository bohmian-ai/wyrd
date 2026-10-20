# Focused follow-up: current documentation acceptance

Result: **RESOLVED**. The discovery reports disagree because the behavior report assessed documentation against the earlier simple-group model without carrying revision 20's result-without-input exception into its A9/A10 conclusion. The runtime corrections are not in dispute. One bounded documentation/projection correction remains proposed for independent validation.

## Immutable subject and scope

- Cumulative base: `22335c2c40a5ad24c27bfa0d4686f4da49f922c5`.
- Candidate: `8fa32190b26b11588721e8db44b3eb94139e3739`; HEAD rechecked unchanged.
- Authority: approved revision 20 in `changes/active/long-horizon-verification/spec.md`, original `tasks/TASK-003a-simplify-grouped-verification.md`, AGENTS, agent rules, spec-driven-development and maintainer-style, applicable active grouped architecture.
- Inputs: subject, claims comparison, behavior/invariant/standards/maintainer discovery reports, and root verification record. Reviewed the cumulative source diff for the implicated documents and actual producer/consumer source. CodeGraph was used first for the poller/scheduler/visible-state path.
- This pass resolves only the A9/A10 documentation conflict. It adds no runtime finding, machinery, security mechanism, or broader task. No source edits, builds, tests, or PostgreSQL sessions were performed. Only this report was written.

## Evidence resolving the conflict

| Question | Authority and current evidence | Resolution |
|---|---|---|
| Is documenting the latest exception required, or optional extra explanation? | Original task A9 at line 483 requires current docs to agree and agreed tradeoffs to be documented. Scenario 8 at lines 451–465 requires examples/public surfaces to describe the same model and accepted limits. Approved spec REQ-004 at lines 422–435 requires retry without pinning when a result's input is absent, then a terminal error when PostgreSQL time is more than five minutes past due. Spec closure at 1036–1046 includes documentation against the current revision. | This is an existing acceptance obligation. It does not invent a new documentation feature. |
| Does the runtime actually have the exception? | `groups.rs:156–159` returns `Incomplete` before pinning; `Visible::missing_inputs` at 439–448 detects result executions absent from the input map. `scheduler.rs:479–488` creates `VerificationSnapshotIncomplete` for overdue incomplete reads; 490–491 retains other incomplete reads. `hand_off_groups:382–393` consumes pending and commits the successful terminal handoff in the same transaction. | Yes; this is the shipped failure behavior to describe, already covered by root's focused P1 proof. |
| Does the user guide describe it? | `docs/src/content/docs/understand/how-verification-works.md:176` says due work judges every visible input/result. Lines 178–180 describe visibility limits and owner refusal versus transient failure, but contain no incomplete-input retry/deadline/terminal outcome. | No. A user cannot tell why this particular pending group eventually becomes errored, why no judgment is produced, or why new activity is needed afterward. |
| Does durable architecture describe it? | `architecture/wyrd-design.md:1324` permits nulls for missing inputs/outcomes; 1336–1345 describes snapshot/query deferral and visible-evidence limits with no revision-20 exception. | The broad description needs qualification. Do **not** remove normal nulls: a present input with no result remains valid, and a missing selected slot value can still be null. The invalid whole-input/result-only case is the distinction to explain. |
| Is the new stable error reference generated correctly? | `wyrd-spec/src/error.rs:1930–1949` declares code `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE`, status 409 and remediation. `docs/src/content/docs/api/errors.md` omits it from the generated verification section. `docs/scripts/generate_api_docs.py::error_catalog/render_errors` is the existing source-derived owner. The `docs:check` task at `mise.toml:1294–1301` regenerates then rejects tracked drift. | No. An independent read-only render comparison returned `catalog_matches: False` and exactly one missing row, quoted below. This is objective generated output drift, not a prose preference. |
| Is the Drift-only security-posture sentence another blocker? | `architecture/wyrd-security-posture.md` describes the existing internal read principal in Drift terms; active architecture at `wyrd-design.md:1460–1468` already explicitly includes group snapshots. No contradictory runtime grant or widened tenant read was established by that sentence. | Non-blocking descriptive staleness only. Do not require a separate correction, security redesign, or expanded proof. |

The read-only render difference is:

```text
| `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` | 409 | Verification snapshot incomplete | Check Bifrost publication for the named source executions, then submit a new turn to the group. |
```

This comparison imported the existing generator with `/usr/bin/python3`/`runpy`, called `render_errors()`, and compared its returned string with the committed page; it did not execute the generator's writing entry point or change generated files.

## Bounded proposed finding

### FU-R2-01 — Current grouped failure documentation and generated error reference lag revision 20

- Discovery source IDs: `INV-R2-01`, `MREV-R2-01`, `STANDARDS-1`; one shared correction boundary.
- Classification: **MISSING / public-contract documentation mismatch**.
- Violated obligation: original A9/current-doc parity, its dependent A10 closure claim, scenario 8 source-driven documentation closure, and the approved REQ-004 revision-20 behavior. AGENTS public generated-contract consistency also applies.
- Exact affected consumers: how-verification-works lines 176–180, grouped architecture lines 1324–1345, and generated errors reference's verification section around 502–520.
- Producer-to-consumer consequence: runtime deliberately refuses to pin result-only evidence and may consume pending with the new stable terminal error, while the published behavior explanation promises visible-evidence judgment or otherwise describes retry without this bound; the advertised generated stable error catalog does not identify the resulting code/remediation. This is not a demonstrated runtime bug or a new data-loss finding.
- Minimum correction: amend the existing grouped paragraphs in the guide and active architecture to describe this one approved exception: a visible result without its execution's input pins nothing and retries; when still incomplete more than five minutes past the current due time on PostgreSQL's clock, one terminal errored execution records the named code and missing execution IDs, consumes pending, and runs no evaluation/verdict/Operator; fresh matching activity can schedule again. Preserve normal input-without-result/null-slot handling, exact sources, immutable prior snapshots, and all accepted visibility/crash/backlog/overlap limits. Regenerate the existing consumed error reference with the existing docs generator, committing whatever consumed generated outputs that owner updates. No manual second catalog, new generator, runtime change, persisted-details expansion, or test harness.
- Stored-error accuracy: the existing `VerificationError` readback retains code/message, so documentation must not promise a new persisted `details` object; IDs are already present in the terminal message. No change to that approved implementation decision is needed.
- Focused closure proof: compare only those paragraphs against revision 20/current source; confirm the existing generator renders the committed catalog identically and includes the new row; use the existing docs generation/check lane. Existing runtime proof is sufficient for a documentation-only correction; no new journey or broad runtime suite is justified.

## Reconciliation and limits

Behavior A9/A10's blanket documentation PASS is unsupported for the current revision. Its runtime/SDK observations can remain valid; the current-doc acceptance cells must be reconciled with the confirmed source mismatch. Invariant and maintainer claims are narrowed to qualifying the existing grouped behavior paragraphs rather than forbidding all null inputs or adding publication machinery. Standards' missing generated row is independently reproduced. These are one documentation correction, not three independent runtime findings.

No other runtime review was repeated. No tests/builds/benchmarks were run; no new security or persistent contract decision is proposed. Final finding retention, stable numbering, and verdict remain with the required fresh Ponytail validator and orchestrator.
